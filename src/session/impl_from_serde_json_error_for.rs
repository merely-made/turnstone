// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<serde_json::Error> for PlaceSidecarError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
