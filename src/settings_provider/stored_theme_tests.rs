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
fn a_settings_file_written_before_the_stored_theme_split_loads_the_same_theme() {
    // Bytes as `ThemeChoice` wrote them, flattened into the settings file.
    let written = r#"{"theme_id":"theme:dark","theme_mode":"hc_dark","fonts":{"interface":["Absent face","serif"],"reading":["monospace"]},"ui_zoom":1.5}"#;
    let root = scratch_root("legacy-theme-bytes");
    let path = application_settings_path(&root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, written).unwrap();
    let mut provider = ApplicationSettingsProvider::load(&root).unwrap();
    let choice = crate::appearance::choice_of(provider.settings().theme.as_ref().unwrap());
    let mut expected = ThemeChoice::new("theme:dark", Some(Mode::HcDark));
    expected.fonts =
        serde_json::from_str(r#"{"interface":["Absent face","serif"],"reading":["monospace"]}"#)
            .unwrap();
    assert_eq!(choice, expected);
    // Saving another setting writes the same theme fields back.
    provider
        .apply(
            &SettingsRef(APPLICATION_REFERENCE.into()),
            "ui.zoom",
            SettingValue::Number(1.25),
        )
        .unwrap();
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let before: serde_json::Value = serde_json::from_str(written).unwrap();
    for key in ["theme_id", "theme_mode", "fonts"] {
        assert_eq!(after[key], before[key], "{key}");
    }
    assert_eq!(
        serde_json::to_string(&serde_json::to_value(&expected).unwrap()).unwrap(),
        serde_json::to_string(
            &serde_json::to_value(provider.settings().theme.as_ref().unwrap()).unwrap()
        )
        .unwrap(),
        "stored and Tabard forms serialize identically"
    );
    std::fs::remove_dir_all(root).unwrap();
}
