// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PageZoomChord {
    pub(super) fn for_character(value: &str) -> Option<Self> {
        Some(match value {
            "+" | "=" => Self::In,
            "-" | "_" => Self::Out,
            "0" => Self::Reset,
            _ => return None,
        })
    }

    pub(super) fn action(self, member: uuid::Uuid) -> Action {
        match self {
            Self::In => Action::PageZoomIn { member },
            Self::Out => Action::PageZoomOut { member },
            Self::Reset => Action::PageZoomReset { member },
        }
    }
}
