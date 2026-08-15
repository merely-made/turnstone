//! Turnstone's first settings projection provider.
//!
//! The provider adapts application-owned settings to Genet's host-facing
//! settings contract. It does not become a second store or a global registry.

use std::io;
use std::path::{Path, PathBuf};

use genet_host_api::settings::{
    SettingControl, SettingMovement, SettingMutability, SettingOption, SettingScope,
    SettingSecurity, SettingSpec, SettingValue, SettingsError, SettingsProvider,
};
use genet_host_api::tile::SettingsRef;
use pandect::{
    ApplicationSettings, ShellbarEdge, load_application_settings, save_application_settings,
};

/// Turnstone's application-owned settings page.
pub const APPLICATION_REFERENCE: &str = "turnstone/application";

const THEME_MODE_OPTIONS: [(&str, &str); 3] = [
    ("system", "Use system appearance"),
    ("light", "Light"),
    ("dark", "Dark"),
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
}

impl ApplicationSettingsProvider {
    /// Load application settings from the data root, using typed defaults when
    /// the application has not written its settings file yet.
    pub fn load(data_root: impl Into<PathBuf>) -> io::Result<Self> {
        let data_root = data_root.into();
        let settings = load_application_settings(&data_root)?.unwrap_or_default();
        Ok(Self {
            data_root,
            settings,
        })
    }

    /// Construct a provider around already-loaded settings.
    pub fn from_settings(data_root: impl Into<PathBuf>, settings: ApplicationSettings) -> Self {
        Self {
            data_root: data_root.into(),
            settings,
        }
    }

    /// Inspect the typed application owner behind the projection.
    pub fn settings(&self) -> &ApplicationSettings {
        &self.settings
    }

    /// Return the data root used for persistence.
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    fn check_reference(reference: &SettingsRef) -> Result<(), SettingsError> {
        if reference.0 == APPLICATION_REFERENCE {
            Ok(())
        } else {
            Err(SettingsError::UnsupportedReference(reference.clone()))
        }
    }

    fn application_text_spec(
        id: &str,
        label: &str,
        value: Option<&String>,
        movement: SettingMovement,
    ) -> SettingSpec {
        SettingSpec {
            id: id.into(),
            label: label.into(),
            scope: SettingScope::Application,
            movement,
            mutability: SettingMutability::Live,
            security: SettingSecurity::Ordinary,
            control: SettingControl::Text,
            value: SettingValue::Text(value.cloned().unwrap_or_default()),
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

    fn save(&self) -> Result<(), SettingsError> {
        save_application_settings(&self.data_root, &self.settings)
            .map_err(|error| SettingsError::Storage(error.to_string()))
    }
}

impl SettingsProvider for ApplicationSettingsProvider {
    fn describe(&self, reference: &SettingsRef) -> Result<Vec<SettingSpec>, SettingsError> {
        Self::check_reference(reference)?;
        Ok(vec![
            Self::application_text_spec(
                "theme.id",
                "Theme",
                self.settings.theme_id.as_ref(),
                SettingMovement::PersonaSynced,
            ),
            Self::application_choice_spec(
                "theme.mode",
                "Theme mode",
                self.settings.theme_mode.as_deref().unwrap_or("system"),
                &THEME_MODE_OPTIONS,
                SettingMovement::PersonaSynced,
            ),
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
        ])
    }

    fn apply(
        &mut self,
        reference: &SettingsRef,
        setting_id: &str,
        value: SettingValue,
    ) -> Result<(), SettingsError> {
        Self::check_reference(reference)?;

        match (setting_id, value) {
            ("theme.id", SettingValue::Text(value)) => {
                self.settings.theme_id = (!value.is_empty()).then_some(value);
            }
            ("theme.mode", SettingValue::Text(value)) => {
                if !THEME_MODE_OPTIONS
                    .iter()
                    .any(|(candidate, _)| *candidate == value)
                {
                    return Err(invalid_choice("theme.mode", &value, &THEME_MODE_OPTIONS));
                }
                self.settings.theme_mode = (value != "system").then_some(value);
            }
            ("ui.zoom", SettingValue::Number(value))
                if value.is_finite() && (0.5..=3.0).contains(&value) =>
            {
                self.settings.ui_zoom = value as f32;
            }
            ("chrome.shellbar.edge", SettingValue::Text(value)) => {
                self.settings.shellbar_edge =
                    shellbar_edge_from_value(&value).ok_or_else(|| {
                        invalid_choice("chrome.shellbar.edge", &value, &SHELLBAR_EDGE_OPTIONS)
                    })?;
            }
            ("chrome.shellbar.visible", SettingValue::Boolean(value)) => {
                self.settings.shellbar_hidden = !value;
            }
            ("theme.id" | "theme.mode", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: setting_id.into(),
                    message: format!("expected Text, got {other:?}"),
                });
            }
            ("ui.zoom", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: setting_id.into(),
                    message: format!("expected Number in 0.5..=3.0, got {other:?}"),
                });
            }
            ("chrome.shellbar.edge", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: "chrome.shellbar.edge".into(),
                    message: format!("expected Text, got {other:?}"),
                });
            }
            ("chrome.shellbar.visible", other) => {
                return Err(SettingsError::InvalidValue {
                    setting_id: "chrome.shellbar.visible".into(),
                    message: format!("expected Boolean, got {other:?}"),
                });
            }
            (other, _) => return Err(SettingsError::UnknownSetting(other.into())),
        }

        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "turnstone-settings-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn application_projection_describes_owner_axes_and_controls() {
        let provider = ApplicationSettingsProvider::from_settings(
            scratch_root("describe"),
            ApplicationSettings::default(),
        );
        let specs = provider
            .describe(&SettingsRef(APPLICATION_REFERENCE.into()))
            .unwrap();

        assert_eq!(specs.len(), 5);
        assert_eq!(specs[0].movement, SettingMovement::PersonaSynced);
        assert_eq!(specs[0].control, SettingControl::Text);
        assert!(matches!(specs[1].control, SettingControl::Choice { .. }));
        assert_eq!(specs[2].scope, SettingScope::Application);
        assert_eq!(
            specs[2].control,
            SettingControl::Number {
                min: Some(0.5),
                max: Some(3.0),
                step: Some(0.05),
            }
        );
        assert!(matches!(specs[3].control, SettingControl::Choice { .. }));
        assert_eq!(specs[4].control, SettingControl::Toggle);
    }

    #[test]
    fn typed_writes_update_the_owner_and_persist() {
        let root = scratch_root("apply");
        let reference = SettingsRef(APPLICATION_REFERENCE.into());
        let mut provider = ApplicationSettingsProvider::load(&root).unwrap();

        provider
            .apply(
                &reference,
                "theme.id",
                SettingValue::Text("theme:night".into()),
            )
            .unwrap();
        provider
            .apply(&reference, "ui.zoom", SettingValue::Number(1.25))
            .unwrap();
        provider
            .apply(
                &reference,
                "chrome.shellbar.edge",
                SettingValue::Text("bottom".into()),
            )
            .unwrap();
        provider
            .apply(
                &reference,
                "chrome.shellbar.visible",
                SettingValue::Boolean(false),
            )
            .unwrap();

        assert_eq!(provider.settings().theme_id.as_deref(), Some("theme:night"));
        assert_eq!(provider.settings().ui_zoom, 1.25);
        assert_eq!(provider.settings().shellbar_edge, ShellbarEdge::Bottom);
        assert!(provider.settings().shellbar_hidden);
        let loaded = load_application_settings(&root).unwrap().unwrap();
        assert_eq!(loaded.theme_id.as_deref(), Some("theme:night"));
        assert_eq!(loaded.ui_zoom, 1.25);
        assert_eq!(loaded.shellbar_edge, ShellbarEdge::Bottom);
        assert!(loaded.shellbar_hidden);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_reference_and_value_do_not_write() {
        let root = scratch_root("invalid");
        let mut provider = ApplicationSettingsProvider::load(&root).unwrap();

        assert!(matches!(
            provider.describe(&SettingsRef("wrong/page".into())),
            Err(SettingsError::UnsupportedReference(_))
        ));
        assert!(matches!(
            provider.apply(
                &SettingsRef(APPLICATION_REFERENCE.into()),
                "ui.zoom",
                SettingValue::Number(4.0)
            ),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(!pandect::application_settings_exist(&root));
        let _ = std::fs::remove_dir_all(root);
    }
}
