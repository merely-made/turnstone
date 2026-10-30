// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Restore the persisted per-node facet store (`facets.json`), if one exists.
/// Carries the `arrangement.position` facets (the durable canvas layout — the
/// graph itself is position-free) plus any other namespace. Missing or corrupt
/// starts empty: the canvas keeps its origin park and settles fresh.
pub fn load_node_facets(data_root: &Path) -> Option<pandect::NodeFacetStore> {
    match pandect::load_node_facets(data_root) {
        Ok(facets) => facets,
        Err(err) => {
            tracing::warn!(%err, "failed to load the facet store; starting empty");
            None
        },
    }
}
