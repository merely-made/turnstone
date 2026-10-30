// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

// `save_browser_nodes` / `load_browser_nodes` left with the web.* facet
// convergence (2026-07-20): browser state persists as web.* facets in
// facets.json (write_web_states in the save path, read_web_states on adopt).
// Only the legacy read below remains, for pre-convergence profiles.

/// Read a pre-convergence `browser_nodes.json`, if one exists — the one-time
/// legacy absorb on adopt (facet values win; this only seeds unseen nodes).
/// Missing or corrupt reads empty.
pub fn load_legacy_browser_nodes(
    data_root: &Path,
) -> pandect::browser_node_state::BrowserNodeStates {
    match pandect::browser_node_state::load_browser_node_states(data_root) {
        Ok(Some(states)) => states,
        Ok(None) => pandect::browser_node_state::BrowserNodeStates::new(),
        Err(err) => {
            tracing::warn!(%err, "failed to read the legacy browser-state sidecar; ignoring it");
            pandect::browser_node_state::BrowserNodeStates::new()
        },
    }
}
