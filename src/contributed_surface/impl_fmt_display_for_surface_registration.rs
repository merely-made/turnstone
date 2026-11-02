// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for SurfaceRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate {
                pane_kind,
                surface_id,
            } => write!(
                f,
                "duplicate surface '{}' for pane '{}'",
                surface_id.as_str(),
                pane_kind.as_str()
            ),
            Self::SourceShapeMismatch {
                surface_id,
                declared,
                schema,
            } => write!(
                f,
                "surface '{}' declares accepted source {:?} but admits schema '{}'",
                surface_id.as_str(),
                declared,
                schema.as_str()
            ),
        }
    }
}
