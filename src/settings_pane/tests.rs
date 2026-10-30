// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use layout_dom_api::LayoutDom;

fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "turnstone-settings-pane-{label}-{}",
        std::process::id()
    ))
}

#[test]
fn pane_renders_controls_from_setting_control_not_setting_ids() {
    let library = tempfile::tempdir().unwrap();
    let mut pane = SettingsPane::new(root("render"));
    pane.runner.update(|state| {
        state.provider = ApplicationSettingsProvider::from_settings(
            root("render"),
            ApplicationSettings::default(),
        )
        .with_theme_library(library.path().join("themes.json"));
    });
    let dom = pane.dom_ref();
    assert_eq!(dom.all_with_class(dom.document(), "setting-row").len(), 6);
    assert_eq!(dom.all_with_class(dom.document(), "setting-label").len(), 6);
    assert_eq!(dom.all_with_class(dom.document(), "setting-apply").len(), 7);
    // UI zoom and cascade budget are number controls.
    assert_eq!(dom.all_with_class(dom.document(), "slider-track").len(), 2);
    assert_eq!(dom.all_with_class(dom.document(), "toggle").len(), 1);
    // Six shared built-ins plus the Turnstone default, five mode choices,
    // and four shellbar edges are controlled choices in the actual pane.
    assert_eq!(dom.all_with_class(dom.document(), "radio").len(), 16);
}

#[test]
fn actual_edit_themes_button_requests_host_work_without_changing_selection() {
    let root = tempfile::tempdir().unwrap();
    let live = LiveSettingsHandle::new(&ApplicationSettings::default());
    let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    let before = live.snapshot();
    let node = taproot::matching_estimated_fixture(
        &*pane.dom_ref(),
        &taproot::Selector::role("button").with_attr("data-action", "edit-themes"),
    )[0];
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
    assert!(matches!(
        live.take_actions().as_slice(),
        [crate::action::Action::OpenThemeWorkshop]
    ));
    assert!(
        live.take_actions().is_empty(),
        "the host consumes each request once"
    );
    assert_eq!(
        live.snapshot(),
        before,
        "opening a workshop does not select a theme"
    );
    assert!(!pandect::application_settings_exist(root.path()));
}

#[test]
fn provider_owner_is_available_to_host_after_projection_construction() {
    let pane = SettingsPane::new(root("owner"));
    assert_eq!(pane.settings().ui_zoom, 1.1);
    assert_eq!(pane.settings().theme, None);
}

#[test]
fn successful_apply_publishes_a_value_snapshot_for_the_shell() {
    let settings = ApplicationSettings::default();
    let live = LiveSettingsHandle::new(&settings);
    let mut pane = SettingsPane::with_live_settings(root("live"), Some(live.clone()));
    pane.runner.update(|state| {
        apply_value(
            state,
            "chrome.shellbar.visible",
            SettingValue::Boolean(false),
        );
    });
    assert!(!live.snapshot().shellbar_visible());
    assert_eq!(live.snapshot().shellbar_edge(), ShellbarEdge::Left);
    let mut app = crate::app::App::test_stub();
    assert!(app.apply_chrome_settings_snapshot(&live.snapshot()));
    assert!(
        !app.shell_chrome_config().shellbar.visible,
        "the shell can observe the same persisted snapshot without a pane callback"
    );
}

#[test]
fn saved_same_identity_waits_for_explicit_apply_and_reopens_latest_definition() {
    use tabard::{
        Theme,
        library::ThemeLibraryStore,
        theme::{
            choice::ThemeChoice,
            registry::{Mode, ThemeRegistry},
        },
    };
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("shared/themes.json");
    let mut theme = Theme::new(
        "theme:turnstone-held",
        "Held appearance",
        ThemeRegistry::default().list()[0].seeds,
    );
    theme.mode_sheets.insert(
        "dark".into(),
        vec![":root{--tabard-color-bg:#123456;}".into()],
    );
    ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[theme.clone()])
        .unwrap();
    let choice = ThemeChoice::new(theme.id.clone(), Some(Mode::Dark));
    let live = LiveSettingsHandle::new(&ApplicationSettings::default());
    let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    pane.runner.update(|state| {
        state.provider =
            ApplicationSettingsProvider::from_settings(root.path(), ApplicationSettings::default())
                .with_theme_library(&library);
        state.provider.apply_theme_choice(choice.clone()).unwrap();
        state.live_settings.apply_provider(&state.provider);
    });
    let applied = live.snapshot();
    let persisted = std::fs::read(pandect::application_settings_path(root.path())).unwrap();
    theme.mode_sheets.insert(
        "dark".into(),
        vec![":root{--tabard-color-bg:#654321;}".into()],
    );
    ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[theme])
        .unwrap();
    pane.reload_themes();
    assert_eq!(
        live.snapshot(),
        applied,
        "saving and refreshing a same-id definition must not implicitly apply"
    );
    assert_eq!(
        std::fs::read(pandect::application_settings_path(root.path())).unwrap(),
        persisted
    );
    // A second Settings pane shares the applied presentation rather than
    // resolving the edited library into the live shell on construction.
    let _second = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    assert_eq!(live.snapshot(), applied);
    pane.runner
        .update(|state| apply_value(state, "ui.zoom", SettingValue::Number(1.5)));
    assert_eq!(live.snapshot().presentation, applied.presentation);
    assert_eq!(live.snapshot().ui_zoom(), 1.5);
    // A new process resolves the saved definition; an explicit same-mode
    // Apply in the existing process reaches that same fresh presentation.
    let reopened = ApplicationSettingsProvider::load(root.path())
        .unwrap()
        .with_theme_library(&library);
    let restarted = LiveSettingsHandle::new(reopened.settings());
    restarted.publish_provider(&reopened);
    assert_ne!(restarted.snapshot().presentation, applied.presentation);
    pane.runner
        .update(|state| apply_value(state, "theme.mode", SettingValue::Text("dark".into())));
    assert_eq!(live.snapshot(), restarted.snapshot());
    assert_eq!(pane.settings().theme, Some(choice));
}

#[test]
fn stale_settings_panes_cannot_apply_saved_definition_or_overwrite_selected_theme() {
    use tabard::{
        Theme,
        library::ThemeLibraryStore,
        theme::{
            choice::ThemeChoice,
            registry::{Mode, ThemeRegistry},
        },
    };
    for reverse in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("shared/themes.json");
        let mut theme = Theme::new(
            "theme:retained-choice",
            "Retained choice",
            ThemeRegistry::default().list()[0].seeds,
        );
        theme.mode_sheets.insert(
            "dark".into(),
            vec![":root{--tabard-color-bg:#123456;}".into()],
        );
        ThemeLibraryStore::load(&library)
            .unwrap()
            .save(&[theme.clone()])
            .unwrap();
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut first = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        let mut stale = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        for pane in [&mut first, &mut stale] {
            pane.runner.update(|state| {
                state.provider = ApplicationSettingsProvider::from_settings(
                    root.path(),
                    ApplicationSettings::default(),
                )
                .with_theme_library(&library);
            });
        }
        let choice = ThemeChoice::new(theme.id.clone(), Some(Mode::Dark));
        first.runner.update(|state| {
            state.provider.apply_theme_choice(choice.clone()).unwrap();
            state.live_settings.apply_provider(&state.provider);
        });
        let applied = live.snapshot();
        let saved_choice = std::fs::read(pandect::application_settings_path(root.path())).unwrap();
        theme.mode_sheets.insert(
            "dark".into(),
            vec![":root{--tabard-color-bg:#654321;}".into()],
        );
        ThemeLibraryStore::load(&library)
            .unwrap()
            .save(&[theme])
            .unwrap();
        if reverse {
            stale.reload_themes();
            first.reload_themes();
        } else {
            first.reload_themes();
            stale.reload_themes();
        }
        let owner = ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .with_theme_library(&library);
        live.publish_provider(&owner);
        assert_eq!(
            live.snapshot(),
            applied,
            "both retained-pane refresh orders must keep the applied definition"
        );
        assert_eq!(
            std::fs::read(pandect::application_settings_path(root.path())).unwrap(),
            saved_choice
        );
        stale
            .runner
            .update(|state| apply_value(state, "ui.zoom", SettingValue::Number(1.5)));
        assert_eq!(live.snapshot().theme_id, applied.theme_id);
        assert_eq!(live.snapshot().theme_mode, applied.theme_mode);
        assert_eq!(live.snapshot().presentation, applied.presentation);
        assert_eq!(live.snapshot().ui_zoom(), 1.5);
        assert_eq!(
            ApplicationSettingsProvider::load(root.path())
                .unwrap()
                .settings()
                .theme,
            Some(choice)
        );
        stale
            .runner
            .update(|state| apply_value(state, "theme.mode", SettingValue::Text("dark".into())));
        assert_ne!(
            live.snapshot().presentation,
            applied.presentation,
            "only explicit Apply adopts the saved definition"
        );
    }
}

#[test]
fn snapshot_projects_every_live_application_axis_to_chrome() {
    let settings = ApplicationSettings {
        theme: Some(tabard::theme::choice::ThemeChoice::new(
            "theme:night",
            Some(tabard::theme::registry::Mode::Light),
        )),
        ui_zoom: 1.75,
        shellbar_edge: ShellbarEdge::Bottom,
        shellbar_hidden: true,
        ..ApplicationSettings::default()
    };
    let mut chrome = ShellChromeConfig::default();
    ChromeSettings::from(&settings).apply_to(&mut chrome);

    assert_eq!(
        chrome.shellbar.placement,
        crate::panes::ChromePlacement::Docked(crate::panes::ChromeEdge::Bottom)
    );
    assert!(!chrome.shellbar.visible);
    assert_eq!(chrome.appearance.theme_id.as_deref(), Some("theme:night"));
    assert_eq!(chrome.appearance.theme_mode, ThemeMode::Light);
    assert_eq!(chrome.appearance.zoom(), 1.75);
}
#[test]
fn painted_settings_targets_drive_physical_edit_and_apply_at_wide_and_narrow_sizes() {
    // Main acceptance requests logical 1180x800 and 640x780 windows.
    // At the native 2x scale the right-hand Settings pane has these sizes.
    for (width, height) in [(1180, 1600), (640, 1560)] {
        let root = tempfile::tempdir().unwrap();
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        pane.runner.update(|state| {
            state.provider = ApplicationSettingsProvider::from_settings(
                root.path(),
                ApplicationSettings::default(),
            )
            .with_theme_library(root.path().join("themes.json"));
        });
        pane.sync(width as f32, height as f32);
        let _ = pane.scene(width, height, 1.0);
        let edit = taproot::Selector::class("setting-apply")
            .with_attr("data-action", "edit-themes")
            .on_surface("settings");
        let retained = pane.selector_point(&edit).unwrap().unwrap();
        let generic = {
            let dom = pane.dom_ref();
            let sheet = crate::ui::cambium_sheet(&ShellChromeConfig::default().appearance);
            taproot::resolve_estimated_fixture(
                &[taproot::ProbeSurface {
                    name: "settings",
                    dom: &dom,
                    rect: [0.0, 0.0, width as f32, height as f32],
                    sheet: &sheet,
                }],
                &edit,
            )
            .map(|hit| hit.point)
        };
        let generic_hits_edit = generic.is_some_and(|(x, y)| {
            let sheet = crate::ui::cambium_sheet(&ShellChromeConfig::default().appearance);
            let mut dom = pane.dom.borrow_mut();
            let edit_node = taproot::matching_estimated_fixture(&dom, &edit)[0];
            pane.layout
                .hit_test_scrolled(&mut dom, &sheet, width, height, x, y, &pane.scroll)
                == Some(edit_node)
        });
        eprintln!(
            "settings {width}x{height}: generic={generic:?} retained={retained:?} generic_hits_edit={generic_hits_edit}"
        );
        let before = live.snapshot();
        pane.click(retained.0, retained.1, width, height);
        assert!(matches!(
            live.take_actions().as_slice(),
            [crate::action::Action::OpenThemeWorkshop]
        ));
        assert_eq!(live.snapshot(), before, "Edit themes cannot apply settings");
        assert!(!pandect::application_settings_exist(root.path()));

        // Dark occurs in both theme.id and theme.mode. Reject it rather
        // than assigning the first occurrence to the wrong row.
        assert!(
            pane.selector_point(
                &taproot::Selector::class("radio")
                    .containing("Dark")
                    .on_surface("settings")
            )
            .is_err()
        );
        assert!(
            pane.selector_point(&taproot::Selector::class("setting-apply").on_surface("settings"))
                .is_err()
        );
        assert_eq!(
            pane.selector_point(&edit.clone().on_surface("inspector")),
            Ok(None)
        );

        fn physical_click(
            pane: &mut SettingsPane,
            selector: taproot::Selector,
            width: u32,
            height: u32,
        ) {
            let _ = pane.scene(width, height, 1.0);
            let point = pane
                .selector_point(&selector.on_surface("settings"))
                .unwrap()
                .expect("painted visible control");
            pane.click(point.0, point.1, width, height);
        }
        physical_click(
            &mut pane,
            taproot::Selector::class("radio").containing("High Contrast"),
            width,
            height,
        );
        assert!(
            live.snapshot().theme_id().is_none(),
            "radio is still a draft"
        );
        physical_click(
            &mut pane,
            taproot::Selector::class("setting-apply").with_attr("data-setting", "theme.id"),
            width,
            height,
        );
        assert_eq!(
            live.snapshot().theme_id(),
            Some(tabard::theme::registry::THEME_ID_HIGH_CONTRAST)
        );
        physical_click(
            &mut pane,
            taproot::Selector::class("radio").containing("High contrast light"),
            width,
            height,
        );
        assert!(
            live.snapshot().theme_mode().is_none(),
            "mode is still a draft"
        );
        physical_click(
            &mut pane,
            taproot::Selector::class("setting-apply").with_attr("data-setting", "theme.mode"),
            width,
            height,
        );
        assert_eq!(live.snapshot().theme_mode(), Some("hc_light"));
        let saved = pandect::load_application_settings(root.path())
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.theme.unwrap(),
            tabard::theme::choice::ThemeChoice::new(
                tabard::theme::registry::THEME_ID_HIGH_CONTRAST,
                Some(tabard::theme::registry::Mode::HcLight)
            )
        );
        assert!(
            live.take_actions().is_empty(),
            "Apply cannot open the workshop"
        );
    }
}

#[test]
fn mode_radio_keyboard_stays_in_its_group_and_enter_applies_only_after_tab() {
    use cambium::{Key, KeyEvent, NamedKey};
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("themes.json");
    let live = LiveSettingsHandle::new(&ApplicationSettings::default());
    let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    pane.runner.update(|state| {
        state.provider =
            ApplicationSettingsProvider::from_settings(root.path(), ApplicationSettings::default())
                .with_theme_library(library.clone());
    });
    // The first click selects HcLight; the second focuses the now-active
    // roving-tabindex radio, exactly as the native fixture does.
    let selector = taproot::Selector::class("radio").containing("High contrast light");
    for _ in 0..2 {
        let node = taproot::matching_estimated_fixture(&*pane.dom_ref(), &selector)[0];
        let _: Vec<()> = pane
            .runner
            .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
    }
    pane.key(KeyEvent::new(Key::Named(NamedKey::ArrowUp)));
    // Arrow navigation is a draft, not a durable change.
    assert!(pane.settings().theme.is_none());
    assert!(live.snapshot().theme_mode().is_none());
    pane.focus_traverse(true);
    {
        let dom = pane.dom_ref();
        let apply = taproot::matching_estimated_fixture(
            &*dom,
            &taproot::Selector::class("setting-apply").with_attr("data-setting", "theme.mode"),
        )[0];
        assert_eq!(pane.runner.focus(), Some(apply));
    }
    pane.key(KeyEvent::new(Key::Named(NamedKey::Enter)));
    assert_eq!(live.snapshot().theme_mode(), Some("dark"));
    assert_eq!(
        live.snapshot().theme_id(),
        None,
        "mode keyboard cannot select the built-in Dark theme row"
    );
    let persisted = ApplicationSettingsProvider::load(root.path()).unwrap();
    assert_eq!(
        persisted
            .settings()
            .theme
            .as_ref()
            .unwrap()
            .theme_mode
            .as_ref()
            .unwrap()
            .as_key(),
        "dark"
    );
    // Repeat the visible group entry, then two ArrowUp presses choose Light.
    for _ in 0..2 {
        let node = taproot::matching_estimated_fixture(&*pane.dom_ref(), &selector)[0];
        let _: Vec<()> = pane
            .runner
            .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
    }
    pane.key(KeyEvent::new(Key::Named(NamedKey::ArrowUp)));
    pane.key(KeyEvent::new(Key::Named(NamedKey::ArrowUp)));
    pane.focus_traverse(true);
    pane.key(KeyEvent::new(Key::Named(NamedKey::Enter)));
    assert_eq!(live.snapshot().theme_mode(), Some("light"));
    assert_eq!(live.snapshot().theme_id(), None);
    pane.focus_traverse(false);
    assert!(pane.runner.focus().is_some());
    pane.blur();
    assert!(pane.runner.focus().is_none());
}

#[test]
fn a_saved_system_theme_wears_the_os_preference() {
    use tabard::theme::{choice::ThemeChoice, registry::THEME_ID_DEFAULT};
    let settings = ApplicationSettings {
        theme: Some(ThemeChoice::new(THEME_ID_DEFAULT, None)),
        ..ApplicationSettings::default()
    };
    let snapshot = ChromeSettings::from(&settings);
    let ground = |system_dark: bool| {
        let mut chrome = ShellChromeConfig::default();
        chrome.appearance.system_dark = system_dark;
        snapshot.apply_to(&mut chrome);
        let appearance = &chrome.appearance;
        let bg = tabard::presentation_palette(
            appearance.theme_presentation.as_ref().unwrap(),
            &appearance.tabard_mode(),
            THEME_ID_DEFAULT,
        )
        .palette
        .bg;
        u32::from(bg.r) + u32::from(bg.g) + u32::from(bg.b)
    };
    assert!(ground(false) > ground(true) + 300, "light OS, light ground");
}
