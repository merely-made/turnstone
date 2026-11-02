// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Restore the lens-window spaces. Missing or corrupt starts with none (the
/// primary window alone — the panes those windows held are gone with them,
/// honestly, not silently folded into the primary).
pub fn load_lens_spaces(data_root: &Path) -> Vec<Option<FrisketLayout>> {
    match frisket_store::load_lens_spaces(data_root) {
        Ok(Some(lenses)) => lenses,
        Ok(None) => Vec::new(),
        Err(err) => {
            tracing::warn!(%err, "failed to load the lens-window sidecar; starting with none");
            Vec::new()
        },
    }
}
