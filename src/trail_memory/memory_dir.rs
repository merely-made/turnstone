// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One session's trail-memory directory (under its `sessions/<id>/` dir).
pub fn memory_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("memory")
}
