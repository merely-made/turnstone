// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Display for PlaceSidecarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "place sidecar I/O: {error}"),
            Self::Json(error) => write!(formatter, "place sidecar JSON: {error}"),
            Self::Binding(error) => write!(formatter, "place sidecar binding: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported place-left marker version {version}")
            },
        }
    }
}
