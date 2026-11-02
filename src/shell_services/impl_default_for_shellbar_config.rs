// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for ShellbarConfig {
    fn default() -> Self {
        Self {
            placement: ChromePlacement::Docked(ChromeEdge::Right),
            visible: true,
        }
    }
}
