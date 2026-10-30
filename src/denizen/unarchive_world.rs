// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Recover a world: move its archived file back to the live slot. A missing
/// archive is fine (the world had no file, or an older build deleted without
/// archiving) — the resident rebuilds on an empty world.
pub fn unarchive_world(session_dir: &Path, log_id: &str) -> std::io::Result<()> {
    let archived = archived_world_path(session_dir, log_id);
    if !archived.is_file() {
        return Ok(());
    }
    let live = nested_log_path(session_dir, log_id);
    if let Some(parent) = live.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&archived, &live)
}
