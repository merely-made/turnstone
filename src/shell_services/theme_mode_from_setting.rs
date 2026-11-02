// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ThemeMode {
    /// Decode the provider's durable vocabulary without exposing provider
    /// types to the shell service seam.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            Some("hc_light") => Self::HcLight,
            Some("hc_dark") => Self::HcDark,
            Some(value) if value.starts_with("custom:") && value.len() > 7 => {
                Self::Custom(value[7..].into())
            },
            _ => Self::System,
        }
    }

    /// The Tabard mode this selects; `System` takes the OS preference.
    pub fn tabard(&self, system_dark: bool) -> tabard::theme::registry::Mode {
        use tabard::theme::registry::Mode;
        match self {
            Self::System if !system_dark => Mode::Light,
            Self::Light => Mode::Light,
            Self::System | Self::Dark => Mode::Dark,
            Self::HcLight => Mode::HcLight,
            Self::HcDark => Mode::HcDark,
            Self::Custom(key) => Mode::Custom(key.clone()),
        }
    }
}
