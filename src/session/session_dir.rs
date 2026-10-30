// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One session's directory: where ALL its sidecars live.
pub fn session_dir(data_root: &Path, id: SessionId) -> PathBuf {
    sessions_root(data_root).join(id.as_uuid().to_string())
}
