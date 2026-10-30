// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Open the store, seed the schema engram, and load the stored corpus. A
/// `None` disables capture until the next `Reopen` — a browser that cannot
/// remember must still browse.
pub(super) fn open_memory(
    dir: &Path,
    capture_library: &crate::place::captured_collection::LocalCaptureLibrary,
) -> Option<(FjallStore, BrowsingMemory)> {
    if let Err(err) = std::fs::create_dir_all(dir) {
        capture_library.unavailable(format!("create session capture directory: {err}"));
        tracing::warn!(%err, dir = %dir.display(), "trail memory: create dir failed; capture disabled until reopen");
        return None;
    }
    let mut store = match FjallStore::open(dir) {
        Ok(store) => store,
        Err(err) => {
            capture_library.unavailable(format!("open session capture store: {err}"));
            tracing::warn!(%err, dir = %dir.display(), "trail memory: open failed; capture disabled until reopen");
            return None;
        },
    };
    if let Err(err) = pollster::block_on(bootstrap_browsing_schema(&mut store)) {
        capture_library.unavailable(format!("bootstrap browsing schema: {err}"));
        tracing::warn!(%err, "trail memory: schema bootstrap failed; capture disabled until reopen");
        return None;
    }
    match pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)) {
        Ok(memory) => {
            if let Err(error) = crate::place::captured_collection::refresh_local_capture_library(
                &mut store,
                dir,
                capture_library,
            ) {
                capture_library.unavailable(error);
            }
            Some((store, memory))
        },
        Err(err) => {
            capture_library.unavailable(format!("load browsing corpus: {err}"));
            tracing::warn!(%err, "trail memory: corpus load failed; capture disabled until reopen");
            None
        },
    }
}
