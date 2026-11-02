// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use layout_dom_api::LayoutDom;

/// Re-render the real production row helper with one extra same-control,
/// same-value row inserted before `theme.mode`. The status string is only
/// a fixture switch; row rendering and Apply callbacks are production code.
fn identity_regression_view(state: &SettingsState) -> SettingsView {
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let mut specs = SettingsProjection::resolve(&state.provider, &reference)
        .expect("application settings projection")
        .specs;
    if matches!(state.status.as_str(), "inserted" | "reordered-before") {
        let mut inserted = specs
            .iter()
            .find(|spec| spec.id == "theme.mode")
            .expect("theme mode spec")
            .clone();
        inserted.id = "test.theme.mode.sibling".into();
        let mode_index = specs
            .iter()
            .position(|spec| spec.id == "theme.mode")
            .unwrap();
        specs.insert(mode_index, inserted);
    } else if state.status == "reordered-after" {
        let mut inserted = specs
            .iter()
            .find(|spec| spec.id == "theme.mode")
            .expect("theme mode spec")
            .clone();
        inserted.id = "test.theme.mode.sibling".into();
        let mode_index = specs
            .iter()
            .position(|spec| spec.id == "theme.mode")
            .unwrap();
        specs.insert(mode_index + 1, inserted);
    }
    setting_rows(&specs)
}

fn identity_test_pane(root: &std::path::Path, status: &str) -> SettingsPane {
    let initial = ApplicationSettings {
        theme: Some(tabard::theme::choice::ThemeChoice::new(
            tabard::theme::registry::THEME_ID_HIGH_CONTRAST,
            None,
        )),
        ..ApplicationSettings::default()
    };
    let provider = ApplicationSettingsProvider::from_settings(root, initial);
    let live_settings = LiveSettingsHandle::from_provider(provider);
    let provider = live_settings
        .owner()
        .expect("provider-backed test owner")
        .provider_snapshot();
    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let document = dom.borrow().document();
    let runner = SettingsRunner::new_at(
        dom.clone(),
        document,
        identity_regression_view as fn(&SettingsState) -> SettingsView,
        SettingsState::new(provider, live_settings, status.into()),
    );
    SettingsPane {
        dom,
        runner,
        scroll: crate::ui::PaneScroll::new(),
        layout: crate::ui::RetainedLayout::new_at(None),
        mount: None,
    }
}

fn setting_radio_checked(pane: &SettingsPane, setting_id: &str, label: &str) -> bool {
    let dom = pane.dom_ref();
    for node in taproot::matching_estimated_fixture(
        &*dom,
        &taproot::Selector::class("radio").containing(label),
    ) {
        let mut ancestor = Some(node);
        while let Some(current) = ancestor {
            if dom
                .attribute(
                    current,
                    &layout_dom_api::Namespace::default(),
                    &layout_dom_api::LocalName::from("data-setting"),
                )
                .is_some_and(|id| id == setting_id)
            {
                return dom
                    .attribute(
                        node,
                        &layout_dom_api::Namespace::default(),
                        &layout_dom_api::LocalName::from("aria-checked"),
                    )
                    .is_some_and(|checked| checked == "true");
            }
            ancestor = dom.parent(current);
        }
    }
    false
}

fn apply_identity_setting(pane: &mut SettingsPane, setting_id: &str) {
    // Taproot attribute selectors deliberately match substrings. Select
    // the exact binding here: the sibling id also contains "theme.mode".
    let node = {
        let dom = pane.dom_ref();
        let matches = dom
            .all_with_class(dom.document(), "setting-apply")
            .into_iter()
            .filter(|&node| {
                dom.attribute(
                    node,
                    &layout_dom_api::Namespace::default(),
                    &layout_dom_api::LocalName::from("data-setting"),
                ) == Some(setting_id)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "one Apply button for {setting_id}");
        matches[0]
    };
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
}

#[test]
fn keyed_settings_rows_keep_drafts_and_apply_to_the_original_id_after_insertion() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = identity_test_pane(root.path(), "");

    // Change the real mode row's draft through a DOM event. High contrast
    // light belongs to the selected built-in theme; the sibling inserted
    // below has the exact same control and committed system value.
    let mode_choice = taproot::Selector::class("radio").containing("High contrast light");
    let mode_node = taproot::matching_estimated_fixture(&*pane.dom_ref(), &mode_choice)[0];
    let _: Vec<()> = pane
        .runner
        .dispatch_click(mode_node, cambium::PointerClick::at((0.0, 0.0)));
    assert!(pane.settings().theme.as_ref().unwrap().theme_mode.is_none());

    // Insert a duplicate-control/value sibling immediately before the
    // edited row, changing its old positional index while preserving its id.
    pane.runner.update(|state| state.status = "inserted".into());
    assert!(setting_radio_checked(
        &pane,
        "theme.mode",
        "High contrast light"
    ));
    assert!(setting_radio_checked(
        &pane,
        "test.theme.mode.sibling",
        "Theme default / system"
    ));
    apply_identity_setting(&mut pane, "theme.mode");

    assert_eq!(pane.runner.state().status, "Saved.");
    let saved = ApplicationSettingsProvider::load(root.path()).unwrap();
    assert_eq!(
        saved.settings().theme.as_ref().unwrap().theme_mode,
        Some(tabard::theme::registry::Mode::HcLight),
        "the retained draft must remain on and apply through theme.mode"
    );
    assert_eq!(
        saved.settings().theme.as_ref().unwrap().theme_id,
        tabard::theme::registry::THEME_ID_HIGH_CONTRAST,
        "the inserted sibling and theme.id row must not receive the draft"
    );
}

#[test]
fn keyed_settings_rows_keep_drafts_on_the_original_id_when_sibling_reorders() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = identity_test_pane(root.path(), "reordered-after");
    let mode_choice = taproot::Selector::class("radio").containing("High contrast light");
    let mode_node = taproot::matching_estimated_fixture(&*pane.dom_ref(), &mode_choice)[0];
    let _: Vec<()> = pane
        .runner
        .dispatch_click(mode_node, cambium::PointerClick::at((0.0, 0.0)));

    // The sibling starts after the edited row, then moves across it. The
    // original mode draft stays selected; the sibling retains its clean
    // system value throughout the keyed reorder.
    pane.runner
        .update(|state| state.status = "reordered-before".into());
    assert!(setting_radio_checked(
        &pane,
        "theme.mode",
        "High contrast light"
    ));
    assert!(setting_radio_checked(
        &pane,
        "test.theme.mode.sibling",
        "Theme default / system"
    ));

    apply_identity_setting(&mut pane, "theme.mode");
    assert_eq!(pane.runner.state().status, "Saved.");
    let saved = ApplicationSettingsProvider::load(root.path()).unwrap();
    assert_eq!(
        saved.settings().theme.as_ref().unwrap().theme_mode,
        Some(tabard::theme::registry::Mode::HcLight)
    );
    assert_eq!(
        saved.settings().theme.as_ref().unwrap().theme_id,
        tabard::theme::registry::THEME_ID_HIGH_CONTRAST
    );
}

#[test]
fn panes_share_the_same_provider_owner_and_sync_other_committed_values() {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let mut first = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    let mut second = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    assert_eq!(first.settings().ui_zoom, 1.1);
    second.runner.update(|state| {
        apply_value(state, "ui.zoom", SettingValue::Number(1.8));
    });
    assert_eq!(
        first.settings().ui_zoom,
        1.1,
        "a cached pane refreshes at its next sync"
    );
    first.sync(640.0, 780.0);
    assert_eq!(first.settings().ui_zoom, 1.8);
    assert_eq!(second.settings().ui_zoom, 1.8);
    assert_eq!(live.owner().unwrap().data_root(), root.path().to_path_buf());
}

#[test]
fn pane_rejects_a_different_root_without_replacing_or_writing_shared_owner() {
    let shared_root = tempfile::tempdir().unwrap();
    let other_root = tempfile::tempdir().unwrap();
    let live = LiveSettingsHandle::from_provider(
        ApplicationSettingsProvider::load(shared_root.path()).unwrap(),
    );
    let before = live.snapshot();
    let mut wrong_root =
        SettingsPane::with_live_settings(other_root.path().into(), Some(live.clone()));
    assert!(wrong_root.runner.state().owner.is_none());
    wrong_root.runner.update(|state| {
        apply_value(state, "ui.zoom", SettingValue::Number(2.0));
    });
    assert_eq!(live.snapshot(), before);
    assert_eq!(
        live.owner().unwrap().data_root(),
        shared_root.path().to_path_buf()
    );
    assert!(!pandect::application_settings_exist(shared_root.path()));
    assert!(!pandect::application_settings_exist(other_root.path()));
}
