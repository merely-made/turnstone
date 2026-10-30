// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The persisted product-free score for this session's active analytic view.
pub fn projection_score_path(session_dir: &Path) -> PathBuf {
    session_dir.join(PROJECTION_SCORE_FILE)
}
