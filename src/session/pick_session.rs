// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The session a boot should open: the recorded current session when it
/// still exists, else the most recently updated manifest, else `None` (a
/// fresh install — the caller mints one).
pub fn pick_session(data_root: &Path, store: &ManifestStore) -> Option<SessionId> {
    let recorded = std::fs::read_to_string(data_root.join(CURRENT_SESSION_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<uuid::Uuid>().ok())
        .map(SessionId::from_uuid)
        .filter(|id| store.get(*id).is_some());
    recorded.or_else(|| {
        store
            .iter()
            .max_by_key(|(_, m)| m.updated_at)
            .map(|(id, _)| id)
    })
}
