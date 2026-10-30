// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for RedshankHost {
    fn default() -> Self {
        Self {
            model: RedshankModel::default(),
            store: None,
            runtime: OnceLock::new(),
            playback_wake: None,
            retained_voice: Arc::default(),
            fetch: None,
            output: Output::Silent,
            token: 0,
            selected: None,
            resumed_from_ms: None,
            anchors: BTreeMap::new(),
            next_note: 0,
            pending_note_open: None,
            pending_note_seek: None,
            next_note_seek: 0,
            hold_progress: false,
            span_stop_ms: None,
            note_warning: None,
        }
    }
}
