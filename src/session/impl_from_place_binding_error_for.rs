// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<PlaceBindingError> for PlaceSidecarError {
    fn from(error: PlaceBindingError) -> Self {
        Self::Binding(error)
    }
}
