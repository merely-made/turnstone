// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's suggestion settings, saved in the browser for this host
//! (graph grammar, DECISIONS 2026-10-16): off by default, the person's to
//! change. A saved value overlays the host defaults field by field, so a
//! setting added later takes its default.

use mere::canvas::Canvas;
use mere::canvas::controls::CanvasCommand;
use scenograph::suggestion::SuggestionSettings;

pub(crate) const SUGGESTIONS_STORAGE_KEY: &str = "graphshell.suggestions";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The host defaults, overlaid by whatever this browser saved.
pub(crate) fn load() -> SuggestionSettings {
    let defaults = SuggestionSettings::for_host("graphshell");
    let saved = storage()
        .and_then(|store| store.get_item(SUGGESTIONS_STORAGE_KEY).ok().flatten())
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());
    let Some(serde_json::Value::Object(saved)) = saved else {
        return defaults;
    };
    let mut merged = serde_json::to_value(&defaults).expect("settings serialize");
    if let serde_json::Value::Object(fields) = &mut merged {
        fields.extend(saved);
    }
    serde_json::from_value(merged).unwrap_or(defaults)
}

pub(crate) fn save(settings: &SuggestionSettings) {
    if let (Some(store), Ok(json)) = (storage(), serde_json::to_string(settings)) {
        let _ = store.set_item(SUGGESTIONS_STORAGE_KEY, &json);
    }
}

/// Save the settings after a command that changed them.
pub(crate) fn after_command(command: CanvasCommand, canvas: &Canvas) {
    if command == CanvasCommand::ToggleSuggestions {
        save(canvas.suggestion_settings());
    }
}
