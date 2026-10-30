// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Restore the persisted pane layout (rung 5 slice C), if one exists. The sidecar
/// is `frame.json` beside `graph.json` (the on-disk tag stays `frame`, a parked
/// format decision). `None` starts on the default single-pane layout.
pub fn load_frisket_layout(data_root: &Path) -> Option<FrisketLayout> {
    match frisket_store::load_frisket_layout(data_root) {
        Ok(layout) => layout,
        Err(err) => {
            tracing::warn!(%err, "failed to load the pane layout; starting on the default");
            None
        },
    }
}
