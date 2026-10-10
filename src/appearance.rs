// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Application appearance is a read-only projection of Tabard's authored
//! library. Turnstone owns selection persistence and maps shared roles onto
//! host chrome; Reader panes keep their own appearance authority.

use std::path::{Path, PathBuf};
use tabard::library::ThemeLibraryStore;
use tabard::theme::{
    choice::ThemeChoice,
    registry::{Mode, THEME_ID_DEFAULT, ThemeRegistry},
};
use tabard::{ResolvedThemeChoice, ThemePresentation};

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
                    notices.extend(resolved.diagnostics.iter().map(ToString::to_string))
                },
                Err(error) => notices.push(error),
            }
        }
        notices.join(" ")
    }
}

/// Supply shared variables, then exact authored rules. Arbitrary CSS remains
/// CSS in Genet's existing cascade; seed colors do not reinterpret it.
pub(crate) fn presentation_css(presentation: &ThemePresentation) -> String {
    match presentation {
        ThemePresentation::Derived(palette) => palette.css_custom_properties(),
        ThemePresentation::AuthoredStylesheet(rules) => {
            let fallback =
                tabard::resolve_theme_choice(&ThemeRegistry::default(), &ThemeChoice::default())
                    .expect("valid Tabard built-in");
            let ThemePresentation::Derived(palette) = fallback.presentation else {
                unreachable!()
            };
            format!("{}\n{}", palette.css_custom_properties(), rules.join("\n"))
        },
    }
}
