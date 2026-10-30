// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use layout_dom_api::{LayoutDom, LocalName, Namespace};

fn panes() -> (
    tempfile::TempDir,
    LiveSettingsHandle,
    SettingsPane,
    SettingsPane,
) {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let first = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    let second = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    (root, live, first, second)
}

fn click(pane: &mut SettingsPane, selector: taproot::Selector) {
    let nodes = taproot::matching_estimated_fixture(&*pane.dom_ref(), &selector);
    assert_eq!(nodes.len(), 1, "selector must identify one actual control");
    let _: Vec<()> = pane
        .runner
        .dispatch_click(nodes[0], cambium::PointerClick::at((0.0, 0.0)));
}

fn visible_draft(pane: &SettingsPane) -> Option<String> {
    let dom = pane.dom_ref();
    let toggle = dom.all_with_class(dom.document(), "toggle")[0];
    dom.attribute(
        toggle,
        &Namespace::default(),
        &LocalName::from("aria-checked"),
    )
    .map(str::to_owned)
}

fn toggle_selector() -> taproot::Selector {
    taproot::Selector::class("toggle")
}

fn apply_selector() -> taproot::Selector {
    taproot::Selector::class("setting-apply").with_attr("data-setting", "chrome.shellbar.visible")
}

fn draft_toggle_off(pane: &mut SettingsPane) {
    click(pane, toggle_selector());
    assert_eq!(visible_draft(pane).as_deref(), Some("false"));
}

fn apply_toggle(pane: &mut SettingsPane) {
    click(pane, apply_selector());
}

#[test]
fn unrelated_behaviors_revision_preserves_dirty_toggle_draft_and_apply_succeeds() {
    let (_root, live, mut first, mut second) = panes();
    draft_toggle_off(&mut first);
    second.runner.update(|state| {
        apply_value(state, "behaviors.cascade_budget", SettingValue::Number(9.0));
    });

    first.sync(640.0, 780.0);
    assert_eq!(visible_draft(&first).as_deref(), Some("false"));
    assert_eq!(first.settings().cascade_budget, 9);
    apply_toggle(&mut first);
    assert!(first.settings().shellbar_hidden);
    assert!(!live.snapshot().shellbar_visible());
    assert_eq!(second.settings().cascade_budget, 9);
    assert!(
        pandect::load_application_settings(_root.path())
            .unwrap()
            .unwrap()
            .shellbar_hidden
    );
}

fn same_domain_aba(first: &mut SettingsPane, second: &mut SettingsPane) {
    draft_toggle_off(first);
    // Change the same domain away and back. The committed value equals the
    // pane's original baseline, while the owner's revision has advanced twice.
    for _ in 0..2 {
        click(second, toggle_selector());
        apply_toggle(second);
    }
    assert_eq!(second.settings().shellbar_hidden, false);
}

#[test]
fn equal_value_same_domain_aba_keeps_draft_but_apply_conflicts_without_write() {
    let (_root, _live, mut first, mut second) = panes();
    same_domain_aba(&mut first, &mut second);
    first.sync(640.0, 780.0);
    assert_eq!(visible_draft(&first).as_deref(), Some("false"));
    apply_toggle(&mut first);
    assert!(!first.settings().shellbar_hidden);
    assert!(
        first
            .runner
            .state()
            .status
            .contains("changed since this edit began")
    );
    assert!(
        !ApplicationSettingsProvider::load(_root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
}

#[test]
fn explicit_discard_and_reload_resets_conflicted_draft_before_next_apply() {
    let (_root, _live, mut first, mut second) = panes();
    same_domain_aba(&mut first, &mut second);
    first.sync(640.0, 780.0);
    assert_eq!(visible_draft(&first).as_deref(), Some("false"));

    let session = first
        .runner
        .state()
        .rows
        .values()
        .find(|row| row.setting_id == "chrome.shellbar.visible")
        .unwrap()
        .session
        .clone();
    let reset_generation = session.snapshot().reset_generation;
    let toggle = taproot::matching_estimated_fixture(&*first.dom_ref(), &toggle_selector())[0];
    click(
        &mut first,
        taproot::Selector::class("setting-reload").with_attr("data-action", "reload-settings"),
    );
    assert_eq!(visible_draft(&first).as_deref(), Some("true"));
    assert!(session.snapshot().reset_generation > reset_generation);
    assert_eq!(
        taproot::matching_estimated_fixture(&*first.dom_ref(), &toggle_selector()),
        vec![toggle]
    );

    draft_toggle_off(&mut first);
    apply_toggle(&mut first);
    assert!(first.settings().shellbar_hidden);
    assert_eq!(first.runner.state().status, "Saved.");
}

#[test]
fn own_same_domain_zoom_commit_advances_sibling_guard_without_losing_its_draft() {
    let (root, _live, mut pane, _other) = panes();
    draft_toggle_off(&mut pane);
    pane.runner.update(|state| {
        apply_value(state, "ui.zoom", SettingValue::Number(1.5));
    });
    assert_eq!(visible_draft(&pane).as_deref(), Some("false"));

    apply_toggle(&mut pane);
    assert_eq!(pane.runner.state().status, "Saved.");
    let saved = ApplicationSettingsProvider::load(root.path()).unwrap();
    assert_eq!(saved.settings().ui_zoom, 1.5);
    assert!(saved.settings().shellbar_hidden);
}

#[test]
fn own_zoom_commit_does_not_rebase_visibility_guard_staler_than_that_commit() {
    let (root, _live, mut pane_a, mut pane_b) = panes();
    draft_toggle_off(&mut pane_a);

    pane_b.runner.update(|state| {
        apply_value(state, "ui.zoom", SettingValue::Number(1.5));
    });
    pane_a.sync(640.0, 780.0);
    assert_eq!(pane_a.settings().ui_zoom, 1.5);
    assert_eq!(visible_draft(&pane_a).as_deref(), Some("false"));

    // The changed zoom spec adopts the external revision. The visibility row
    // remains based on revision zero, so this local zoom commit must not grant
    // that older visibility draft the intervening write's newer baseline.
    pane_a.runner.update(|state| {
        apply_value(state, "ui.zoom", SettingValue::Number(2.0));
    });
    assert_eq!(pane_a.settings().ui_zoom, 2.0);
    assert_eq!(visible_draft(&pane_a).as_deref(), Some("false"));
    apply_toggle(&mut pane_a);
    assert!(
        pane_a
            .runner
            .state()
            .status
            .contains("changed since this edit began")
    );

    let saved = ApplicationSettingsProvider::load(root.path()).unwrap();
    assert_eq!(saved.settings().ui_zoom, 2.0);
    assert!(!saved.settings().shellbar_hidden);
}
