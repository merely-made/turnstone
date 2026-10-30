// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Load the manifest set from `sessions/`. Failures are logged per directory
/// (the store's own report), never fatal.
pub fn load_manifests(data_root: &Path) -> ManifestStore {
    let mut store = ManifestStore::new();
    match store.load_from_disk(sessions_root(data_root)) {
        Ok(report) => {
            for failure in &report.failed {
                tracing::warn!(
                    dir = %failure.dir_name,
                    reason = %failure.reason,
                    "a session manifest failed to load"
                );
            }
        },
        Err(err) => tracing::warn!(%err, "failed to read the sessions directory"),
    }
    store
}
