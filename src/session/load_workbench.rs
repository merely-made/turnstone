// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Restore the workbench tiling, pruned to `present` (the live graph's
/// members, so a tile whose node vanished between sessions collapses away).
/// A missing or corrupt sidecar starts on an empty workbench.
pub fn load_workbench(
    data_root: &Path,
    present: &std::collections::HashSet<uuid::Uuid>,
) -> mere::platen::Workbench {
    let path = data_root.join(WORKBENCH_FILE);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return mere::platen::Workbench::new();
    };
    match mere::platen::Workbench::from_persisted_json(&json, present) {
        Some(wb) => {
            tracing::info!(path = ?path, tiles = wb.tile_count(), "workbench tiling restored");
            wb
        },
        None => {
            tracing::warn!(path = ?path, "failed to parse the workbench sidecar; starting empty");
            mere::platen::Workbench::new()
        },
    }
}
