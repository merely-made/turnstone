// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist one image blob under its digest hex. Best-effort, like the graph:
/// a lost favicon re-fetches, so a write failure is logged, not fatal.
pub fn save_image_blob(data_root: &Path, hex: &str, bytes: &[u8]) {
    if let Err(error) = try_save_image_blob(data_root, hex, bytes) {
        tracing::warn!(%error, "failed to persist an image blob");
    }
}
