// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme_id: None,
            theme_mode: ThemeMode::System,
            ui_zoom: 1.1,
            theme_presentation: None,
            theme_typography: Default::default(),
            interface_font_properties: String::new(),
            system_dark: true,
        }
    }
}
