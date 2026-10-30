// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceMemberAccess {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pull => "pull only",
            Self::Read => "reader",
            Self::Write => "writer",
            Self::Manage => "manager",
        }
    }
}
