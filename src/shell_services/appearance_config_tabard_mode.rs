// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl AppearanceConfig {
    /// The Tabard mode host surfaces wear, with `System` resolved.
    pub fn tabard_mode(&self) -> tabard::theme::registry::Mode {
        self.theme_mode.tabard(self.system_dark)
    }

    /// A finite, user-setting-safe scale for layout and paint consumers.
    pub fn zoom(&self) -> f32 {
        self.ui_zoom.clamp(0.5, 3.0)
    }
}
