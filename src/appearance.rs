// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Application appearance is a read-only projection of Tabard's authored
//! library. Turnstone owns selection persistence and maps shared roles onto
//! host chrome; Reader panes keep their own appearance authority.

use std::path::{Path, PathBuf};
use tabard::ResolvedThemeChoice;
use tabard::library::ThemeLibraryStore;
use tabard::theme::{
    choice::ThemeChoice,
    registry::{Mode, THEME_ID_DEFAULT, ThemeRegistry},
};

pub(crate) struct ThemeCatalog {
    pub path: PathBuf,
    pub registry: ThemeRegistry,
    pub error: Option<String>,
}

pub(crate) fn library_path(data_root: &Path) -> PathBuf {
    std::env::var_os("TURNSTONE_THEME_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| dirs::data_local_dir().map(|dir| dir.join("mere/tabard/themes.json")))
        .unwrap_or_else(|| data_root.join("themes.json"))
}

impl ThemeCatalog {
    pub fn load(path: PathBuf) -> Self {
        let mut registry = ThemeRegistry::default();
        let result = ThemeLibraryStore::load(&path).and_then(|store| {
            for theme in store.themes() {
                registry
                    .add_user_theme(theme.clone())
                    .map_err(std::io::Error::other)?;
            }
            Ok(())
        });
        let error = result.err().map(|error| {
            format!("Could not load theme library: {error}. Existing library is preserved.")
        });
        Self {
            path,
            registry,
            error,
        }
    }

    pub fn resolve(&self, choice: &ThemeChoice) -> Result<ResolvedThemeChoice, String> {
        // Mode-only records are real legacy settings. Resolve their presentation
        // through Tabard's default definition without rewriting the stored id.
        let effective = if choice.theme_id.is_empty() {
            ThemeChoice::new(THEME_ID_DEFAULT, choice.theme_mode.clone())
        } else {
            choice.clone()
        };
        let mut resolved = tabard::resolve_theme_choice(&self.registry, &effective)
            .map_err(|error| error.to_string())?;
        resolved.requested = choice.clone();
        Ok(resolved)
    }

    pub fn modes(&self, choice: Option<&ThemeChoice>) -> Vec<Mode> {
        let mut modes = vec![Mode::Light, Mode::Dark, Mode::HcLight, Mode::HcDark];
        let id = choice
            .map(|choice| choice.theme_id.as_str())
            .filter(|id| !id.is_empty())
            .unwrap_or(THEME_ID_DEFAULT);
        if let Some(theme) = self.registry.theme_def(id) {
            modes.extend(
                theme
                    .mode_sheets
                    .keys()
                    .filter_map(|key| Mode::from_key(key))
                    .filter(|mode| matches!(mode, Mode::Custom(_))),
            );
        }
        modes
    }

    pub fn notice(&self, choice: Option<&ThemeChoice>) -> String {
        let mut notices = self.error.iter().cloned().collect::<Vec<_>>();
        if let Some(choice) = choice {
            match self.resolve(choice) {
                Ok(resolved) => {
                    notices.extend(resolved.diagnostics.iter().map(ToString::to_string));
                    if resolved
                        .theme
                        .typography
                        .0
                        .contains_key(&tabard::FontRole::Interface)
                    {
                        let mut fonts = tabard::Typography::default();
                        fonts.0.insert(
                            tabard::FontRole::Interface,
                            resolved.theme.typography.0[&tabard::FontRole::Interface].clone(),
                        );
                        notices.extend(fonts.missing_families(installed_families()));
                    }
                    for role in interface_preferences(&choice.fonts)
                        .resolve(installed_families(), &font_defaults())
                    {
                        if role.used_default {
                            notices.push(format!(
                                "Interface: requested families are unavailable; using {}.",
                                role.family.name()
                            ));
                        }
                    }
                },
                Err(error) => notices.push(error),
            }
        }
        notices.join(" ")
    }

    pub(crate) fn interface_font_properties(&self, fonts: &tabard::FontPreferences) -> String {
        let fonts = interface_preferences(fonts);
        if fonts.is_empty() {
            return String::new();
        }
        crate::frame_style::interface_font_properties(&fonts, installed_families())
    }
}

fn installed_families() -> &'static [String] {
    static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FAMILIES.get_or_init(cambium_genet_winit_host::installed_font_families)
}
fn font_defaults() -> std::collections::BTreeMap<tabard::FontRole, tabard::FontFamily> {
    [(
        tabard::FontRole::Interface,
        tabard::FontFamily::new("sans-serif").unwrap(),
    )]
    .into()
}
fn interface_preferences(fonts: &tabard::FontPreferences) -> tabard::FontPreferences {
    tabard::FontPreferences(
        fonts
            .0
            .iter()
            .filter(|(role, _)| **role == tabard::FontRole::Interface)
            .map(|(&role, families)| (role, families.clone()))
            .collect(),
    )
}
