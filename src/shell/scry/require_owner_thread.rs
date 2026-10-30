// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn require_owner_thread(owner: ThreadId) -> Result<(), SurfaceError> {
    if owner != std::thread::current().id() {
        return Err(SurfaceError::SpawnFailed(
            "Scry producers must be constructed on their factory's UI thread".into(),
        ));
    }
    Ok(())
}
