// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Existing host fixtures address a setting id; actual rows address their
/// independent durable component key and retain their own revision guard.
pub(super) fn apply_value(state: &mut SettingsState, setting_id: &str, value: SettingValue) {
    let default = composition::component_key(setting_id);
    let key = state
        .rows
        .contains_key(&default)
        .then_some(default)
        .or_else(|| {
            state
                .rows
                .iter()
                .find(|(_, row)| {
                    row.reference.0 == APPLICATION_REFERENCE && row.setting_id == setting_id
                })
                .map(|(key, _)| key.clone())
        });
    if let Some(key) = key {
        let session = state.rows[&key].session.clone();
        session.record_edit(value.clone());
        row_commands::apply_instance(state, &key, value, session.snapshot().edit_generation);
    } else {
        state.status = "This setting is unavailable.".into();
    }
}

pub(super) fn refresh_settings(state: &mut SettingsState) {
    source_sync::refresh_settings(state);
}

pub(super) fn reload_settings(state: &mut SettingsState) {
    source_sync::refresh_settings(state);
    let keys = state.rows.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        row_commands::discard_instance(state, &key);
    }
    state.live_settings.report_status(None);
    state.status = "Reloaded settings and discarded edits.".into();
}
