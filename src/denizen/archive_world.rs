// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Archive a world: move its live file to the archive slot (archive-never-
/// orphan — the move happens BEFORE the bearing node leaves the graph, and a
/// failure aborts the delete). A world with no live file is fine: there is
/// nothing to move, recovery starts it empty as always.
pub fn archive_world(session_dir: &Path, log_id: &str) -> std::io::Result<()> {
    let live = nested_log_path(session_dir, log_id);
    if !live.is_file() {
        return Ok(());
    }
    let archived = archived_world_path(session_dir, log_id);
    if let Some(parent) = archived.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&live, &archived)
}
