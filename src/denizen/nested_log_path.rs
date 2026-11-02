// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Where a participant's nested log persists, beside the session's other state:
/// `sessions/<id>/denizens/<log-id>.json`.
pub fn nested_log_path(session_dir: &Path, log_id: &str) -> PathBuf {
    session_dir.join("denizens").join(format!("{log_id}.json"))
}
