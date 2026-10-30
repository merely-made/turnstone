// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn require_owner(owner: ThreadId) -> Result<(), SurfaceError> {
    if std::thread::current().id() == owner {
        Ok(())
    } else {
        Err(spawn_error("Servo operation requires its UI thread"))
    }
}
