// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Complete a forget: remove the archived world of a purged tombstone
/// (emptying the bin, or athanor's retirement pass). Best-effort — the
/// tombstone is already gone; a leftover file is litter, not data loss.
pub fn purge_archived_world(session_dir: &Path, log_id: &str) {
    let archived = archived_world_path(session_dir, log_id);
    if archived.is_file()
        && let Err(err) = std::fs::remove_file(&archived)
    {
        tracing::warn!(%err, log_id, "failed to purge an archived participant world");
    }
}
