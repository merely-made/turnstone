// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

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
fn failed_setting_write_keeps_the_owner_and_cannot_leak_into_a_later_save() {
    let root = scratch_root("failed-write");
    let target = application_settings_path(&root);
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), b"preserved application bytes").unwrap();
    let initial = ApplicationSettings::default();
    let mut provider = ApplicationSettingsProvider::from_settings(&root, initial.clone());
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    assert!(
        provider
            .apply(
                &reference,
                "theme.id",
                SettingValue::Text("theme:unpublished".into())
            )
            .is_err()
    );
    assert_eq!(provider.settings(), &initial);
    assert_eq!(
        std::fs::read(target.join("owned")).unwrap(),
        b"preserved application bytes"
    );
    std::fs::remove_dir_all(&target).unwrap();
    provider
        .apply(&reference, "ui.zoom", SettingValue::Number(1.5))
        .unwrap();
    let reopened = ApplicationSettingsProvider::load(&root).unwrap();
    assert_eq!(reopened.settings().theme, initial.theme);
    assert_eq!(reopened.settings().ui_zoom, 1.5);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_load_projection_refuses_to_overwrite_the_corrupt_settings_file() {
    let root = scratch_root("failed-load");
    let target = application_settings_path(&root);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"unreadable application settings").unwrap();
    let error = ApplicationSettingsProvider::load(&root).err().unwrap();
    let mut provider = ApplicationSettingsProvider::from_failed_load(&root, error.to_string());
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    assert!(
        provider
            .apply(&reference, "ui.zoom", SettingValue::Number(1.5))
            .is_err()
    );
    assert_eq!(
        provider.settings().ui_zoom,
        ApplicationSettings::default().ui_zoom
    );
    assert_eq!(
        std::fs::read(target).unwrap(),
        b"unreadable application settings"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn theme_and_mode_edits_preserve_family_requests_through_reopening() {
    let root = scratch_root("frame-font-requests");
    let mut choice = ThemeChoice::new("theme:dark", Some(Mode::Dark));
    choice.fonts =
        serde_json::from_str(r#"{"interface":["Absent face","serif"],"reading":["monospace"]}"#)
            .unwrap();
    let mut settings = ApplicationSettings::default();
    settings.theme = Some(choice.clone());
    let mut provider = ApplicationSettingsProvider::from_settings(&root, settings);
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    provider
        .apply(
            &reference,
            "theme.id",
            SettingValue::Text("theme:light".into()),
        )
        .unwrap();
    provider
        .apply(&reference, "theme.mode", SettingValue::Text("light".into()))
        .unwrap();
    provider
        .apply(&reference, "theme.id", SettingValue::Text(String::new()))
        .unwrap();
    provider
        .apply(
            &reference,
            "theme.mode",
            SettingValue::Text("system".into()),
        )
        .unwrap();
    assert_eq!(
        provider.settings().theme.as_ref().unwrap().fonts,
        choice.fonts
    );
    let reopened = ApplicationSettingsProvider::load(&root).unwrap();
    assert_eq!(reopened.settings().theme, provider.settings().theme);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn saved_workshop_definition_becomes_available_without_selecting_it_until_apply() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("profile");
    let library = temporary.path().join("shared/themes.json");
    let mut provider =
        ApplicationSettingsProvider::from_settings(&root, ApplicationSettings::default())
            .with_theme_library(&library);
    let mut editor = tabard_workshop::WorkshopState::load(&library).unwrap();
    editor
        .edit_definition(
            tabard::theme::registry::ThemeRegistry::default()
                .theme_def(tabard::theme::registry::THEME_ID_DEFAULT)
                .unwrap(),
            Some(Mode::HcLight),
        )
        .unwrap();
    assert!(editor.saved_choice().is_err());
    editor.save();
    let saved = editor.saved_choice().unwrap();
    provider.reload_themes();
    assert_eq!(provider.settings().theme, None);
    assert!(!application_settings_path(&root).exists());
    let specs = provider
        .describe(&SettingsRef(APPLICATION_REFERENCE.into()))
        .unwrap();
    let SettingControl::Choice { options } = &specs[0].control else {
        panic!("theme picker")
    };
    assert!(options.iter().any(|option| option.value == saved.theme_id));
    provider.apply_theme_choice(saved.clone()).unwrap();
    let reopened = ApplicationSettingsProvider::load(&root)
        .unwrap()
        .with_theme_library(&library);
    assert_eq!(reopened.settings().theme, Some(saved));
    assert_eq!(
        reopened
            .catalog()
            .resolve(reopened.settings().theme.as_ref().unwrap())
            .unwrap()
            .resolved
            .theme_mode,
        Some(Mode::HcLight)
    );
}

#[test]
fn missing_identity_and_custom_mode_remain_stored_while_visible_fallback_is_used() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("profile");
    let library = temporary.path().join("themes.json");
    let requested = ThemeChoice::new(
        "theme:temporarily-absent",
        Some(Mode::Custom("concert".into())),
    );
    let settings = ApplicationSettings {
        theme: Some(requested.clone()),
        ..Default::default()
    };
    let mut provider =
        ApplicationSettingsProvider::from_settings(&root, settings).with_theme_library(&library);
    let resolved = provider.catalog().resolve(&requested).unwrap();
    assert_eq!(resolved.requested, requested);
    assert_eq!(resolved.diagnostics.len(), 2);
    assert!(provider.appearance_notice().contains("unavailable"));
    provider
        .apply(
            &SettingsRef(APPLICATION_REFERENCE.into()),
            "ui.zoom",
            SettingValue::Number(1.5),
        )
        .unwrap();
    let reopened = ApplicationSettingsProvider::load(&root)
        .unwrap()
        .with_theme_library(&library);
    assert_eq!(reopened.settings().theme, Some(requested));
}

#[test]
fn malformed_library_blocks_theme_changes_and_preserves_both_owners() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("profile");
    let library = temporary.path().join("themes.json");
    std::fs::write(&library, "broken theme library").unwrap();
    let original = ApplicationSettings::default();
    let mut provider = ApplicationSettingsProvider::from_settings(&root, original.clone())
        .with_theme_library(&library);
    assert!(
        provider
            .appearance_notice()
            .contains("Existing library is preserved")
    );
    assert!(
        provider
            .apply_theme_choice(ThemeChoice::new("theme:dark", Some(Mode::Dark)))
            .is_err()
    );
    assert!(
        provider
            .apply(
                &SettingsRef(APPLICATION_REFERENCE.into()),
                "theme.mode",
                SettingValue::Text("light".into())
            )
            .is_err()
    );
    assert_eq!(provider.settings(), &original);
    assert!(!application_settings_path(&root).exists());
    assert_eq!(
        std::fs::read_to_string(&library).unwrap(),
        "broken theme library"
    );
    provider
        .apply(
            &SettingsRef(APPLICATION_REFERENCE.into()),
            "ui.zoom",
            SettingValue::Number(1.5),
        )
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(library).unwrap(),
        "broken theme library"
    );
    assert_eq!(provider.settings().theme, None);
}

#[test]
fn combined_saved_choice_does_not_publish_when_application_write_fails() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("profile");
    std::fs::create_dir_all(&root).unwrap();
    let target = application_settings_path(&root);
    std::fs::create_dir_all(&target).unwrap();
    let initial = ApplicationSettings::default();
    let mut provider = ApplicationSettingsProvider::from_settings(&root, initial.clone())
        .with_theme_library(temporary.path().join("themes.json"));
    assert!(
        provider
            .apply_theme_choice(ThemeChoice::new("theme:dark", Some(Mode::HcDark)))
            .is_err()
    );
    assert_eq!(provider.settings(), &initial);
    assert!(target.is_dir());
}

#[test]
fn authored_custom_modes_are_offered_only_by_the_selected_definition() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("profile");
    let library = temporary.path().join("themes.json");
    let mut theme = tabard::theme::registry::ThemeRegistry::default()
        .theme_def(tabard::theme::registry::THEME_ID_DEFAULT)
        .unwrap()
        .clone();
    theme.id = "theme:concert".into();
    theme.name = "Concert".into();
    theme.source = tabard::theme::registry::ThemeSource::User;
    let rules = vec![".omni { background-color: rgb(17, 34, 51); }".into()];
    theme
        .mode_sheets
        .insert("custom:concert".into(), rules.clone());
    tabard::library::ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[theme])
        .unwrap();
    let mut provider =
        ApplicationSettingsProvider::from_settings(&root, ApplicationSettings::default())
            .with_theme_library(&library);
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    assert!(
        provider
            .apply(
                &reference,
                "theme.mode",
                SettingValue::Text("custom:concert".into())
            )
            .is_err()
    );
    provider
        .apply(
            &reference,
            "theme.id",
            SettingValue::Text("theme:concert".into()),
        )
        .unwrap();
    let specs = provider.describe(&reference).unwrap();
    let SettingControl::Choice { options } = &specs[1].control else {
        panic!("mode picker")
    };
    for mode in ["light", "dark", "hc_light", "hc_dark", "custom:concert"] {
        assert!(options.iter().any(|option| option.value == mode));
    }
    provider
        .apply(
            &reference,
            "theme.mode",
            SettingValue::Text("custom:concert".into()),
        )
        .unwrap();
    let resolved = provider
        .catalog()
        .resolve(provider.settings().theme.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        resolved.presentation,
        tabard::ThemePresentation::AuthoredStylesheet(rules)
    );
    let reopened = ApplicationSettingsProvider::load(&root)
        .unwrap()
        .with_theme_library(&library);
    assert_eq!(reopened.settings().theme, provider.settings().theme);
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

    assert_eq!(specs.len(), 6);
    // Named rather than merely counted: a row that silently stops being
    // described is the failure this test should catch.
    assert!(
        specs
            .iter()
            .any(|spec| spec.id == "behaviors.cascade_budget"),
        "the cascade budget is offered as a setting"
    );
    assert_eq!(specs[0].movement, SettingMovement::PersonaSynced);
    assert!(matches!(specs[0].control, SettingControl::Choice { .. }));
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
    assert_eq!(
        specs[5].control,
        SettingControl::Number {
            min: Some(1.0),
            max: Some(16.0),
            step: Some(1.0),
        }
    );
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
            SettingValue::Text("theme:dark".into()),
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

    assert_eq!(
        provider.settings().theme.as_ref().unwrap().theme_id,
        "theme:dark"
    );
    assert_eq!(provider.settings().ui_zoom, 1.25);
    assert_eq!(provider.settings().shellbar_edge, ShellbarEdge::Bottom);
    assert!(provider.settings().shellbar_hidden);
    let loaded = pandect::load_application_settings(&root).unwrap().unwrap();
    assert_eq!(loaded.theme.as_ref().unwrap().theme_id, "theme:dark");
    assert_eq!(loaded.ui_zoom, 1.25);
    assert_eq!(loaded.shellbar_edge, ShellbarEdge::Bottom);
    assert!(loaded.shellbar_hidden);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn legacy_mode_without_id_survives_load_and_an_unrelated_write() {
    let root = scratch_root("legacy-mode");
    let path = application_settings_path(&root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"theme_id":null,"theme_mode":"light","ui_zoom":1.25}"#,
    )
    .unwrap();
    let mut provider = ApplicationSettingsProvider::load(&root).unwrap();
    assert_eq!(
        provider.settings().theme,
        Some(ThemeChoice::new("", Some(Mode::Light)))
    );
    let snapshot = crate::settings_pane::ChromeSettings::from(provider.settings());
    assert_eq!(snapshot.theme_id(), None);
    assert_eq!(snapshot.theme_mode(), Some("light"));
    let specs = provider
        .describe(&SettingsRef(APPLICATION_REFERENCE.into()))
        .unwrap();
    assert_eq!(specs[0].value, SettingValue::Text(String::new()));
    assert_eq!(specs[1].value, SettingValue::Text("light".into()));
    provider
        .apply(
            &SettingsRef(APPLICATION_REFERENCE.into()),
            "ui.zoom",
            SettingValue::Number(1.5),
        )
        .unwrap();
    let restored = ApplicationSettingsProvider::load(&root).unwrap();
    assert_eq!(restored.settings().theme, provider.settings().theme);
    assert_eq!(restored.settings().ui_zoom, 1.5);
    let encoded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(encoded["theme_id"], "");
    assert_eq!(encoded["theme_mode"], "light");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn theme_id_and_mode_edits_preserve_the_other_value() {
    let root = scratch_root("independent-theme-fields");
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let mut provider = ApplicationSettingsProvider::load(&root).unwrap();
    provider
        .apply(&reference, "theme.mode", SettingValue::Text("light".into()))
        .unwrap();
    assert_eq!(
        provider.settings().theme,
        Some(ThemeChoice::new("", Some(Mode::Light)))
    );
    provider
        .apply(
            &reference,
            "theme.id",
            SettingValue::Text("theme:dark".into()),
        )
        .unwrap();
    assert_eq!(
        provider.settings().theme,
        Some(ThemeChoice::new("theme:dark", Some(Mode::Light)))
    );
    provider
        .apply(&reference, "theme.mode", SettingValue::Text("dark".into()))
        .unwrap();
    provider
        .apply(&reference, "theme.id", SettingValue::Text(String::new()))
        .unwrap();
    assert_eq!(
        provider.settings().theme,
        Some(ThemeChoice::new("", Some(Mode::Dark)))
    );
    let restored = ApplicationSettingsProvider::load(&root).unwrap();
    assert_eq!(restored.settings().theme, provider.settings().theme);
    let snapshot = crate::settings_pane::ChromeSettings::from(restored.settings());
    assert_eq!(snapshot.theme_id(), None);
    assert_eq!(snapshot.theme_mode(), Some("dark"));
    provider
        .apply(
            &reference,
            "theme.mode",
            SettingValue::Text("system".into()),
        )
        .unwrap();
    assert_eq!(provider.settings().theme, None);
    provider
        .apply(
            &reference,
            "theme.id",
            SettingValue::Text("theme:dark".into()),
        )
        .unwrap();
    provider
        .apply(&reference, "theme.mode", SettingValue::Text("light".into()))
        .unwrap();
    provider
        .apply(
            &reference,
            "theme.mode",
            SettingValue::Text("system".into()),
        )
        .unwrap();
    assert_eq!(
        provider.settings().theme,
        Some(ThemeChoice::new("theme:dark", None))
    );
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
