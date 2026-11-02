// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium::composition::PlacementNode;
use layout_dom_api::{LayoutDom, LocalName, Namespace};

const THEME_ID: &str = "theme.id";
const SIBLING_ID: &str = "test.theme.id.sibling";

fn composition_test_view(state: &SettingsState) -> SettingsView {
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let mut specs = SettingsProjection::resolve(&state.provider, &reference)
        .expect("application settings projection")
        .specs;
    let original = specs
        .iter_mut()
        .find(|spec| spec.id == THEME_ID)
        .expect("theme id spec");
    original.control = mere_surface_api::settings::SettingControl::Text;
    let sibling = SettingSpec {
        id: SIBLING_ID.into(),
        ..original.clone()
    };
    if matches!(
        state.status.as_str(),
        "inserted" | "wrapper-a" | "wrapper-b" | "hidden" | "collapsed"
    ) {
        let position = specs.iter().position(|spec| spec.id == THEME_ID).unwrap();
        specs.insert(position, sibling);
    } else if state.status == "reordered" {
        let position = specs.iter().position(|spec| spec.id == THEME_ID).unwrap() + 1;
        specs.insert(position, sibling);
    }

    let mut model = composition::default_settings_model(&specs);
    let original_key = composition::component_key(THEME_ID);
    let sibling_key = composition::component_key(SIBLING_ID);
    let mut collapsed: std::collections::BTreeSet<cambium::composition::ContainerKey> =
        Default::default();
    match state.status.as_str() {
        "wrapper-a" | "wrapper-b" | "collapsed" => {
            let group_key = if state.status == "wrapper-b" {
                "replacement-wrapper"
            } else {
                "settings-wrapper"
            };
            let mut placement = Vec::new();
            for node in model.placement.drain(..) {
                if matches!(&node, PlacementNode::Component { key } if key == &original_key)
                    || (state.status == "collapsed"
                        && matches!(&node, PlacementNode::Component { key } if key == &sibling_key))
                {
                    if placement.iter().all(|node| {
                        !matches!(node, PlacementNode::Group { key, .. } if key.0 == group_key)
                    }) {
                        placement.push(PlacementNode::Group {
                            key: group_key.into(),
                            children: Vec::new(),
                        });
                    }
                    if let Some(PlacementNode::Group {
                        children: group_children,
                        ..
                    }) = placement.last_mut()
                    {
                        group_children.push(node);
                    }
                } else {
                    placement.push(node);
                }
            }
            model.placement = placement;
            if state.status == "collapsed" {
                collapsed.insert(cambium::composition::ContainerKey {
                    kind: cambium::composition::ContainerKind::Group,
                    key: group_key.into(),
                });
            }
        },
        "hidden" => {
            model.placement.retain(
                |node| !matches!(node, PlacementNode::Component { key } if key == &original_key),
            );
            model.unplaced.push(original_key);
        },
        _ => {},
    }
    composition::setting_rows_with_sink(
        &specs,
        model,
        collapsed,
        state.composition_lifecycle.clone(),
    )
}

fn pane(root: &std::path::Path) -> SettingsPane {
    let initial = ApplicationSettings {
        theme: Some(tabard::theme::choice::ThemeChoice::new(
            tabard::theme::registry::THEME_ID_HIGH_CONTRAST,
            None,
        )),
        ..ApplicationSettings::default()
    };
    let provider = ApplicationSettingsProvider::from_settings(root, initial);
    let live_settings = LiveSettingsHandle::from_provider(provider);
    let provider = live_settings.owner().unwrap().provider_snapshot();
    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let document = dom.borrow().document();
    let runner = SettingsRunner::new_at(
        dom.clone(),
        document,
        composition_test_view as fn(&SettingsState) -> SettingsView,
        SettingsState::new(provider, live_settings, "base".into()),
    );
    SettingsPane {
        dom,
        runner,
        scroll: crate::ui::PaneScroll::new(),
        layout: crate::ui::RetainedLayout::new_at(None),
        mount: None,
    }
}

fn attr(dom: &ScriptedDom, node: genet_scripted_dom::NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(str::to_owned)
}

fn row(pane: &SettingsPane, setting_id: &str) -> genet_scripted_dom::NodeId {
    let dom = pane.dom_ref();
    dom.all_with_class(dom.document(), "setting-row")
        .into_iter()
        .find(|&node| attr(&dom, node, "data-setting").as_deref() == Some(setting_id))
        .expect("setting row in DOM")
}

fn text_field(pane: &SettingsPane, setting_id: &str) -> genet_scripted_dom::NodeId {
    let dom = pane.dom_ref();
    taproot::matching_estimated_fixture(
        &*dom,
        &taproot::Selector::role("textbox").containing("Theme ·"),
    )
    .into_iter()
    .find(|&node| {
        let mut ancestor = Some(node);
        while let Some(current) = ancestor {
            if attr(&dom, current, "data-setting").as_deref() == Some(setting_id) {
                return true;
            }
            ancestor = dom.parent(current);
        }
        false
    })
    .expect("text editor in setting row")
}

fn row_order(pane: &SettingsPane) -> Vec<String> {
    let dom = pane.dom_ref();
    dom.all_with_class(dom.document(), "setting-row")
        .into_iter()
        .filter_map(|node| attr(&dom, node, "data-setting"))
        .collect()
}

fn type_character(pane: &mut SettingsPane, setting_id: &str, value: &str) {
    let node = text_field(pane, setting_id);
    let _ = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
    let _: Vec<()> = pane
        .runner
        .dispatch_key(cambium::KeyEvent::new(cambium::Key::Character(
            value.into(),
        )));
}

fn key(pane: &mut SettingsPane, key: cambium::Key) {
    let _: Vec<()> = pane.runner.dispatch_key(cambium::KeyEvent::new(key));
}

fn replace_text(pane: &mut SettingsPane, setting_id: &str, value: &str) {
    let node = text_field(pane, setting_id);
    let _ = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
    key(pane, cambium::Key::Named(cambium::NamedKey::Home));
    let length = edit_value(pane, setting_id).chars().count();
    for _ in 0..length {
        key(pane, cambium::Key::Named(cambium::NamedKey::Delete));
    }
    for character in value.chars() {
        key(pane, cambium::Key::Character(character.to_string()));
    }
}

fn apply_exact(pane: &mut SettingsPane, setting_id: &str) {
    let node = {
        let dom = pane.dom_ref();
        dom.all_with_class(dom.document(), "setting-apply")
            .into_iter()
            .find(|&node| attr(&dom, node, "data-setting").as_deref() == Some(setting_id))
            .expect("exact Apply button")
    };
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
}

fn edit_value(pane: &SettingsPane, setting_id: &str) -> String {
    let dom = pane.dom_ref();
    attr(
        &dom,
        text_field(pane, setting_id),
        "data-cambium-text-value",
    )
    .unwrap()
}

#[test]
fn inserted_and_reordered_equal_text_rows_keep_draft_on_setting_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = pane(root.path());
    let key = composition::component_key(THEME_ID);
    let original_route = pane
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .unwrap();
    let original_row = row(&pane, THEME_ID);
    let original_value = edit_value(&pane, THEME_ID);
    type_character(&mut pane, THEME_ID, "x");
    assert_eq!(edit_value(&pane, THEME_ID), format!("{original_value}x"));

    pane.runner.update(|state| state.status = "inserted".into());
    assert_eq!(
        row_order(&pane).first().map(String::as_str),
        Some(SIBLING_ID)
    );
    assert_eq!(edit_value(&pane, THEME_ID), format!("{original_value}x"));
    assert_eq!(edit_value(&pane, SIBLING_ID), original_value);
    let after_insert = pane
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .unwrap();
    assert_eq!(after_insert.route.slot, original_route.route.slot);
    assert!(after_insert.visible);

    pane.runner
        .update(|state| state.status = "reordered".into());
    let order = row_order(&pane);
    assert!(
        order.iter().position(|id| id == THEME_ID).unwrap()
            < order.iter().position(|id| id == SIBLING_ID).unwrap()
    );
    assert_eq!(edit_value(&pane, THEME_ID), format!("{original_value}x"));
    assert_eq!(edit_value(&pane, SIBLING_ID), original_value);
    let after_reorder = pane
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .unwrap();
    assert_eq!(after_reorder.route.slot, original_route.route.slot);
}

#[test]
fn wrapper_replacement_hide_and_reveal_keep_original_draft_and_block_hidden_apply() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = pane(root.path());
    let key = composition::component_key(THEME_ID);
    let original_value = edit_value(&pane, THEME_ID);
    type_character(&mut pane, THEME_ID, "x");
    let original_row = row(&pane, THEME_ID);
    let original_apply = {
        let dom = pane.dom_ref();
        dom.all_with_class(dom.document(), "setting-apply")
            .into_iter()
            .find(|&node| attr(&dom, node, "data-setting").as_deref() == Some(THEME_ID))
            .unwrap()
    };

    pane.runner.set_focus(None);
    pane.runner
        .update(|state| state.status = "collapsed".into());
    assert!(
        !pane
            .runner
            .state()
            .composition_lifecycle
            .route(&key)
            .unwrap()
            .visible
    );
    let before_collapse_click = pane.settings().theme.as_ref().unwrap().theme_id.clone();
    let _: Vec<()> = pane
        .runner
        .dispatch_click(original_apply, cambium::PointerClick::at((0.0, 0.0)));
    assert_eq!(
        pane.settings().theme.as_ref().unwrap().theme_id,
        before_collapse_click
    );
    assert_eq!(pane.runner.state().status, "collapsed");

    pane.runner
        .update(|state| state.status = "wrapper-a".into());
    assert_eq!(edit_value(&pane, THEME_ID), format!("{original_value}x"));
    pane.runner
        .update(|state| state.status = "wrapper-b".into());
    assert_eq!(edit_value(&pane, THEME_ID), format!("{original_value}x"));
    assert_eq!(row(&pane, THEME_ID), original_row);

    let apply = {
        let dom = pane.dom_ref();
        dom.all_with_class(dom.document(), "setting-apply")
            .into_iter()
            .find(|&node| attr(&dom, node, "data-setting").as_deref() == Some(THEME_ID))
            .unwrap()
    };
    pane.runner.set_focus(None);
    pane.runner.update(|state| state.status = "hidden".into());
    let hidden = pane
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .unwrap();
    assert!(!hidden.visible);
    let before = pane.settings().theme.as_ref().unwrap().theme_id.clone();
    let _: Vec<()> = pane
        .runner
        .dispatch_click(apply, cambium::PointerClick::at((0.0, 0.0)));
    assert_eq!(pane.settings().theme.as_ref().unwrap().theme_id, before);
    assert_eq!(pane.runner.state().status, "hidden");

    pane.runner.update(|state| state.status = "revealed".into());
    assert_eq!(edit_value(&pane, THEME_ID), format!("{original_value}x"));
    assert_eq!(row(&pane, THEME_ID), original_row);
    assert!(
        pane.runner
            .state()
            .composition_lifecycle
            .route(&key)
            .unwrap()
            .visible
    );

    let target = match &pane.runner.state().provider.settings().theme {
        Some(_) => SettingsProjection::resolve(
            &pane.runner.state().provider,
            &SettingsRef(APPLICATION_REFERENCE.into()),
        )
        .unwrap()
        .specs
        .into_iter()
        .find(|spec| spec.id == THEME_ID)
        .and_then(|spec| match spec.control {
            mere_surface_api::settings::SettingControl::Choice { options } => options
                .into_iter()
                .map(|option| option.value)
                .find(|value| !value.is_empty() && value != &original_value),
            _ => None,
        })
        .expect("another registered theme option"),
        None => panic!("fixture starts with a selected theme"),
    };
    replace_text(&mut pane, THEME_ID, &target);
    apply_exact(&mut pane, THEME_ID);
    assert_eq!(pane.settings().theme.as_ref().unwrap().theme_id, target);
    assert_eq!(pane.runner.state().status, "Saved.");
}
