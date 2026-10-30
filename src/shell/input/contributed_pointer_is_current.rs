// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ContributedPointer {
    pub(in super::super) fn is_current(
        &self,
        surface: Option<crate::surface::Surface>,
        generation: Option<u64>,
        window: u64,
    ) -> bool {
        surface == Some(self.surface)
            && generation == Some(self.generation)
            && window == self.window
    }
}
