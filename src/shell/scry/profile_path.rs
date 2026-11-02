// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn profile_path(profile: &str) -> Result<PathBuf, SurfaceError> {
    let path = PathBuf::from(profile);
    if profile.trim().is_empty() || !path.is_absolute() {
        return Err(SurfaceError::SpawnFailed(
            "Scry requires an explicit absolute per-node profile directory".into(),
        ));
    }
    Ok(path)
}
