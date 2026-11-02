// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Display for ProjectionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSession => f.write_str("projection request names the wrong session"),
            Self::UnsupportedProtocol => {
                f.write_str("projection request uses an unsupported protocol")
            },
            Self::UnsupportedScore {
                received,
                supported,
            } => write!(
                f,
                "projection score version {received} is unsupported; expected {supported}"
            ),
            Self::UnsupportedArrangement => {
                f.write_str("G3 Turnstone endpoint currently accepts Spiral arrangements")
            },
            Self::InvalidScene(reason) => write!(f, "invalid disclosed scene: {reason}"),
            Self::Presentation(reason) => f.write_str(reason),
        }
    }
}
