// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneSource {
    pub fn fixed_context(&self) -> Option<PaneContext> {
        match self {
            Self::Fixed(source) => Some(source.context()),
            Self::FromContext(_) => None,
        }
    }
}
