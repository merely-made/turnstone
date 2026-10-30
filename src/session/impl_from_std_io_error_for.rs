// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<std::io::Error> for PlaceSidecarError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
