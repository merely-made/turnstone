// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn spawn_error(stage: &str, error: impl std::fmt::Display) -> SurfaceError {
    SurfaceError::SpawnFailed(format!("Scry could not {stage}: {error}"))
}
