// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One-time migration from the flat single-session layout: when a flat
/// `graph.json` sits at the root and no session holds anything yet, mint a
/// session, MOVE the flat sidecars into its directory, and write its
/// manifest. Returns the minted id when a migration ran. Best-effort per
/// file (a copy that fails logs and stays put — the graph file moving is
/// what the migration is judged by).
pub fn migrate_flat_layout(data_root: &Path, store: &mut ManifestStore) -> Option<SessionId> {
    if !store.is_empty() {
        return None;
    }
    let flat_graph = data_root.join(session_graph_store::GRAPH_FILE);
    if !flat_graph.exists() {
        return None;
    }
    let id = SessionId::new();
    let dir = session_dir(data_root, id);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::warn!(%err, "flat-layout migration could not create the session dir");
        return None;
    }
    for file in SESSION_FILES {
        let from = data_root.join(file);
        if !from.exists() {
            continue;
        }
        if let Err(err) = std::fs::rename(&from, dir.join(file)) {
            tracing::warn!(%err, %file, "flat-layout migration failed to move a sidecar");
            if file == session_graph_store::GRAPH_FILE {
                return None;
            }
        }
    }
    let mut manifest = GraphSessionManifest::new(id, crate::panes::GraphId::nil());
    manifest.storage_path = Some(dir);
    store.insert(manifest);
    if let Err(err) = store.flush_dirty() {
        tracing::warn!(%err, "flat-layout migration failed to write the manifest");
    }
    tracing::info!(session = %id.as_uuid(), "flat single-session layout migrated to sessions/");
    Some(id)
}
