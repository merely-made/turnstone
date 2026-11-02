// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the session graph at the flat `graph.json`. Best-effort: a write
/// failure is logged, not fatal. Run after each enrichment (so a crash loses
/// nothing) and on close.
#[cfg(test)]
pub(crate) fn save_session_graph(data_root: &Path, graph: &Graph) {
    let graph_file = data_root.join(session_graph_store::GRAPH_FILE);
    if let Err(err) = crate::session_graph_file::save(&graph_file, graph) {
        tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
    }
}
