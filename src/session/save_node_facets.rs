// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the per-node facet store at `facets.json`. Best-effort, like the graph.
pub fn save_node_facets(data_root: &Path, facets: &pandect::NodeFacetStore) {
    if let Err(err) = pandect::save_node_facets(data_root, facets) {
        tracing::warn!(%err, "failed to persist the facet store");
    }
}
