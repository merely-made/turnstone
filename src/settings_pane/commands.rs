// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The pane submits commands through its binding and the application owner.

use mere_surface_api::settings::{
    RevisionedSettingsProvider, RevisionedWriteOutcome, SettingValue, SettingsRevisionGuard,
};
use workbench::SettingsRef;

use super::{APPLICATION_REFERENCE, SettingsBinding, SettingsState, bindings_for};

pub(super) fn apply_value(state: &mut SettingsState, setting_id: &str, value: SettingValue) {
    state.live_settings.report_status(None);
    let Some(mut owner) = state.owner.clone() else {
        state.status = "Application settings owner is unavailable.".into();
        return;
    };
    let Some(binding) = state.bindings.get_mut(setting_id) else {
        state.status = "This setting is unavailable. Discard edits and reload to reconnect.".into();
        return;
    };
    let expected = binding.guard().clone();
    // Delivery is synchronous today, so an edit cannot interleave with this
    // command. The binding also supports delayed delivery and edit generations;
    // retained dirty/conflicted draft presentation belongs to the next UI slice.
    let result = (|| {
        let ticket = binding.prepare(uuid::Uuid::new_v4().to_string(), value, 0)?;
        let outcome = binding.execute(&ticket, &mut owner)?;
        let disposition = binding.complete(&ticket, outcome.clone(), 0)?;
        Ok::<_, crate::settings_provider::SettingsBindingError>((outcome, disposition))
    })();
    match result {
        Ok((RevisionedWriteOutcome::Committed(committed), _)) => {
            advance_local_siblings(state, &owner, &expected, &committed.guard);
            state.provider = owner.provider_snapshot();
            if setting_id.starts_with("theme.") {
                state.live_settings.apply_provider(&state.provider);
            } else {
                state.live_settings.publish_provider(&state.provider);
            }
            state.live_settings.report_status(None);
            state.status = "Saved.".into();
        },
        Ok((RevisionedWriteOutcome::Conflict { .. }, _)) => {
            state.status = "Settings changed since this edit began. Your draft was kept. Discard edits and reload before applying again.".into();
        },
        Ok((RevisionedWriteOutcome::Unavailable { reason, .. }, _)) => {
            state.status = format!("Settings are unavailable: {}", reason.message);
        },
        Ok((RevisionedWriteOutcome::Validation { failure, .. }, _)) => {
            state.status = format!("Could not apply setting: {}", failure.message);
        },
        Ok((RevisionedWriteOutcome::Storage { failure, .. }, _)) => {
            state.status = format!("Could not save application settings: {}", failure.message);
        },
        Err(error) => {
            state.status = format!("Could not admit settings write: {error:?}");
        },
    }
}

/// An intentional local commit can advance unchanged sibling baselines that
/// held its exact precondition. It cannot grant a newer guard to an older
/// draft, or hide an intervening durable change. Delivery here is synchronous;
/// pending asynchronous command policy belongs to the retained binding UI.
fn advance_local_siblings(
    state: &mut SettingsState,
    owner: &super::SharedApplicationSettingsOwner,
    expected: &SettingsRevisionGuard,
    committed: &SettingsRevisionGuard,
) {
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let Ok(snapshot) = owner.snapshot(&reference) else {
        return;
    };
    for entry in &snapshot.specs {
        if entry.guard != *committed {
            continue;
        }
        if let Some(binding) = state.bindings.get_mut(&entry.spec.id)
            && binding.guard() == expected
            && binding.needs_snapshot_adoption(&snapshot) == Ok(false)
        {
            // The snapshot was validated by needs_snapshot_adoption. An
            // exhausted generation leaves the old baseline fail-closed.
            let _ = binding.adopt_snapshot(&snapshot, entry.spec.id.clone());
        }
    }
}

/// Refresh values without silently granting an equal-value draft a newer
/// revision. Changed value/control resets the row and adopts its new baseline.
pub(super) fn refresh_settings(state: &mut SettingsState) {
    let Some(owner) = state.owner.clone() else {
        return;
    };
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let result = (|| {
        let snapshot = owner.snapshot(&reference).map_err(|error| error.message)?;
        let replaced_owner = state
            .bindings
            .values()
            .any(|binding| binding.guard().owner != snapshot.owner);
        for entry in &snapshot.specs {
            if let Some(binding) = state.bindings.get_mut(&entry.spec.id) {
                if binding
                    .needs_snapshot_adoption(&snapshot)
                    .map_err(|e| format!("{e:?}"))?
                {
                    binding
                        .adopt_snapshot(&snapshot, entry.spec.id.clone())
                        .map_err(|e| format!("{e:?}"))?;
                }
            } else {
                let binding = SettingsBinding::from_snapshot(&snapshot, entry.spec.id.clone(), 8)
                    .map_err(|e| format!("{e:?}"))?;
                state.bindings.insert(entry.spec.id.clone(), binding);
            }
        }
        state.bindings.retain(|id, binding| {
            let present = snapshot.specs.iter().any(|entry| &entry.spec.id == id);
            if !present {
                binding.dispose();
            }
            present
        });
        if replaced_owner {
            state.reload_epoch = state
                .reload_epoch
                .checked_add(1)
                .ok_or("Settings reload generation exhausted")?;
        }
        state.provider = owner.provider_snapshot();
        state.live_settings.publish_provider(&state.provider);
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        state.status = format!("Could not refresh application settings: {error}");
    }
}

/// Explicit user action: revoke pending tickets and discard retained row edits.
pub(super) fn reload_settings(state: &mut SettingsState) {
    let Some(owner) = state.owner.clone() else {
        state.status = "Application settings owner is unavailable.".into();
        return;
    };
    match bindings_for(&owner) {
        Ok(bindings) => {
            let Some(epoch) = state.reload_epoch.checked_add(1) else {
                state.status = "Settings reload generation exhausted.".into();
                return;
            };
            for binding in state.bindings.values_mut() {
                binding.dispose();
            }
            state.bindings = bindings;
            state.reload_epoch = epoch;
            state.provider = owner.provider_snapshot();
            state.live_settings.publish_provider(&state.provider);
            state.live_settings.report_status(None);
            state.status = "Reloaded settings and discarded edits.".into();
        },
        Err(error) => state.status = format!("Could not reload application settings: {error}"),
    }
}
