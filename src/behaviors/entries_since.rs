// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The entries after `cursor` belonging to the loaded authority context.
/// Focus and the legacy active-canvas cursor cannot supply capture origin.
pub fn entries_since(app: &App, cursor: u64) -> Vec<CommittedEntry> {
    let Some(origin) = app.behavior_origin() else {
        return Vec::new();
    };
    projected_tail(app, origin, cursor)
        .map(|(_, entries)| entries)
        .unwrap_or_default()
}
