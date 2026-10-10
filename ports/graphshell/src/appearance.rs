// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's host appearance, separate from graph truth and a projection's
//! authored `appearance`. Tabard owns definitions, validation and derivation;
//! the application owns its preference record and its visual role mapping.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
pub use tabard::theme::choice::ThemeChoice;
pub use tabard::theme::registry::Mode;
use tabard::theme::registry::{ThemeRegistry, ThemeSource};
use tabard::{ResolvedThemeChoice, Theme, ThemePresentation, resolve_theme_choice};

pub const APPEARANCE_STORAGE_KEY: &str = "graphshell.application.appearance.v1";
pub const MAX_APPEARANCE_BYTES: usize = 1_048_576;

/// Host settings envelope; every member of `themes` remains the existing
/// portable Tabard definition rather than a second theme format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceDocument {
    pub version: u32,
    pub choice: ThemeChoice,
    pub themes: Vec<Theme>,
}

impl Default for AppearanceDocument {
    fn default() -> Self {
        Self {
            version: 1,
            choice: ThemeChoice::default(),
            themes: Vec::new(),
        }
    }
}

pub struct Appearance {
    document: AppearanceDocument,
    registry: ThemeRegistry,
    resolved: ResolvedThemeChoice,
}

impl Default for Appearance {
    fn default() -> Self {
        Self::from_document(AppearanceDocument::default()).expect("built-in appearance is valid")
    }
}

impl Appearance {
    pub fn from_json(json: &str) -> Result<Self, String> {
        if json.len() > MAX_APPEARANCE_BYTES {
            return Err("Appearance settings exceed 1 MiB.".into());
        }
        let document = serde_json::from_str(json)
            .map_err(|error| format!("Invalid appearance settings: {error}"))?;
        Self::from_document(document)
    }

    pub fn from_document(document: AppearanceDocument) -> Result<Self, String> {
        if document.version != 1 {
            return Err(format!(
                "Unsupported appearance settings version {}.",
                document.version
            ));
        }
        let mut registry = ThemeRegistry::default();
        let mut identities = HashSet::new();
        for theme in &document.themes {
            let key = theme.id.trim().to_ascii_lowercase();
            if !identities.insert(key) {
                return Err(format!("Duplicate imported theme {}.", theme.id));
            }
            if theme.source != ThemeSource::User || registry.theme_def(&theme.id).is_some() {
                return Err(format!(
                    "Imported theme {} collides with a built-in or is not a user definition.",
                    theme.id
                ));
            }
            // Reuse portable intake validation, including nonfinite harmony
            // offsets and valid attached mode-sheet keys.
            let validated = tabard::portable::parse_theme_json(
                &tabard::portable::theme_json(theme).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            registry.add_user_theme(validated)?;
        }
        let resolved =
            resolve_theme_choice(&registry, &document.choice).map_err(|error| error.to_string())?;
        Ok(Self {
            document,
            registry,
            resolved,
        })
    }

    pub fn json(&self) -> Result<String, String> {
        let json = serde_json::to_string(&self.document).map_err(|error| error.to_string())?;
        if json.len() > MAX_APPEARANCE_BYTES {
            return Err("Appearance settings exceed 1 MiB.".into());
        }
        Ok(json)
    }

    pub fn choice(&self) -> &ThemeChoice {
        &self.document.choice
    }
    pub fn resolved(&self) -> &ResolvedThemeChoice {
        &self.resolved
    }

    /// Reuse the shared face-palette mapping with the six browser-computed
    /// roles that mapping actually consumes. Other palette fields are unused;
    /// this does not purport to derive an arbitrary stylesheet from seeds.
    pub fn canvas_face_palette(&self, colors: [[u8; 4]; 6]) -> mere::canvas::DerivedFacePalette {
        let fallback = resolve_theme_choice(&ThemeRegistry::default(), &ThemeChoice::default())
            .expect("built-in fallback");
        let ThemePresentation::Derived(base) = fallback.presentation else {
            unreachable!("built-in has no authored sheets")
        };
        let mut palette = base.palette;
        let color = |value: [u8; 4]| {
            mere::canvas::palette::ThemeColor::rgba(value[0], value[1], value[2], value[3])
        };
        palette.primary = color(colors[0]);
        palette.secondary = color(colors[1]);
        palette.tertiary = color(colors[2]);
        palette.success = color(colors[3]);
        palette.danger = color(colors[4]);
        palette.text = color(colors[5]);
        mere::canvas::DerivedFacePalette::from_palette(&palette)
    }
    pub fn themes(&self) -> Vec<(String, String)> {
        self.registry
            .list()
            .iter()
            .map(|theme| (theme.id.clone(), theme.name.clone()))
            .collect()
    }
    pub fn modes(&self) -> Vec<Mode> {
        let mut modes = vec![Mode::Light, Mode::Dark, Mode::HcLight, Mode::HcDark];
        modes.extend(
            self.resolved
                .theme
                .mode_sheets
                .keys()
                .filter_map(|key| Mode::from_key(key))
                .filter(|mode| matches!(mode, Mode::Custom(_))),
        );
        modes
    }

    pub fn with_choice(&self, choice: ThemeChoice) -> Result<Self, String> {
        if self.registry.theme_def(&choice.theme_id).is_none() {
            return Err(format!("Theme {} is unavailable.", choice.theme_id));
        }
        let mut document = self.document.clone();
        document.choice = choice;
        Self::from_explicit_choice(document)
    }

    pub fn with_import(&self, json: &str) -> Result<Self, String> {
        if json.len() > MAX_APPEARANCE_BYTES {
            return Err("Theme file exceeds 1 MiB.".into());
        }
        let theme = tabard::portable::parse_theme_json(json).map_err(|error| error.to_string())?;
        if theme.source != ThemeSource::User
            || ThemeRegistry::default().theme_def(&theme.id).is_some()
        {
            return Err(
                "Import a user theme exported by Tabard; built-in identities are reserved.".into(),
            );
        }
        let mut document = self.document.clone();
        document.themes.retain(|old| {
            old.id.trim().to_ascii_lowercase() != theme.id.trim().to_ascii_lowercase()
        });
        document.choice =
            ThemeChoice::new(theme.id.clone(), self.resolved.resolved.theme_mode.clone());
        document.themes.push(theme);
        Self::from_explicit_choice(document)
    }

    fn from_explicit_choice(document: AppearanceDocument) -> Result<Self, String> {
        let mut candidate = Self::from_document(document)?;
        // An explicit selection of a new definition chooses its supported
        // default when the previous custom mode does not exist there. Stored
        // unresolved choices remain untouched by `from_document`/`from_json`.
        candidate.document.choice = candidate.resolved.resolved.clone();
        candidate.resolved = resolve_theme_choice(&candidate.registry, &candidate.document.choice)
            .map_err(|error| error.to_string())?;
        Ok(candidate)
    }

    /// Commit only after the application's independent settings store accepts
    /// the bytes. Browser storage failure leaves the valid preference intact.
    pub fn accept(
        &mut self,
        candidate: Self,
        store: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        store(&candidate.json()?)?;
        *self = candidate;
        Ok(())
    }

    /// Default roles support omitted authored properties; the exact authored
    /// rules are appended last and remain authoritative. Those defaults are
    /// the shared built-in, never a claimed derivation of arbitrary CSS.
    pub fn stylesheet(&self) -> String {
        let (defaults, authored) = match &self.resolved.presentation {
            ThemePresentation::Derived(palette) => (palette.css_custom_properties(), String::new()),
            ThemePresentation::AuthoredStylesheet(rules) => {
                let mode = self
                    .resolved
                    .resolved
                    .theme_mode
                    .as_ref()
                    .expect("resolved mode");
                let fallback_mode = if matches!(mode, Mode::Custom(_)) {
                    Mode::Dark
                } else {
                    mode.clone()
                };
                let fallback = resolve_theme_choice(
                    &ThemeRegistry::default(),
                    &ThemeChoice::new(ThemeChoice::default().theme_id, Some(fallback_mode)),
                )
                .expect("built-in fallback");
                let ThemePresentation::Derived(palette) = fallback.presentation else {
                    unreachable!("built-in has no authored sheets")
                };
                (palette.css_custom_properties(), rules.join("\n"))
            },
        };
        format!("{defaults}\n{ROLE_STYLESHEET}\n{authored}")
    }
}

/// Product roles shared by both Graphshell browser compositions. Their
/// structural layout remains owned by each existing application stylesheet.
pub const ROLE_STYLESHEET: &str = r#"
:root { --gs-canvas-background:var(--tabard-color-bg); --gs-canvas-edge:var(--tabard-color-text-dim); }
.shell, main { color:var(--tabard-color-text); }
main { background-color:var(--tabard-color-bg); }
.topbar, .rail, .remote-card, .tree-tools, .detail, .session-card, .proof, .product-proof { background-color:var(--tabard-color-surface); color:var(--tabard-color-text); border-color:var(--tabard-color-text-dim); }
.pill, .tree-controls button, .tree-tools button, .tree-product button, .select-box, .select-list, .tree-product [data-cambium-text-value], .tree-detail, .preview-card { background-color:var(--tabard-color-surface-2); color:var(--tabard-color-text); border-color:var(--tabard-color-text-dim); }
.pill.active, .tools-switch button[aria-pressed=true], .preview-card.selected { background-color:var(--tabard-color-tertiary); color:var(--tabard-color-on-tertiary); border-color:var(--tabard-color-primary); }
.action, .action-submit { background-color:var(--tabard-color-primary); color:var(--tabard-color-on-primary); }
.brand, .session-name, .selection, .detail-title, .remote-card-title, .action-form-title, .action-field-label, .product-status, .card-title, .tools-active, .tools-cards, .preview-title { color:var(--tabard-color-text); }
.subtitle, .hint, .eyebrow, .address, .action-help, .action-choice, .proof-copy, .tools-caption, .tree-tools .tools-caption, .tools-status, .tools-storage, .tools-note, .card-detail, .preview-status { color:var(--tabard-color-text-dim); }
.action-choice.selected, .remote-card-kicker { color:var(--tabard-color-primary); }
.action-form, .tools-section + .tools-section { border-color:var(--tabard-color-text-dim); }
.action-form-error { color:var(--tabard-color-danger); }
.action-status, .proof-pass { color:var(--tabard-color-success); }
.preview-root { color:var(--tabard-color-text); background-color:var(--tabard-color-bg); }
.tree-refusal { color:var(--tabard-color-text); background-color:var(--tabard-color-surface-2); border-color:var(--tabard-color-danger); }
.applet-library { color:var(--tabard-color-text); background-color:var(--tabard-color-bg); }
.applet-library button, .applet-search-field { color:var(--tabard-color-text); background-color:var(--tabard-color-surface-2); border-color:var(--tabard-color-text-dim); }
.applet-library button:disabled { color:var(--tabard-color-text-dim); background-color:var(--tabard-color-surface); }
.applet-library li, .applet-library pre, .applet-review { color:var(--tabard-color-text); background-color:var(--tabard-color-surface); border-color:var(--tabard-color-text-dim); }
[data-graphshell-color-role=background] { background-color:var(--gs-canvas-background); }
[data-graphshell-color-role=edge] { background-color:var(--gs-canvas-edge); }
[data-graphshell-color-role=text] { background-color:var(--tabard-color-text); }
[data-graphshell-color-role=surface] { background-color:var(--tabard-color-surface); }
[data-graphshell-color-role=primary] { background-color:var(--tabard-color-primary); }
[data-graphshell-color-role=secondary] { background-color:var(--tabard-color-secondary); }
[data-graphshell-color-role=tertiary] { background-color:var(--tabard-color-tertiary); }
[data-graphshell-color-role=on-primary] { background-color:var(--tabard-color-on-primary); }
[data-graphshell-color-role=success] { background-color:var(--tabard-color-success); }
[data-graphshell-color-role=danger] { background-color:var(--tabard-color-danger); }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn imported_theme() -> Theme {
        let original = ThemeRegistry::default().list()[0].clone();
        let mut theme = original.clone();
        theme.id = "theme:graphshell-test".into();
        theme.name = "Garden".into();
        theme.source = ThemeSource::User;
        theme
    }

    #[test]
    fn imported_choice_reopens_with_all_canonical_modes_without_activating_a_graph() {
        let mut appearance = Appearance::default()
            .with_import(&tabard::portable::theme_json(&imported_theme()).unwrap())
            .unwrap();
        for mode in [Mode::Light, Mode::Dark, Mode::HcLight, Mode::HcDark] {
            appearance = appearance
                .with_choice(ThemeChoice::new(
                    "theme:graphshell-test",
                    Some(mode.clone()),
                ))
                .unwrap();
            let reopened = Appearance::from_json(&appearance.json().unwrap()).unwrap();
            assert_eq!(reopened.choice(), appearance.choice());
            assert_eq!(reopened.resolved().resolved.theme_mode, Some(mode));
            assert_eq!(reopened.resolved().theme, imported_theme());
        }
    }

    #[test]
    fn corrupt_intake_and_failed_storage_preserve_valid_choice() {
        let mut appearance = Appearance::default();
        let original = appearance.json().unwrap();
        assert!(appearance.with_import("{broken").is_err());
        assert!(Appearance::from_json("{\"version\":9,\"choice\":{},\"themes\":[]}").is_err());
        let candidate = appearance
            .with_choice(ThemeChoice::new("theme:light", Some(Mode::Light)))
            .unwrap();
        assert!(
            appearance
                .accept(candidate, |_| Err("quota".into()))
                .is_err()
        );
        assert_eq!(appearance.json().unwrap(), original);
    }

    #[test]
    fn missing_identity_keeps_requested_choice_and_reports_shared_fallback() {
        let choice = ThemeChoice::new("theme:removed", Some(Mode::Light));
        let appearance = Appearance::from_document(AppearanceDocument {
            choice: choice.clone(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(appearance.choice(), &choice);
        assert_ne!(appearance.resolved().resolved.theme_id, choice.theme_id);
        assert!(!appearance.resolved().diagnostics.is_empty());
        assert_eq!(
            Appearance::from_json(&appearance.json().unwrap())
                .unwrap()
                .choice(),
            &choice
        );
    }

    #[test]
    fn browser_roles_retheme_faces_with_shared_mapping_and_alpha() {
        let roles = [
            [11, 22, 33, 200],
            [44, 55, 66, 255],
            [77, 88, 99, 255],
            [100, 120, 140, 255],
            [160, 180, 200, 255],
            [235, 245, 255, 255],
        ];
        let colors = Appearance::default().canvas_face_palette(roles).colors();
        assert_eq!(&colors[..5], &roles[..5]);
        assert_ne!(
            colors[5], colors[0],
            "the shared companion mapping remains active"
        );
    }

    #[test]
    fn explicit_theme_change_chooses_supported_mode_without_rewriting_stored_fallbacks() {
        let mut theme = imported_theme();
        theme.mode_sheets.insert(
            "custom:garden".into(),
            vec![":root { --tabard-color-bg:#203020; }".into()],
        );
        let selected = Appearance::default()
            .with_import(&tabard::portable::theme_json(&theme).unwrap())
            .unwrap()
            .with_choice(ThemeChoice::new(
                theme.id,
                Some(Mode::Custom("garden".into())),
            ))
            .unwrap();
        let changed = selected
            .with_choice(ThemeChoice::new(
                "theme:light",
                selected.choice().theme_mode.clone(),
            ))
            .unwrap();
        assert_eq!(changed.choice().theme_mode, Some(Mode::Light));
        assert_eq!(changed.choice(), &changed.resolved().resolved);
        assert!(changed.resolved().diagnostics.is_empty());
    }

    #[test]
    fn authored_css_remains_exact_and_last_with_explicit_default_role_fallback() {
        let json = include_str!("../web/fixtures/tabard-garden.theme.json");
        let theme = tabard::portable::parse_theme_json(json).unwrap();
        let rules = theme.mode_sheets.get("dark").unwrap();
        let appearance = Appearance::default().with_import(json).unwrap();
        assert!(matches!(
            appearance.resolved().presentation,
            ThemePresentation::AuthoredStylesheet(_)
        ));
        assert!(appearance.stylesheet().ends_with(&rules.join("\n")));
        assert!(appearance.stylesheet().contains("--tabard-color-surface:"));
        assert_eq!(
            Appearance::from_json(&appearance.json().unwrap())
                .unwrap()
                .resolved()
                .theme
                .mode_sheets,
            theme.mode_sheets
        );
    }
}
