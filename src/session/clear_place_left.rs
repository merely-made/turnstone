// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Clear the left mark, so this session reads as ordinarily joined again.
/// Returns `false` when there was none to clear.
pub fn clear_place_left(session_dir: &Path) -> Result<bool, PlaceSidecarError> {
    match std::fs::remove_file(session_dir.join(PLACE_LEFT_FILE)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
