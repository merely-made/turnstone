// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn unsupported(operation: &str) -> SurfaceError {
    SurfaceError::Unsupported(format!("Servo host does not support {operation}"))
}
