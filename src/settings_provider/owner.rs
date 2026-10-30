// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared in-process owner for application settings.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use cambium::reactive_diagnostics::{
    CauseRelation, DiagnosticDetails, DiagnosticKind, Producer, ProducerRef,
};

use mere_surface_api::settings::{
    CommittedSettingsWrite, RevisionedSettingSpec, RevisionedSettingsProvider,
    RevisionedSettingsSnapshot, RevisionedSettingsWriteRequest, RevisionedWriteOutcome,
    SettingValue, SettingsDomainRevision, SettingsError, SettingsOwnerIncarnation,
    SettingsProvider, SettingsRevisionDomain, SettingsRevisionGuard, SettingsStorageFailure,
    SettingsUnavailable, SettingsValidationFailure,
};
use pandect::ApplicationSettings;
use tabard::theme::choice::ThemeChoice;
use workbench::SettingsRef;

use super::{APPLICATION_REFERENCE, ApplicationSettingsProvider};

const APPEARANCE: &str = "appearance-choice";
const SHELL_CHROME: &str = "shell-chrome";
const BEHAVIORS: &str = "behaviors";
const THEME_CATALOG: &str = "theme-catalog";
const DOMAINS: [&str; 4] = [APPEARANCE, SHELL_CHROME, BEHAVIORS, THEME_CATALOG];

/// A shared in-process authority for Turnstone's application settings.
///
/// Clones serialize writes through one `RefCell` and compare revisions before
/// persistence. This guarantee covers callers that participate through these
/// shared-owner clones. The backend file replacement is not cross-process
/// compare-and-swap; another process or a direct writer can race between a
/// durable read and replacement, and the next owner read only observes the
/// resulting state. `provider_snapshot` is a detached projection for existing
/// readers and does not join that write-coordination boundary.
#[derive(Clone)]
pub struct SharedApplicationSettingsOwner {
    inner: Rc<RefCell<OwnerState>>,
}

struct OwnerState {
    provider: ApplicationSettingsProvider,
    incarnation: SettingsOwnerIncarnation,
    revisions: BTreeMap<String, u64>,
    diagnostics: Option<Producer>,
    next_update_id: u64,
    revision_causes: BTreeMap<String, (u64, ProducerRef)>,
}

impl OwnerState {
    fn update_id(&mut self) -> Option<u64> {
        let id = self.next_update_id;
        self.next_update_id = self.next_update_id.checked_add(1)?;
        Some(id)
    }

    fn emit(
        &self,
        kind: DiagnosticKind,
        update_id: Option<u64>,
        revision: Option<u64>,
        cause: Option<(CauseRelation, ProducerRef)>,
    ) -> Option<ProducerRef> {
        self.emit_details(
            kind,
            DiagnosticDetails {
                update_id,
                revision,
                ..DiagnosticDetails::default()
            },
            cause,
        )
    }

    fn emit_details(
        &self,
        kind: DiagnosticKind,
        details: DiagnosticDetails,
        cause: Option<(CauseRelation, ProducerRef)>,
    ) -> Option<ProducerRef> {
        let producer = self.diagnostics.as_ref()?;
        match cause {
            Some(cause) => producer
                .emit_with_causes(kind, None, details, &[cause])
                .ok(),
            None => producer.emit(kind, None, details).ok(),
        }
    }

    fn request(&mut self, domain: &str) -> (Option<u64>, Option<ProducerRef>) {
        let update_id = self.update_id();
        let revision = self.revisions.get(domain).copied();
        let cause = self
            .revision_causes
            .get(domain)
            .map(|(_, reference)| (CauseRelation::TriggeredBy, *reference));
        let reference = self.emit(
            DiagnosticKind::SettingsRequested,
            update_id,
            revision,
            cause,
        );
        (update_id, reference)
    }

    fn request_optional(&mut self, domain: Option<&str>) -> (Option<u64>, Option<ProducerRef>) {
        let update_id = self.update_id();
        let revision = domain.and_then(|domain| self.revisions.get(domain).copied());
        let cause = domain
            .and_then(|domain| self.revision_causes.get(domain))
            .map(|(_, reference)| (CauseRelation::TriggeredBy, *reference));
        let reference = self.emit(
            DiagnosticKind::SettingsRequested,
            update_id,
            revision,
            cause,
        );
        (update_id, reference)
    }

    fn advance_revision(
        &mut self,
        domain: &str,
        revision: u64,
        update_id: Option<u64>,
        commit: Option<ProducerRef>,
    ) {
        let cause = commit.map(|reference| (CauseRelation::CommittedBy, reference));
        if let Some(reference) = self.emit_details(
            DiagnosticKind::RevisionAdvanced,
            DiagnosticDetails {
                update_id,
                revision: Some(revision),
                dependency_id: dependency_id_for_domain(domain),
                ..DiagnosticDetails::default()
            },
            cause,
        ) {
            self.revision_causes
                .insert(domain.to_owned(), (revision, reference));
        }
    }
}

fn dependency_id_for_domain(domain: &str) -> Option<u64> {
    match domain {
        APPEARANCE => Some(1),
        SHELL_CHROME => Some(2),
        BEHAVIORS => Some(3),
        THEME_CATALOG => Some(4),
        _ => None,
    }
}

/// A value-free causal head for one owner revision. Domain names are the
/// owner's fixed bounded vocabulary; setting identities and values are never
/// exposed through this diagnostic projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnerRevisionCause {
    pub domain: String,
    pub revision: u64,
    pub cause: ProducerRef,
}

impl SharedApplicationSettingsOwner {
    /// Create a shared owner around one loaded persistence provider.
    pub fn new(provider: ApplicationSettingsProvider) -> Self {
        let revisions = DOMAINS
            .into_iter()
            .map(|domain| (domain.to_owned(), 0))
            .collect();
        Self {
            inner: Rc::new(RefCell::new(OwnerState {
                provider,
                incarnation: SettingsOwnerIncarnation(uuid::Uuid::new_v4().to_string()),
                revisions,
                diagnostics: Producer::new().ok(),
                next_update_id: 1,
                revision_causes: BTreeMap::new(),
            })),
        }
    }

    /// Return the latest value-free causal head for every changed domain.
    /// Consumers match these to the same coherent snapshot's revisions.
    pub(crate) fn latest_revision_causes(&self) -> Vec<OwnerRevisionCause> {
        self.inner
            .borrow()
            .revision_causes
            .iter()
            .map(|(domain, (revision, cause))| OwnerRevisionCause {
                domain: domain.clone(),
                revision: *revision,
                cause: *cause,
            })
            .collect()
    }

    /// Return the persistence root selected by the application.
    pub fn data_root(&self) -> PathBuf {
        self.inner.borrow().provider.data_root().to_path_buf()
    }

    /// Clone a provider projection for existing read-only consumers.
    pub fn provider_snapshot(&self) -> ApplicationSettingsProvider {
        self.inner.borrow().provider.clone()
    }

    /// Reload authored themes and advance only the catalog revision when its
    /// semantic contents or read status changed.
    pub fn reload_themes(&self) -> Result<(), SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        let previous = state.provider.catalog().clone();
        state.provider.reload_themes();
        if *state.provider.catalog() != previous {
            let next = match next_revision(&state, THEME_CATALOG) {
                Ok(next) => next,
                Err(error) => {
                    state.provider.theme_catalog = previous;
                    return Err(error);
                },
            };
            state.revisions.insert(THEME_CATALOG.into(), next);
            state.advance_revision(THEME_CATALOG, next, None, None);
        }
        Ok(())
    }

    /// Serialized compatibility entry point for the existing theme workshop.
    /// It uses the same in-process owner lock and advances the choice revision
    /// only after the backend persists the choice successfully.
    pub fn apply_theme_choice(&self, choice: ThemeChoice) -> Result<(), SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        let next = next_revision(&state, APPEARANCE)?;
        let (update_id, requested) = state.request(APPEARANCE);
        if let Err(error) = state.provider.apply_theme_choice(choice) {
            state.emit(
                diagnostic_kind_for_error(&error),
                update_id,
                state.revisions.get(APPEARANCE).copied(),
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return Err(error);
        }
        state.revisions.insert(APPEARANCE.into(), next);
        let committed = state.emit(
            DiagnosticKind::SettingsCommitted,
            update_id,
            Some(next),
            requested.map(|reference| (CauseRelation::CommittedBy, reference)),
        );
        state.advance_revision(APPEARANCE, next, update_id, committed);
        Ok(())
    }
}

impl SettingsProvider for SharedApplicationSettingsOwner {
    fn describe(
        &self,
        reference: &SettingsRef,
    ) -> Result<Vec<mere_surface_api::settings::SettingSpec>, SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        state.provider.describe(reference)
    }

    fn apply(
        &mut self,
        reference: &SettingsRef,
        setting_id: &str,
        value: SettingValue,
    ) -> Result<(), SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        let domain = domain_for_setting(setting_id);
        let next = domain
            .map(|domain| next_revision(&state, domain))
            .transpose()?;
        let (update_id, requested) = state.request_optional(domain);
        if let Err(error) = state.provider.apply(reference, setting_id, value) {
            state.emit(
                diagnostic_kind_for_error(&error),
                update_id,
                domain.and_then(|domain| state.revisions.get(domain).copied()),
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return Err(error);
        }
        if let (Some(domain), Some(next)) = (domain, next) {
            state.revisions.insert(domain.into(), next);
            let committed = state.emit(
                DiagnosticKind::SettingsCommitted,
                update_id,
                Some(next),
                requested.map(|reference| (CauseRelation::CommittedBy, reference)),
            );
            state.advance_revision(domain, next, update_id, committed);
        }
        Ok(())
    }
}

impl RevisionedSettingsProvider for SharedApplicationSettingsOwner {
    fn snapshot(
        &self,
        reference: &SettingsRef,
    ) -> Result<RevisionedSettingsSnapshot, SettingsUnavailable> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state).map_err(|error| SettingsUnavailable {
            message: format!("{error:?}"),
        })?;
        if reference.0 != APPLICATION_REFERENCE {
            return Err(SettingsUnavailable {
                message: format!("unsupported settings reference: {}", reference.0),
            });
        }
        let specs = state
            .provider
            .describe(reference)
            .map_err(|error| SettingsUnavailable {
                message: format!("{error:?}"),
            })?
            .into_iter()
            .filter_map(|spec| {
                let domain = domain_for_setting(&spec.id)?;
                Some(RevisionedSettingSpec {
                    spec,
                    guard: guard_for(&state, domain),
                })
            })
            .collect();
        Ok(snapshot_from_state(&state, reference.clone(), specs))
    }

    fn apply_revisioned(
        &mut self,
        request: RevisionedSettingsWriteRequest,
    ) -> RevisionedWriteOutcome {
        let request_id = request.request_id.clone();
        let mut state = self.inner.borrow_mut();
        let requested_domain = domain_for_setting(&request.setting_id);
        if let Err(error) = observe_durable_settings(&mut state) {
            let (update_id, requested) = state.request_optional(requested_domain);
            state.emit(
                DiagnosticKind::SettingsStorage,
                update_id,
                requested_domain.and_then(|domain| state.revisions.get(domain).copied()),
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return RevisionedWriteOutcome::Storage {
                request_id,
                failure: SettingsStorageFailure {
                    message: format!("{error:?}"),
                },
            };
        }
        let (update_id, requested) = state.request_optional(requested_domain);
        if request.reference.0 != APPLICATION_REFERENCE {
            state.emit(
                DiagnosticKind::SettingsUnavailable,
                update_id,
                None,
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return RevisionedWriteOutcome::Unavailable {
                request_id,
                reason: SettingsUnavailable {
                    message: format!("unsupported settings reference: {}", request.reference.0),
                },
            };
        }
        let Some(domain) = domain_for_setting(&request.setting_id) else {
            state.emit(
                DiagnosticKind::SettingsValidation,
                update_id,
                None,
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return RevisionedWriteOutcome::Validation {
                request_id,
                failure: SettingsValidationFailure {
                    setting_id: request.setting_id,
                    message: "unknown setting".into(),
                },
            };
        };
        let current = guard_for(&state, domain);
        if request.expected != current {
            state.emit_details(
                DiagnosticKind::SettingsConflict,
                DiagnosticDetails {
                    update_id,
                    revision: Some(current.revision),
                    dependency_id: dependency_id_for_domain(domain),
                    expected_revision: Some(request.expected.revision),
                    current_revision: Some(current.revision),
                    ..DiagnosticDetails::default()
                },
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return RevisionedWriteOutcome::Conflict {
                request_id,
                current,
            };
        }
        let next = match next_revision(&state, domain) {
            Ok(next) => next,
            Err(error) => {
                state.emit(
                    DiagnosticKind::SettingsStorage,
                    update_id,
                    Some(current.revision),
                    requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
                );
                return RevisionedWriteOutcome::Storage {
                    request_id,
                    failure: SettingsStorageFailure {
                        message: format!("{error:?}"),
                    },
                };
            },
        };
        if let Err(error) = state.provider.apply(
            &request.reference,
            &request.setting_id,
            request.value.clone(),
        ) {
            state.emit(
                diagnostic_kind_for_error(&error),
                update_id,
                Some(current.revision),
                requested.map(|reference| (CauseRelation::TriggeredBy, reference)),
            );
            return map_write_error(request_id, error);
        }
        state.revisions.insert(domain.into(), next);
        let committed_value = state
            .provider
            .describe(&request.reference)
            .ok()
            .and_then(|specs| {
                specs
                    .into_iter()
                    .find(|spec| spec.id == request.setting_id)
                    .map(|spec| spec.value)
            })
            .unwrap_or(request.value);
        let committed = state.emit(
            DiagnosticKind::SettingsCommitted,
            update_id,
            Some(next),
            requested.map(|reference| (CauseRelation::CommittedBy, reference)),
        );
        state.advance_revision(domain, next, update_id, committed);
        RevisionedWriteOutcome::Committed(CommittedSettingsWrite {
            request_id,
            reference: request.reference,
            setting_id: request.setting_id,
            value: committed_value,
            guard: guard_for(&state, domain),
        })
    }
}

fn snapshot_from_state(
    state: &OwnerState,
    reference: SettingsRef,
    specs: Vec<RevisionedSettingSpec>,
) -> RevisionedSettingsSnapshot {
    RevisionedSettingsSnapshot {
        reference,
        owner: state.incarnation.clone(),
        revisions: DOMAINS
            .into_iter()
            .map(|domain| SettingsDomainRevision {
                domain: SettingsRevisionDomain(domain.into()),
                revision: state.revisions[domain],
            })
            .collect(),
        specs,
    }
}

fn guard_for(state: &OwnerState, domain: &str) -> SettingsRevisionGuard {
    SettingsRevisionGuard {
        owner: state.incarnation.clone(),
        domain: SettingsRevisionDomain(domain.into()),
        revision: state.revisions[domain],
    }
}

fn domain_for_setting(setting_id: &str) -> Option<&'static str> {
    if setting_id.starts_with("theme.") {
        Some(APPEARANCE)
    } else if setting_id == "ui.zoom" || setting_id.starts_with("chrome.") {
        Some(SHELL_CHROME)
    } else if setting_id.starts_with("behaviors.") {
        Some(BEHAVIORS)
    } else {
        None
    }
}

/// Read the durable base and fold externally observed axis changes into
/// this in-process owner's cache and revisions before any snapshot or write.
fn observe_durable_settings(state: &mut OwnerState) -> Result<ApplicationSettings, SettingsError> {
    let durable = state.provider.current_write_base()?;
    let cached = state.provider.settings();
    let changed = [
        (APPEARANCE, cached.theme != durable.theme),
        (
            SHELL_CHROME,
            cached.ui_zoom != durable.ui_zoom
                || cached.shellbar_edge != durable.shellbar_edge
                || cached.shellbar_hidden != durable.shellbar_hidden,
        ),
        (BEHAVIORS, cached.cascade_budget != durable.cascade_budget),
    ];
    let mut updates = Vec::new();
    for (domain, differs) in changed {
        if differs {
            updates.push((domain, next_revision(state, domain)?));
        }
    }
    state.provider.settings = durable.clone();
    for (domain, revision) in updates {
        state.revisions.insert(domain.into(), revision);
        state.advance_revision(domain, revision, None, None);
    }
    Ok(durable)
}

fn diagnostic_kind_for_error(error: &SettingsError) -> DiagnosticKind {
    match error {
        SettingsError::UnsupportedReference(_) => DiagnosticKind::SettingsUnavailable,
        SettingsError::UnknownSetting(_) | SettingsError::InvalidValue { .. } => {
            DiagnosticKind::SettingsValidation
        },
        SettingsError::Storage(_) => DiagnosticKind::SettingsStorage,
    }
}

fn next_revision(state: &OwnerState, domain: &str) -> Result<u64, SettingsError> {
    state.revisions[domain]
        .checked_add(1)
        .ok_or_else(|| SettingsError::Storage(format!("settings revision overflow for {domain}")))
}

fn map_write_error(request_id: String, error: SettingsError) -> RevisionedWriteOutcome {
    match error {
        SettingsError::UnsupportedReference(reference) => RevisionedWriteOutcome::Unavailable {
            request_id,
            reason: SettingsUnavailable {
                message: format!("unsupported settings reference: {}", reference.0),
            },
        },
        SettingsError::UnknownSetting(setting_id) => RevisionedWriteOutcome::Validation {
            request_id,
            failure: SettingsValidationFailure {
                setting_id,
                message: "unknown setting".into(),
            },
        },
        SettingsError::InvalidValue {
            setting_id,
            message,
        } => RevisionedWriteOutcome::Validation {
            request_id,
            failure: SettingsValidationFailure {
                setting_id,
                message,
            },
        },
        SettingsError::Storage(message) => RevisionedWriteOutcome::Storage {
            request_id,
            failure: SettingsStorageFailure { message },
        },
    }
}

#[path = "owner_tests.rs"]
#[cfg(test)]
mod tests;
