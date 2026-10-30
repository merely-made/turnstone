// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Where an ARCHIVED world sits while its bearer is in the recycle bin:
/// `sessions/<id>/denizens/archive/<log-id>.json` (the file-level echo of
/// chartulary's `archive/nested/...` slot convention).
pub fn archived_world_path(session_dir: &Path, log_id: &str) -> PathBuf {
    session_dir
        .join("denizens")
        .join("archive")
        .join(format!("{log_id}.json"))
}
