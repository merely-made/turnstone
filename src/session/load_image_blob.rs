// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Read one image blob back, or `None` when it is absent (swept, not yet
/// fetched, or a reference that outlived its blob).
pub fn load_image_blob(data_root: &Path, hex: &str) -> Option<Vec<u8>> {
    std::fs::read(images_dir(data_root).join(hex)).ok()
}
