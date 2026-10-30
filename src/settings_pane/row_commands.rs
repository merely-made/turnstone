// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium::{SettingRowEvent, SettingRowFailureKind, SettingRowPhase, composition::ComponentKey};
use mere_surface_api::settings::{
    RevisionedSettingsSnapshot, RevisionedWriteOutcome, SettingsRevisionGuard,
};

pub(super) fn row_event(state: &mut SettingsState, key: &ComponentKey, event: SettingRowEvent) {
    match event {
        SettingRowEvent::Edited { .. } => {
            if let Some(row) = state.rows.get_mut(key) {
                // Keep the explicitly reviewed source snapshot while the
                // person adjusts their edit. Retry still checks its exact
                // guard against a fresh owner snapshot before any write.
                if row.session.snapshot().phase != SettingRowPhase::ReviewingMine {
                    row.reviewed = None;
                }
            }
        },
        SettingRowEvent::Apply {
            value,
            edit_generation,
        } => apply_instance(state, key, value, edit_generation),
        SettingRowEvent::Discard => discard_instance(state, key),
        SettingRowEvent::ReviewMine => review_instance(state, key),
        SettingRowEvent::RetryMine {
            value,
            edit_generation,
        } => retry_instance(state, key, value, edit_generation),
    }
}

pub(super) fn apply_instance(
    state: &mut SettingsState,
    key: &ComponentKey,
    value: SettingValue,
    generation: u64,
) {
    state.live_settings.report_status(None);
    let Some(row) = state.rows.get(key) else {
        state.status = "This setting is unavailable.".into();
        return;
    };
    let session = row.session.clone();
    let reference = row.reference.clone();
    let setting_id = row.setting_id.clone();
    let current = session.snapshot();
    if current.edit_generation != generation || current.draft_value != value {
        state.status = "The edit changed before Apply. Review it and apply again.".into();
        return;
    }
    let Some(source) = state.sources.get(&reference.0).cloned() else {
        session.set_unavailable("The settings source is unavailable.");
        return;
    };
    if !session.begin_apply(generation) {
        state.status =
            "This edit cannot be applied until its pending operation or conflict is resolved."
                .into();
        return;
    }
    let result = (|| {
        let row = state
            .rows
            .get_mut(key)
            .ok_or("The component was removed.")?;
        let binding = row
            .binding
            .as_mut()
            .ok_or("The setting binding is unavailable.")?;
        let expected = binding.guard().clone();
        let ticket = binding
            .prepare(uuid::Uuid::new_v4().to_string(), value, generation)
            .map_err(|error| format!("{error:?}"))?;
        let outcome = binding
            .execute(&ticket, &mut *source.borrow_mut())
            .map_err(|error| format!("{error:?}"))?;
        binding
            .complete(&ticket, outcome.clone(), session.snapshot().edit_generation)
            .map_err(|error| format!("{error:?}"))?;
        Ok::<_, String>((outcome, expected))
    })();
    match result {
        Ok((RevisionedWriteOutcome::Committed(committed), expected)) => {
            let mut canonical = current.baseline_spec;
            canonical.value = committed.value;
            if session.acknowledge(generation, canonical).is_none() {
                session.fail(
                    SettingRowFailureKind::Validation,
                    "The response did not match this pending edit.",
                );
                state.status = "Could not admit the settings acknowledgement.".into();
                return;
            }
            if let Some(row) = state.rows.get_mut(key) {
                row.reviewed = None;
            }
            if reference.0 == APPLICATION_REFERENCE {
                if let Some(owner) = &state.owner {
                    state.provider = owner.provider_snapshot();
                }
                if setting_id.starts_with("theme.") {
                    state.live_settings.apply_provider(&state.provider);
                } else {
                    state.live_settings.publish_provider(&state.provider);
                }
            }
            source_sync::refresh_settings(state);
            if let Some(snapshot) = state.snapshots.get(&reference.0).cloned() {
                advance_local_siblings(
                    state,
                    key,
                    &reference,
                    &expected,
                    &committed.guard,
                    &snapshot,
                );
            }
            state.status = "Saved.".into();
        },
        Ok((RevisionedWriteOutcome::Conflict { .. }, _)) => {
            if let Ok(snapshot) = source_sync::refresh_row_snapshot(state, key) {
                if let Some(spec) = source_sync::current_spec(&snapshot, &setting_id) {
                    session.observe(spec.clone());
                }
            }
            session.fail(SettingRowFailureKind::Conflict, "Settings changed since this edit began. Use the current value, or review your edit before retrying.");
            state.status = "Settings changed since this edit began. Your draft was kept.".into();
        },
        Ok((RevisionedWriteOutcome::Unavailable { reason, .. }, _)) => {
            // This is a completed owner response, rather than an availability
            // observation while an independent operation is still pending.
            session.fail(SettingRowFailureKind::Unavailable, reason.message.clone());
            state.status = format!("Settings are unavailable: {}", reason.message);
        },
        Ok((RevisionedWriteOutcome::Validation { failure, .. }, _)) => {
            state.status = format!("Could not apply setting: {}", failure.message);
            session.fail(SettingRowFailureKind::Validation, failure.message);
        },
        Ok((RevisionedWriteOutcome::Storage { failure, .. }, _)) => {
            state.status = format!("Could not save application settings: {}", failure.message);
            session.fail(SettingRowFailureKind::Persistence, failure.message);
        },
        Err(error) => {
            state.status = format!("Could not admit settings write: {error}");
            session.fail(
                SettingRowFailureKind::Validation,
                "The settings response or binding was invalid. Your draft was retained.",
            );
        },
    }
}

fn advance_local_siblings(
    state: &mut SettingsState,
    applied: &ComponentKey,
    reference: &SettingsRef,
    expected: &SettingsRevisionGuard,
    committed: &SettingsRevisionGuard,
    snapshot: &RevisionedSettingsSnapshot,
) {
    for (key, row) in &mut state.rows {
        if key == applied
            || row.reference != *reference
            || row.session.snapshot().pending_generation.is_some()
        {
            continue;
        }
        let Some(entry) = snapshot
            .specs
            .iter()
            .find(|entry| entry.spec.id == row.setting_id)
        else {
            continue;
        };
        let Some(binding) = &mut row.binding else {
            continue;
        };
        if entry.guard == *committed
            && binding.guard() == expected
            && binding.needs_snapshot_adoption(snapshot) == Ok(false)
        {
            let _ = binding.adopt_snapshot(snapshot, row.setting_id.clone());
        }
    }
}

pub(super) fn discard_instance(state: &mut SettingsState, key: &ComponentKey) {
    let Some(row) = state.rows.get(key) else {
        return;
    };
    let session = row.session.clone();
    if session.snapshot().pending_generation.is_some() {
        state.status = "Wait for the pending Apply before discarding this edit.".into();
        return;
    }
    match source_sync::refresh_row_snapshot(state, key) {
        Ok(snapshot) => {
            let row = state
                .rows
                .get_mut(key)
                .expect("the synchronous row still exists");
            let spec = source_sync::current_spec(&snapshot, &row.setting_id)
                .expect("validated setting")
                .clone();
            let binding = SettingsBinding::from_snapshot(&snapshot, row.setting_id.clone(), 8);
            match binding {
                Ok(binding) => {
                    if let Some(previous) = &mut row.binding {
                        previous.dispose();
                    }
                    row.binding = Some(binding);
                    row.reviewed = None;
                    session.reset(spec);
                    state.status = "Discarded edit and adopted the current value.".into();
                },
                Err(error) => {
                    session.set_unavailable(format!("Could not revalidate the setting: {error:?}"))
                },
            }
        },
        Err(reason) => {
            let row = state
                .rows
                .get_mut(key)
                .expect("the synchronous row still exists");
            if let Some(binding) = &mut row.binding {
                binding.dispose();
            }
            row.binding = None;
            row.reviewed = None;
            session.reset_to_observed();
            session.set_unavailable(reason);
        },
    }
}

fn review_instance(state: &mut SettingsState, key: &ComponentKey) {
    let Some(row) = state.rows.get(key) else {
        return;
    };
    let session = row.session.clone();
    if session.snapshot().phase != SettingRowPhase::Conflict {
        return;
    }
    match source_sync::refresh_row_snapshot(state, key) {
        Ok(snapshot) => {
            let row = state
                .rows
                .get_mut(key)
                .expect("the synchronous row still exists");
            if row
                .binding
                .as_ref()
                .is_none_or(|binding| binding.guard().owner != snapshot.owner)
            {
                session.set_unavailable("The source owner was replaced. Discard to revalidate it.");
                return;
            }
            let spec =
                source_sync::current_spec(&snapshot, &row.setting_id).expect("validated setting");
            session.observe(spec.clone());
            if session.review_mine() {
                row.reviewed = Some(snapshot);
                state.status =
                    "Review the current value and your edit, then choose Apply my edit.".into();
            } else {
                session.set_unavailable("The control changed. Discard to use the current control.");
            }
        },
        Err(reason) => session.set_unavailable(reason),
    }
}

fn retry_instance(
    state: &mut SettingsState,
    key: &ComponentKey,
    value: SettingValue,
    generation: u64,
) {
    let Some(row) = state.rows.get(key) else {
        return;
    };
    let session = row.session.clone();
    if session.snapshot().phase == SettingRowPhase::Failed {
        // A deliberate retry after a storage/validation failure keeps the
        // original guard. Only conflict review can adopt a newer guard.
        apply_instance(state, key, value, generation);
        return;
    }
    if session.snapshot().phase != SettingRowPhase::ReviewingMine {
        return;
    }
    let Some(reviewed) = row.reviewed.clone() else {
        return;
    };
    let current = match source_sync::refresh_row_snapshot(state, key) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            session.set_unavailable(error);
            return;
        },
    };
    let row = state
        .rows
        .get_mut(key)
        .expect("the synchronous row still exists");
    let reviewed_guard = reviewed
        .specs
        .iter()
        .find(|entry| entry.spec.id == row.setting_id)
        .map(|entry| &entry.guard);
    let current_guard = current
        .specs
        .iter()
        .find(|entry| entry.spec.id == row.setting_id)
        .map(|entry| &entry.guard);
    if reviewed.reference != current.reference
        || reviewed.owner != current.owner
        || reviewed_guard != current_guard
    {
        row.reviewed = None;
        if let Some(spec) = source_sync::current_spec(&current, &row.setting_id) {
            session.observe(spec.clone());
        }
        session.fail(
            SettingRowFailureKind::Conflict,
            "The setting changed again. Review the latest value before retrying.",
        );
        return;
    }
    let result = row
        .binding
        .as_mut()
        .ok_or("The binding is unavailable.")
        .and_then(|binding| {
            binding
                .adopt_snapshot(&current, row.setting_id.clone())
                .map_err(|_| "The fresh binding could not be admitted.")
        });
    if let Err(reason) = result {
        session.set_unavailable(reason);
        return;
    }
    row.reviewed = None;
    apply_instance(state, key, value, generation);
}
