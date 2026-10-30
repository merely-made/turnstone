// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn view_intent_path(session_dir: &Path) -> PathBuf {
    session_dir.join(VIEW_INTENT_FILE)
}
