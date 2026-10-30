// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Admission boundary for delayed writes from one settings client binding.
//!
//! This module holds request identity and revision guards, not component draft
//! state, persistence, or scheduling. A UI client keeps its own edit and may
//! use the returned disposition to decide whether an acknowledgement cleans it.

use std::collections::HashMap;

use mere_surface_api::settings::{
    CommittedSettingsWrite, RevisionedSettingSpec, RevisionedSettingsProvider,
    RevisionedSettingsSnapshot, RevisionedSettingsWriteRequest, RevisionedWriteOutcome,
    SettingControl, SettingSpec, SettingValue, SettingsOwnerIncarnation, SettingsRevisionGuard,
};
use workbench::SettingsRef;

/// Why a binding or request could not be admitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsBindingError {
    Disposed,
    InvalidSnapshot(&'static str),
    MissingSetting(String),
    DuplicateSetting(String),
    EmptyRequestId,
    DuplicateRequestId(String),
    PendingLimit,
    StaleTicket,
    GenerationExhausted,
    AlreadyExecuted,
    UnknownTicket,
    MalformedOutcome,
}

/// Whether an admitted successful acknowledgement still matches the caller's
/// latest edit generation. Failed writes always return [`KeepDraft`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteDisposition {
    Clean,
    KeepNewerDraft,
    KeepDraft,
}

/// Opaque ticket for one prepared command. Its target and expected guard
/// cannot be changed by the caller after preparation.
#[derive(Debug)]
pub struct SettingsWriteTicket {
    instance: uuid::Uuid,
    generation: u64,
    token: uuid::Uuid,
    request_id: String,
}

impl SettingsWriteTicket {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

struct PendingWrite {
    token: uuid::Uuid,
    expected: SettingsRevisionGuard,
    value: SettingValue,
    edit_generation: u64,
    executed: bool,
}

/// Product-owned admission state for one setting binding.
pub struct SettingsBinding {
    instance: uuid::Uuid,
    generation: u64,
    reference: SettingsRef,
    setting_id: String,
    owner: SettingsOwnerIncarnation,
    baseline_spec: SettingSpec,
    guard: SettingsRevisionGuard,
    max_pending: usize,
    pending: HashMap<String, PendingWrite>,
    disposed: bool,
}

impl SettingsBinding {
    /// Bind to one setting in a coherent owner snapshot. A fresh UUID marks
    /// this client's lifetime and is generated only here, at construction.
    pub fn from_snapshot(
        snapshot: &RevisionedSettingsSnapshot,
        setting_id: impl Into<String>,
        max_pending: usize,
    ) -> Result<Self, SettingsBindingError> {
        if max_pending == 0 {
            return Err(SettingsBindingError::PendingLimit);
        }
        let setting_id = setting_id.into();
        let entry = selected_spec(snapshot, &setting_id)?;
        Ok(Self {
            instance: uuid::Uuid::new_v4(),
            generation: 0,
            reference: snapshot.reference.clone(),
            setting_id,
            owner: snapshot.owner.clone(),
            baseline_spec: entry.spec.clone(),
            guard: entry.guard.clone(),
            max_pending,
            pending: HashMap::new(),
            disposed: false,
        })
    }

    /// Prepare an independent command against the currently adopted guard.
    /// Multiple commands may be pending; each retains its own payload and
    /// guard rather than replacing an earlier request.
    pub fn prepare(
        &mut self,
        request_id: impl Into<String>,
        value: SettingValue,
        edit_generation: u64,
    ) -> Result<SettingsWriteTicket, SettingsBindingError> {
        self.ensure_live()?;
        let request_id = request_id.into();
        if request_id.is_empty() {
            return Err(SettingsBindingError::EmptyRequestId);
        }
        if self.pending.contains_key(&request_id) {
            return Err(SettingsBindingError::DuplicateRequestId(request_id));
        }
        if self.pending.len() >= self.max_pending {
            return Err(SettingsBindingError::PendingLimit);
        }
        self.pending.insert(
            request_id.clone(),
            PendingWrite {
                token: uuid::Uuid::new_v4(),
                expected: self.guard.clone(),
                value,
                edit_generation,
                executed: false,
            },
        );
        Ok(SettingsWriteTicket {
            instance: self.instance,
            generation: self.generation,
            token: self.pending[&request_id].token,
            request_id,
        })
    }

    /// Deliver a ticket once, after verifying it still belongs to this live
    /// binding generation. A rebind or disposal refuses it without calling
    /// the provider.
    pub fn execute(
        &mut self,
        ticket: &SettingsWriteTicket,
        provider: &mut dyn RevisionedSettingsProvider,
    ) -> Result<RevisionedWriteOutcome, SettingsBindingError> {
        let request = self.request(ticket)?;
        let pending = self
            .pending
            .get_mut(&ticket.request_id)
            .ok_or(SettingsBindingError::UnknownTicket)?;
        if pending.executed {
            return Err(SettingsBindingError::AlreadyExecuted);
        }
        pending.executed = true;
        Ok(provider.apply_revisioned(request))
    }

    /// Read the immutable request represented by a live ticket without
    /// delivering it. Useful for logging and typed conflict presentation.
    pub fn request(
        &self,
        ticket: &SettingsWriteTicket,
    ) -> Result<RevisionedSettingsWriteRequest, SettingsBindingError> {
        self.validate_ticket(ticket)?;
        let pending = self
            .pending
            .get(&ticket.request_id)
            .ok_or(SettingsBindingError::UnknownTicket)?;
        Ok(RevisionedSettingsWriteRequest {
            request_id: ticket.request_id.clone(),
            reference: self.reference.clone(),
            setting_id: self.setting_id.clone(),
            expected: pending.expected.clone(),
            value: pending.value.clone(),
        })
    }

    /// Admit one correlated owner response. Only a well-formed commit advances
    /// the baseline; failures and conflicts leave it unchanged until an
    /// explicit fresh snapshot is adopted.
    pub fn complete(
        &mut self,
        ticket: &SettingsWriteTicket,
        outcome: RevisionedWriteOutcome,
        current_edit_generation: u64,
    ) -> Result<WriteDisposition, SettingsBindingError> {
        self.validate_ticket(ticket)?;
        let pending = self
            .pending
            .get(&ticket.request_id)
            .ok_or(SettingsBindingError::UnknownTicket)?;
        if !pending.executed {
            return Err(SettingsBindingError::MalformedOutcome);
        }
        let captured = pending;
        let disposition = match outcome {
            RevisionedWriteOutcome::Committed(committed) => {
                validate_commit(
                    &committed,
                    &ticket.request_id,
                    &self.reference,
                    &self.setting_id,
                    &self.owner,
                    &captured.expected,
                    &self.baseline_spec.control,
                )?;
                // Responses can arrive out of order. A valid older commit may
                // close its own pending ticket, but it must not roll the
                // client's already-adopted baseline backwards or replace the
                // newer committed value.
                if committed.guard.revision <= self.guard.revision {
                    WriteDisposition::KeepDraft
                } else if current_edit_generation == captured.edit_generation {
                    self.guard = committed.guard;
                    self.baseline_spec.value = committed.value;
                    WriteDisposition::Clean
                } else {
                    self.guard = committed.guard;
                    self.baseline_spec.value = committed.value;
                    WriteDisposition::KeepNewerDraft
                }
            },
            RevisionedWriteOutcome::Conflict {
                request_id,
                current,
            } => {
                if request_id != ticket.request_id
                    || current.domain != captured.expected.domain
                    || current.owner.0.is_empty()
                    || (current.owner == self.owner
                        && current.revision <= captured.expected.revision)
                {
                    return Err(SettingsBindingError::MalformedOutcome);
                }
                WriteDisposition::KeepDraft
            },
            RevisionedWriteOutcome::Unavailable { request_id, .. }
            | RevisionedWriteOutcome::Storage { request_id, .. } => {
                if request_id != ticket.request_id {
                    return Err(SettingsBindingError::MalformedOutcome);
                }
                WriteDisposition::KeepDraft
            },
            RevisionedWriteOutcome::Validation {
                request_id,
                failure,
            } => {
                if request_id != ticket.request_id || failure.setting_id != self.setting_id {
                    return Err(SettingsBindingError::MalformedOutcome);
                }
                WriteDisposition::KeepDraft
            },
        };
        self.pending.remove(&ticket.request_id);
        Ok(disposition)
    }

    /// Explicitly adopt a fresh coherent snapshot, revoking all outstanding
    /// tickets even when the setting's id remains the same.
    pub fn adopt_snapshot(
        &mut self,
        snapshot: &RevisionedSettingsSnapshot,
        setting_id: impl Into<String>,
    ) -> Result<(), SettingsBindingError> {
        self.ensure_live()?;
        let setting_id = setting_id.into();
        let entry = selected_spec(snapshot, &setting_id)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(SettingsBindingError::GenerationExhausted)?;
        self.reference = snapshot.reference.clone();
        self.setting_id = setting_id;
        self.owner = snapshot.owner.clone();
        self.baseline_spec = entry.spec.clone();
        self.guard = entry.guard.clone();
        self.pending.clear();
        Ok(())
    }

    /// Permanently invalidate this client binding and all its pending tickets.
    pub fn dispose(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.pending.clear();
        self.disposed = true;
    }

    pub fn baseline_guard(&self) -> &SettingsRevisionGuard {
        &self.guard
    }

    pub fn guard(&self) -> &SettingsRevisionGuard {
        &self.guard
    }

    pub fn setting_id(&self) -> &str {
        &self.setting_id
    }

    pub fn reference(&self) -> &SettingsRef {
        &self.reference
    }

    /// Whether this snapshot should replace the current binding baseline.
    /// Label-only and same-value domain-revision changes deliberately return
    /// false so the caller can preserve a local draft and its older guard.
    pub fn needs_snapshot_adoption(
        &self,
        snapshot: &RevisionedSettingsSnapshot,
    ) -> Result<bool, SettingsBindingError> {
        self.ensure_live()?;
        validate_snapshot(snapshot)?;
        if snapshot.reference != self.reference || snapshot.owner != self.owner {
            return Ok(true);
        }
        let entry = selected_spec(snapshot, &self.setting_id)?;
        Ok(entry.spec.id != self.baseline_spec.id
            || entry.spec.value != self.baseline_spec.value
            || entry.spec.control != self.baseline_spec.control)
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    fn ensure_live(&self) -> Result<(), SettingsBindingError> {
        if self.disposed {
            Err(SettingsBindingError::Disposed)
        } else {
            Ok(())
        }
    }

    fn validate_ticket(&self, ticket: &SettingsWriteTicket) -> Result<(), SettingsBindingError> {
        self.ensure_live()?;
        if ticket.instance != self.instance || ticket.generation != self.generation {
            return Err(SettingsBindingError::StaleTicket);
        }
        match self.pending.get(&ticket.request_id) {
            Some(pending) if pending.token == ticket.token => Ok(()),
            Some(_) => Err(SettingsBindingError::StaleTicket),
            None => Err(SettingsBindingError::UnknownTicket),
        }
    }
}

fn validate_snapshot(snapshot: &RevisionedSettingsSnapshot) -> Result<(), SettingsBindingError> {
    if snapshot.owner.0.is_empty() {
        return Err(SettingsBindingError::InvalidSnapshot(
            "owner incarnation is empty",
        ));
    }
    let mut revisions = HashMap::new();
    for revision in &snapshot.revisions {
        if revision.domain.0.is_empty() {
            return Err(SettingsBindingError::InvalidSnapshot(
                "revision domain is empty",
            ));
        }
        if revisions
            .insert(revision.domain.clone(), revision.revision)
            .is_some()
        {
            return Err(SettingsBindingError::InvalidSnapshot(
                "duplicate domain revision",
            ));
        }
    }
    for entry in &snapshot.specs {
        if entry.guard.domain.0.is_empty() {
            return Err(SettingsBindingError::InvalidSnapshot(
                "setting guard domain is empty",
            ));
        }
        let Some(revision) = revisions.get(&entry.guard.domain) else {
            return Err(SettingsBindingError::InvalidSnapshot(
                "setting guard domain is missing from revision vector",
            ));
        };
        if entry.guard.owner != snapshot.owner || *revision != entry.guard.revision {
            return Err(SettingsBindingError::InvalidSnapshot(
                "setting guard disagrees with snapshot owner or revision vector",
            ));
        }
    }
    Ok(())
}

fn selected_spec<'a>(
    snapshot: &'a RevisionedSettingsSnapshot,
    setting_id: &str,
) -> Result<&'a RevisionedSettingSpec, SettingsBindingError> {
    validate_snapshot(snapshot)?;
    let mut selected = snapshot
        .specs
        .iter()
        .filter(|entry| entry.spec.id == setting_id);
    let Some(entry) = selected.next() else {
        return Err(SettingsBindingError::MissingSetting(setting_id.into()));
    };
    if selected.next().is_some() {
        return Err(SettingsBindingError::DuplicateSetting(setting_id.into()));
    }
    Ok(entry)
}

fn validate_commit(
    committed: &CommittedSettingsWrite,
    request_id: &str,
    reference: &SettingsRef,
    setting_id: &str,
    owner: &SettingsOwnerIncarnation,
    expected: &SettingsRevisionGuard,
    control: &SettingControl,
) -> Result<(), SettingsBindingError> {
    let value_matches_control = match (control, &committed.value) {
        (SettingControl::Toggle, SettingValue::Boolean(_)) => true,
        (SettingControl::Choice { options }, SettingValue::Text(value)) => {
            options.iter().any(|option| option.value == *value)
        },
        (SettingControl::Number { min, max, .. }, SettingValue::Number(value)) => {
            value.is_finite()
                && min.is_none_or(|min| *value >= min)
                && max.is_none_or(|max| *value <= max)
        },
        (SettingControl::Text, SettingValue::Text(_)) => true,
        _ => false,
    };
    if committed.request_id != request_id
        || &committed.reference != reference
        || committed.setting_id != setting_id
        || !value_matches_control
        || committed.guard.owner != *owner
        || committed.guard.domain != expected.domain
        || committed.guard.revision <= expected.revision
    {
        return Err(SettingsBindingError::MalformedOutcome);
    }
    Ok(())
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
