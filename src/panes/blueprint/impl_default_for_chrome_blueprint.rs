// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for ChromeBlueprint {
    fn default() -> Self {
        Self {
            omnibar: ChromePlacement::Overlay,
            shellbar: ChromePlacement::Docked(ChromeEdge::Right),
            transcript: ChromePlacement::Hidden,
            status: ChromePlacement::Docked(ChromeEdge::Bottom),
        }
    }
}
