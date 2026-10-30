// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's first settings projection provider.
//!
//! The provider adapts application-owned settings to Genet's host-facing
//! settings contract. It does not become a second store or a global registry.

use std::io;
use std::path::{Path, PathBuf};

use mere_surface_api::settings::{
    SettingControl, SettingMovement, SettingMutability, SettingOption, SettingScope,
    SettingSecurity, SettingSpec, SettingValue, SettingsError, SettingsProvider,
};
use pandect::{
    ApplicationSettings, ShellbarEdge, application_settings_path, save_application_settings,
};
use tabard::theme::{choice::ThemeChoice, registry::Mode};
use workbench::SettingsRef;

/// Turnstone's application-owned settings page.
pub const APPLICATION_REFERENCE: &str = "turnstone/application";

const THEME_MODE_OPTIONS: [(&str, &str); 5] = [
    ("system", "Use system appearance"),
    ("light", "Light"),
    ("dark", "Dark"),
    ("hc_light", "High contrast light"),
    ("hc_dark", "High contrast dark"),
];

const SHELLBAR_EDGE_OPTIONS: [(&str, &str); 4] = [
    ("left", "Left"),
    ("right", "Right"),
    ("top", "Top"),
    ("bottom", "Bottom"),
];

fn shellbar_edge_value(edge: ShellbarEdge) -> &'static str {
    match edge {
        ShellbarEdge::Left => "left",
        ShellbarEdge::Right => "right",
        ShellbarEdge::Top => "top",
        ShellbarEdge::Bottom => "bottom",
    }
}

fn shellbar_edge_from_value(value: &str) -> Option<ShellbarEdge> {
    match value {
        "left" => Some(ShellbarEdge::Left),
        "right" => Some(ShellbarEdge::Right),
        "top" => Some(ShellbarEdge::Top),
        "bottom" => Some(ShellbarEdge::Bottom),
        _ => None,
    }
}

fn invalid_choice(setting_id: &str, value: &str, options: &[(&str, &str)]) -> SettingsError {
    let choices = options
        .iter()
        .map(|(candidate, _)| *candidate)
        .collect::<Vec<_>>()
        .join(", ");
    SettingsError::InvalidValue {
        setting_id: setting_id.into(),
        message: format!("expected one of {choices}, got {value:?}"),
    }
}

/// Adapts Turnstone's application settings store to the host projection.
pub struct ApplicationSettingsProvider {
    data_root: PathBuf,
    settings: ApplicationSettings,
    write_error: Option<String>,
    theme_catalog: crate::appearance::ThemeCatalog,
}

impl ApplicationSettingsProvider {
    /// Load application settings from the data root, using typed defaults when
    /// the application has not written its settings file yet.
    pub fn load(data_root: impl Into<PathBuf>) -> io::Result<Self> {
        let data_root = data_root.into();
        let settings = match std::fs::read_to_string(application_settings_path(&data_root)) {
            Ok(json) => {
                let mut settings: ApplicationSettings = serde_json::from_str(&json)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                // The earlier two optional strings could hold a mode without
                // an id. The flattened typed choice needs an id to deserialize.
                // Preserve those real records at this product read boundary;
                // an empty typed id still projects to an unset chrome id.
                if settings.theme.is_none() {
                    #[derive(serde::Deserialize)]
                    struct LegacyMode {
                        theme_mode: Option<String>,
                        #[serde(default)]
                        fonts: tabard::FontPreferences,
                    }
                    let legacy: LegacyMode = serde_json::from_str(&json)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                    if let Some(mode) = legacy.theme_mode.as_deref().and_then(Mode::from_key) {
                        let mut choice = ThemeChoice::new("", Some(mode));
                        choice.fonts = legacy.fonts;
                        settings.theme = Some(choice);
                    }
                }
                settings
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => ApplicationSettings::default(),
            Err(error) => return Err(error),
        };
        let theme_catalog =
            crate::appearance::ThemeCatalog::load(crate::appearance::library_path(&data_root));
        Ok(Self {
            theme_catalog,
            data_root,
            settings,
            write_error: None,
        })
    }

    /// Construct a provider around already-loaded settings.
    pub fn from_settings(data_root: impl Into<PathBuf>, settings: ApplicationSettings) -> Self {
        let data_root = data_root.into();
        let theme_catalog =
            crate::appearance::ThemeCatalog::load(crate::appearance::library_path(&data_root));
        Self {
            data_root,
            settings,
            write_error: None,
            theme_catalog,
        }
    }

    /// Select an explicit authored library without changing selection storage.
    /// Failed library reads remain visible and prevent theme writes.
    pub fn with_theme_library(mut self, path: impl Into<PathBuf>) -> Self {
        self.theme_catalog = crate::appearance::ThemeCatalog::load(path.into());
        self
    }

    /// Keep a failed read visible without letting a later setting replace its file.
    pub fn from_failed_load(data_root: impl Into<PathBuf>, error: impl Into<String>) -> Self {
        let mut provider = Self::from_settings(data_root, ApplicationSettings::default());
        provider.write_error = Some(error.into());
        provider
    }

    /// Inspect the typed application owner behind the projection.
    pub fn settings(&self) -> &ApplicationSettings {
        &self.settings
    }

    /// Return the data root used for persistence.
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub(crate) fn catalog(&self) -> &crate::appearance::ThemeCatalog {
        &self.theme_catalog
    }

    pub(crate) fn appearance_notice(&self) -> String {
        self.theme_catalog.notice(self.settings.theme.as_ref())
    }

    pub(crate) fn reload_themes(&mut self) {
        self.theme_catalog = crate::appearance::ThemeCatalog::load(self.theme_catalog.path.clone());
    }

    /// Apply one complete, validated selection after the shared workshop has
    /// actually saved it. Persistence succeeds before the live value changes.
    pub(crate) fn apply_theme_choice(&mut self, choice: ThemeChoice) -> Result<(), SettingsError> {
        if let Some(error) = &self.theme_catalog.error {
            return Err(SettingsError::Storage(error.clone()));
        }
        let resolved = self
            .theme_catalog
            .resolve(&choice)
            .map_err(SettingsError::Storage)?;
        if !resolved.diagnostics.is_empty() {
            return Err(SettingsError::InvalidValue {
                setting_id: "theme.id".into(),
                message: resolved
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            });
        }
        let mut candidate = self.current_write_base()?;
        candidate.theme = Some(choice);
        self.save_candidate(&candidate)?;
        self.settings = candidate;
        Ok(())
    }

    fn theme_specs(&self) -> Vec<SettingSpec> {
        let mut themes = vec![SettingOption {
            value: String::new(),
            label: "Turnstone default".into(),
        }];
        themes.extend(
            self.theme_catalog
                .registry
                .list()
                .into_iter()
                .map(|theme| SettingOption {
                    value: theme.id.clone(),
                    label: theme.name.clone(),
                }),
        );
        if let Some(choice) = &self.settings.theme {
            if !choice.theme_id.is_empty()
                && self
                    .theme_catalog
                    .registry
                    .theme_def(&choice.theme_id)
                    .is_none()
            {
                themes.push(SettingOption {
                    value: choice.theme_id.clone(),
                    label: format!("{} (unavailable)", choice.theme_id),
                });
            }
        }
        let mut modes = vec![SettingOption {
            value: "system".into(),
            label: "Theme default / system".into(),
        }];
        modes.extend(
            self.theme_catalog
                .modes(self.settings.theme.as_ref())
                .into_iter()
                .map(|mode| SettingOption {
                    value: mode.as_key(),
                    label: mode.label(),
                }),
        );
        if let Some(mode) = self
            .settings
            .theme
            .as_ref()
            .and_then(|choice| choice.theme_mode.as_ref())
        {
            if !modes.iter().any(|option| option.value == mode.as_key()) {
                modes.push(SettingOption {
                    value: mode.as_key(),
                    label: format!("{} (unavailable)", mode.label()),
                });
            }
        }
        vec![
            SettingSpec {
                id: "theme.id".into(),
                label: "Theme".into(),
                scope: SettingScope::Application,
                movement: SettingMovement::PersonaSynced,
                mutability: SettingMutability::Live,
                security: SettingSecurity::Ordinary,
                control: SettingControl::Choice { options: themes },
                value: SettingValue::Text(
                    self.settings
                        .theme
                        .as_ref()
                        .map(|choice| choice.theme_id.clone())
                        .unwrap_or_default(),
                ),
            },
            SettingSpec {
                id: "theme.mode".into(),
                label: "Theme mode".into(),
                scope: SettingScope::Application,
                movement: SettingMovement::PersonaSynced,
                mutability: SettingMutability::Live,
                security: SettingSecurity::Ordinary,
                control: SettingControl::Choice { options: modes },
                value: SettingValue::Text(
                    self.settings
                        .theme
                        .as_ref()
                        .and_then(|choice| choice.theme_mode.as_ref())
                        .map(Mode::as_key)
                        .unwrap_or_else(|| "system".into()),
                ),
            },
        ]
    }

    fn check_reference(reference: &SettingsRef) -> Result<(), SettingsError> {
        if reference.0 == APPLICATION_REFERENCE {
            Ok(())
        } else {
            Err(SettingsError::UnsupportedReference(reference.clone()))
        }
    }

    fn application_choice_spec(
        id: &str,
        label: &str,
        value: impl Into<String>,
        options: &[(&str, &str)],
        movement: SettingMovement,
    ) -> SettingSpec {
        SettingSpec {
            id: id.into(),
            label: label.into(),
            scope: SettingScope::Application,
            movement,
            mutability: SettingMutability::Live,
            security: SettingSecurity::Ordinary,
            control: SettingControl::Choice {
                options: options
                    .iter()
                    .map(|(value, label)| SettingOption {
                        value: (*value).into(),
                        label: (*label).into(),
                    })
                    .collect(),
            },
            value: SettingValue::Text(value.into()),
        }
    }

    /// Retained panes may cache an older choice. Each edit changes its own
    /// axis against the current durable owner, rather than overwriting another
    /// pane's successful selection with its cached snapshot.
    fn current_write_base(&self) -> Result<ApplicationSettings, SettingsError> {
        match std::fs::metadata(application_settings_path(&self.data_root)) {
            Ok(_) => Self::load(&self.data_root)
                .map(|provider| provider.settings)
                .map_err(|error| SettingsError::Storage(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(self.settings.clone()),
            Err(error) => Err(SettingsError::Storage(error.to_string())),
        }
    }

    fn save_candidate(&self, candidate: &ApplicationSettings) -> Result<(), SettingsError> {
        if let Some(error) = &self.write_error {
            return Err(SettingsError::Storage(format!(
                "Application settings were not loaded: {error}. The existing file is preserved."
            )));
        }
        save_application_settings(&self.data_root, candidate)
            .map_err(|error| SettingsError::Storage(error.to_string()))
    }
}

impl SettingsProvider for ApplicationSettingsProvider {
    fn describe(&self, reference: &SettingsRef) -> Result<Vec<SettingSpec>, SettingsError> {
        Self::check_reference(reference)?;
        let mut specs = self.theme_specs();
        specs.extend(vec![
            SettingSpec {
                id: "ui.zoom".into(),
                label: "UI zoom".into(),
                scope: SettingScope::Application,
                movement: SettingMovement::LocalOnly,
                mutability: SettingMutability::Live,
                security: SettingSecurity::Ordinary,
                control: SettingControl::Number {
                    min: Some(0.5),
                    max: Some(3.0),
                    step: Some(0.05),
                },
                value: SettingValue::Number(f64::from(self.settings.ui_zoom)),
            },
            Self::application_choice_spec(
                "chrome.shellbar.edge",
                "Shellbar edge",
                shellbar_edge_value(self.settings.shellbar_edge),
                &SHELLBAR_EDGE_OPTIONS,
                SettingMovement::LocalOnly,
            ),
            SettingSpec {
                id: "chrome.shellbar.visible".into(),
                label: "Show shellbar".into(),
                scope: SettingScope::Application,
                movement: SettingMovement::LocalOnly,
                mutability: SettingMutability::Live,
                security: SettingSecurity::Ordinary,
                control: SettingControl::Toggle,
                value: SettingValue::Boolean(!self.settings.shellbar_hidden),
            },
            SettingSpec {
                id: "behaviors.cascade_budget".into(),
                label: "Behavior cascade rounds".into(),
                scope: SettingScope::Application,
                movement: SettingMovement::LocalOnly,
                mutability: SettingMutability::Live,
                security: SettingSecurity::Ordinary,
                // A number with a floor of 1 and no unlimited value: an
                // unbounded cascade is the condition the budget exists to
                // report.
                control: SettingControl::Number {
                    min: Some(1.0),
                    max: Some(16.0),
                    step: Some(1.0),
                },
                value: SettingValue::Number(f64::from(self.settings.cascade_budget)),
            },
        ]);
        Ok(specs)
    }

    fn apply(
        &mut self,
        reference: &SettingsRef,
        setting_id: &str,
        value: SettingValue,
    ) -> Result<(), SettingsError> {
        Self::check_reference(reference)?;

        let mut candidate = self.current_write_base()?;
        match (setting_id, value) {
            ("theme.id", SettingValue::Text(value)) => {
                if let Some(error) = &self.theme_catalog.error {
                    return Err(SettingsError::Storage(error.clone()));
                }
                if !value.is_empty() && self.theme_catalog.registry.theme_def(&value).is_none() {
                    return Err(SettingsError::InvalidValue {
                        setting_id: "theme.id".into(),
                        message: "Choose an available theme".into(),
                    });
                }
                let mode = candidate
                    .theme
                    .as_ref()
                    .and_then(|theme| theme.theme_mode.clone());
                let fonts = candidate
                    .theme
                    .as_ref()
                    .map(|t| t.fonts.clone())
                    .unwrap_or_default();
                candidate.theme =
                    (!value.is_empty() || mode.is_some() || !fonts.is_empty()).then(|| {
                        let mut choice = ThemeChoice::new(value, mode);
                        choice.fonts = fonts;
                        choice
                    });
            },
            ("theme.mode", SettingValue::Text(value)) => {
                if let Some(error) = &self.theme_catalog.error {
                    return Err(SettingsError::Storage(error.clone()));
                }
                if value != "system"
                    && !self
                        .theme_catalog
                        .modes(candidate.theme.as_ref())
                        .iter()
                        .any(|mode| mode.as_key() == value)
                {
                    return Err(invalid_choice("theme.mode", &value, &THEME_MODE_OPTIONS));
                }
                let id = candidate
                    .theme
                    .as_ref()
                    .map(|theme| theme.theme_id.clone())
                    .unwrap_or_default();
                let mode = Mode::from_key(&value);
                let fonts = candidate
                    .theme
                    .as_ref()
                    .map(|t| t.fonts.clone())
                    .unwrap_or_default();
                candidate.theme =
                    (!id.is_empty() || mode.is_some() || !fonts.is_empty()).then(|| {
                        let mut choice = ThemeChoice::new(id, mode);
                        choice.fonts = fonts;
                        choice
                    });
            },
            ("ui.zoom", SettingValue::Number(value))
                if value.is_finite() && (0.5..=3.0).contains(&value) =>
            {
                candidate.ui_zoom = value as f32;
            },
            ("chrome.shellbar.edge", SettingValue::Text(value)) => {
                candidate.shellbar_edge = shellbar_edge_from_value(&value).ok_or_else(|| {
                    invalid_choice("chrome.shellbar.edge", &value, &SHELLBAR_EDGE_OPTIONS)
                })?;
            },
            ("chrome.shellbar.visible", SettingValue::Boolean(value)) => {
                candidate.shellbar_hidden = !value;
            },
            ("behaviors.cascade_budget", SettingValue::Number(value))
                if value.is_finite() && (1.0..=16.0).contains(&value) =>
            {
                candidate.cascade_budget = value as u32;
            },
            ("behaviors.cascade_budget", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: "behaviors.cascade_budget".into(),
                    message: format!("expected Number in 1..=16, got {other:?}"),
                });
            },
            ("theme.id" | "theme.mode", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: setting_id.into(),
                    message: format!("expected Text, got {other:?}"),
                });
            },
            ("ui.zoom", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: setting_id.into(),
                    message: format!("expected Number in 0.5..=3.0, got {other:?}"),
                });
            },
            ("chrome.shellbar.edge", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: "chrome.shellbar.edge".into(),
                    message: format!("expected Text, got {other:?}"),
                });
            },
            ("chrome.shellbar.visible", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: "chrome.shellbar.visible".into(),
                    message: format!("expected Boolean, got {other:?}"),
                });
            },
            (other, _) => return Err(SettingsError::UnknownSetting(other.into())),
        }

        self.save_candidate(&candidate)?;
        self.settings = candidate;
        Ok(())
    }
}

#[cfg(test)]
#[path = "settings_provider/tests.rs"]
mod tests;
