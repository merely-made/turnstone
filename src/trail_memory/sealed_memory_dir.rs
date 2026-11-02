// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn sealed_memory_dir(memory: &Path) -> PathBuf {
    let mut dir = memory.as_os_str().to_os_string();
    dir.push(".sealed-sources-v1");
    PathBuf::from(dir)
}
