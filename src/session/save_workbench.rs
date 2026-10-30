// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the workbench tiling as platen's canonical `(Arrangement, geometry)`
/// pair (the live `Pane` tree is a derived cache, never serde;
/// `to_persisted_json` debug-asserts `canonical_roundtrips` — platen's
/// persistence discipline, preserved verbatim). Best-effort, like the rest.
pub fn save_workbench(data_root: &Path, workbench: &mere::platen::Workbench) {
    match workbench.to_persisted_json() {
        Ok(json) => {
            let path = data_root.join(WORKBENCH_FILE);
            if let Err(err) = std::fs::write(&path, json) {
                tracing::warn!(%err, path = ?path, "failed to persist the workbench tiling");
            }
        },
        Err(err) => tracing::warn!(%err, "failed to serialize the workbench tiling"),
    }
}
