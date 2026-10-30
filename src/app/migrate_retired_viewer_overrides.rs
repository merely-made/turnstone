// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Upgrade all legacy `genet.web` pins in one loaded session. Returns whether
/// the caller should persist the changed facet state.
pub(super) fn migrate_retired_viewer_overrides(
    states: &mut pandect::browser_node_state::BrowserNodeStates,
) -> bool {
    let mut migrated = false;
    for state in states.nodes.values_mut() {
        if state.viewer_override.as_deref() == Some(inker::routing::ENGINE_GENET_WEB) {
            state.viewer_override = Some(inker::routing::ENGINE_GENET_LIVERY.to_string());
            migrated = true;
        }
    }
    migrated
}
