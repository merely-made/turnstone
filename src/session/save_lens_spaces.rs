// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the lens-window spaces at `windows.json` (rung 7 depth: windows
/// are pane hosts, so torn-out panes survive a restart AS windows). Closed
/// slots persist as `null`, keeping ordinals stable. Best-effort, like the
/// rest.
pub fn save_lens_spaces(data_root: &Path, lenses: &[Option<FrisketLayout>]) {
    if let Err(err) = frisket_store::save_lens_spaces(data_root, lenses) {
        tracing::warn!(%err, "failed to persist the lens windows");
    }
}
