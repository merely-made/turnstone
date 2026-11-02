// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Repair pre-overmap manifests whose `root_graph_id` is nil: mint each a real
/// `GraphId` and flush. The root graph is the session's container node (the
/// one-node model) — its id keys the `scene.*` facets and is the session's
/// identity in the overmap, so nil ids would collide every pre-overmap session
/// onto one overmap node. Returns how many were healed; the caller migrates
/// each healed session's nil-keyed scene facets on adopt
/// (`App::adopt_session`). Idempotent: a healed store has nothing nil.
pub fn heal_nil_graph_ids(sessions: &mut ManifestStore) -> usize {
    let nil: Vec<SessionId> = sessions
        .iter()
        .filter(|(_, m)| m.root_graph_id == crate::panes::GraphId::nil())
        .map(|(id, _)| id)
        .collect();
    for id in &nil {
        sessions.update(*id, |m| m.root_graph_id = crate::panes::GraphId::new());
    }
    if !nil.is_empty() {
        if let Err(err) = sessions.flush_dirty() {
            tracing::warn!(%err, "failed to persist healed session graph ids");
        }
        tracing::info!(
            count = nil.len(),
            "healed nil root_graph_ids (overmap identity)"
        );
    }
    nil.len()
}
