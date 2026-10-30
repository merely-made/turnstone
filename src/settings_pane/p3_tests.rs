// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! End-to-end Settings consumer regressions through real retained DOM events.

use super::*;
use crate::panes::{PaneId, PaneViewState};
use cambium::{SettingRowPhase, composition::ComponentKey};
use genet_scripted_dom::NodeId;
use layout_dom_api::{LayoutDom, LocalName, Namespace};

const VISIBLE_ID: &str = "chrome.shellbar.visible";
const INSTANCE_PREFIX: &str = "turnstone.setting:chrome.shellbar.visible";

fn app_with_settings(root: &std::path::Path) -> (crate::app::App, PaneId) {
    let mut app = crate::app::App::test_stub_at(root.to_path_buf());
    app.update(crate::action::Action::SummonPane(
        crate::panes::PaneKindId::new(crate::panes::kind::SETTINGS),
    ));
    let pane = app
        .frisket
        .iter_leaves()
        .find_map(|(id, content, _)| {
            matches!(content, crate::panes::PaneContent::Registered(kind)
                if kind.as_str() == crate::panes::kind::SETTINGS)
            .then_some(id)
        })
        .expect("summoned Settings pane");
    (app, pane)
}

fn connected_pane(
    app: &crate::app::App,
    pane_id: PaneId,
    root: &std::path::Path,
    live: &LiveSettingsHandle,
) -> SettingsPane {
    let mut pane = SettingsPane::with_live_settings(root.to_path_buf(), Some(live.clone()));
    pane.sync_for_pane(app, pane_id, 640.0, 780.0);
    pane
}

fn attr(dom: &ScriptedDom, node: NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(str::to_owned)
}

fn is_below(dom: &ScriptedDom, node: NodeId, ancestor: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate == ancestor {
            return true;
        }
        current = dom.parent(candidate);
    }
    false
}

fn descendants(dom: &ScriptedDom, root: NodeId) -> Vec<NodeId> {
    let mut result = vec![root];
    let mut index = 0;
    while index < result.len() {
        result.extend(dom.dom_children(result[index]).collect::<Vec<_>>());
        index += 1;
    }
    result
}

fn instance_root(pane: &SettingsPane, key: &str) -> NodeId {
    let dom = pane.dom_ref();
    dom.all_with_class(dom.document(), "settings-owned-row")
        .into_iter()
        .find(|&node| attr(&dom, node, "data-setting-instance").as_deref() == Some(key))
        .expect("retained setting instance")
}

fn control(pane: &SettingsPane, class: &str, key: &str) -> NodeId {
    let dom = pane.dom_ref();
    let root = instance_root(pane, key);
    dom.all_with_class(dom.document(), class)
        .into_iter()
        .find(|&node| is_below(&dom, node, root))
        .expect("control under exact component instance")
}

fn click_node(pane: &mut SettingsPane, node: NodeId) {
    dispatch_node(pane, node);
    pane.flush_editor_commands();
}

fn dispatch_node(pane: &mut SettingsPane, node: NodeId) {
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((1.0, 1.0)));
}

fn click_action(pane: &mut SettingsPane, action: &str, component: &str) {
    let node = {
        let dom = pane.dom_ref();
        let matches = descendants(&dom, dom.document())
            .into_iter()
            .filter(|&node| {
                attr(&dom, node, "data-action").as_deref() == Some(action)
                    && attr(&dom, node, "data-component").as_deref() == Some(component)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "one exact {action} control for {component}"
        );
        matches[0]
    };
    click_node(pane, node);
}

fn click_labelled_action(pane: &mut SettingsPane, label: &str) {
    let node = {
        let dom = pane.dom_ref();
        let matches = descendants(&dom, dom.document())
            .into_iter()
            .filter(|&node| attr(&dom, node, "aria-label").as_deref() == Some(label))
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "one {label} control");
        matches[0]
    };
    click_node(pane, node);
}

fn toggle_draft_off(pane: &mut SettingsPane, key: &str) {
    let node = control(pane, "toggle", key);
    click_node(pane, node);
    let dom = pane.dom_ref();
    assert_eq!(attr(&dom, node, "aria-checked").as_deref(), Some("false"));
    assert_eq!(
        pane.runner.state().rows[&ComponentKey(key.into())]
            .session
            .snapshot()
            .draft_value,
        SettingValue::Boolean(false)
    );
}

fn apply(pane: &mut SettingsPane, key: &str) {
    let node = control(pane, "setting-apply", key);
    click_node(pane, node);
}

fn shellbar_row_key(pane: &SettingsPane) -> ComponentKey {
    pane.runner
        .state()
        .rows
        .keys()
        .find(|key| key.0 == INSTANCE_PREFIX)
        .expect("default shellbar row")
        .clone()
}

fn add_duplicate_shellbar_row(pane: &mut SettingsPane) -> ComponentKey {
    click_action(pane, "toggle-editor", "");
    let original = shellbar_row_key(pane);
    click_action(pane, "duplicate", &original.0);
    let matches = pane
        .runner
        .state()
        .rows
        .iter()
        .filter(|(_, row)| row.setting_id == VISIBLE_ID)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        2,
        "duplicate creates another retained row instance"
    );
    matches.into_iter().find(|key| key != &original).unwrap()
}

#[test]
fn real_rows_keep_same_setting_draft_across_external_write_until_apply_conflicts() {
    let root = tempfile::tempdir().unwrap();
    let (app, pane_id) = app_with_settings(root.path());
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let mut first = connected_pane(&app, pane_id, root.path(), &live);
    let mut second = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&first);

    toggle_draft_off(&mut first, &key.0);
    toggle_draft_off(&mut second, &key.0);
    apply(&mut second, &key.0);
    assert_eq!(
        second.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::Clean
    );
    assert!(second.runner.state().provider.settings().shellbar_hidden);

    first.sync(640.0, 780.0);
    let draft = first.runner.state().rows[&key].session.snapshot();
    assert!(draft.dirty, "refresh must retain the original row's draft");
    assert_eq!(draft.draft_value, SettingValue::Boolean(false));
    assert_eq!(draft.observed_spec.value, SettingValue::Boolean(false));
    apply(&mut first, &key.0);
    assert_eq!(
        first.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::Conflict
    );
    assert!(
        first
            .runner
            .state()
            .status
            .contains("changed since this edit began")
    );
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden,
        "the stale row cannot overwrite the external commit"
    );
}

#[test]
fn conflict_use_current_discards_only_after_explicit_row_action() {
    let root = tempfile::tempdir().unwrap();
    let (app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut first = connected_pane(&app, pane_id, root.path(), &live);
    let mut second = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&first);
    toggle_draft_off(&mut first, &key.0);
    toggle_draft_off(&mut second, &key.0);
    apply(&mut second, &key.0);
    first.sync(640.0, 780.0);
    apply(&mut first, &key.0);
    assert_eq!(
        first.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::Conflict
    );

    let use_current = control(&first, "setting-use-current", &key.0);
    click_node(&mut first, use_current);
    let state = first.runner.state().rows[&key].session.snapshot();
    assert_eq!(state.phase, SettingRowPhase::Clean);
    assert_eq!(state.draft_value, SettingValue::Boolean(false));
    assert_eq!(
        first.runner.state().status,
        "Discarded edit and adopted the current value."
    );
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
}

#[test]
fn conflict_review_does_not_retry_until_apply_my_edit_is_dispatched() {
    let root = tempfile::tempdir().unwrap();
    let (app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut first = connected_pane(&app, pane_id, root.path(), &live);
    let mut second = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&first);
    toggle_draft_off(&mut first, &key.0);
    toggle_draft_off(&mut second, &key.0);
    apply(&mut second, &key.0);
    first.sync(640.0, 780.0);
    apply(&mut first, &key.0);
    assert_eq!(
        first.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::Conflict
    );

    let review = control(&first, "setting-review-mine", &key.0);
    click_node(&mut first, review);
    assert_eq!(
        first.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::ReviewingMine
    );
    assert!(
        first
            .runner
            .state()
            .status
            .contains("Review the current value")
    );
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );

    let retry = control(&first, "setting-retry-mine", &key.0);
    click_node(&mut first, retry);
    assert_eq!(
        first.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::Clean
    );
    assert_eq!(first.runner.state().status, "Saved.");
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
}

#[test]
fn duplicate_rows_have_independent_drafts_and_a_stale_duplicate_conflicts() {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let mut pane = SettingsPane::with_live_settings(root.path().to_path_buf(), Some(live));
    let original = shellbar_row_key(&pane);
    let duplicate = add_duplicate_shellbar_row(&mut pane);

    toggle_draft_off(&mut pane, &original.0);
    assert_eq!(
        pane.runner.state().rows[&duplicate]
            .session
            .snapshot()
            .draft_value,
        SettingValue::Boolean(true)
    );
    toggle_draft_off(&mut pane, &duplicate.0);
    apply(&mut pane, &original.0);
    assert_eq!(
        pane.runner.state().rows[&original].session.snapshot().phase,
        SettingRowPhase::Clean
    );
    assert_eq!(
        pane.runner.state().rows[&duplicate]
            .session
            .snapshot()
            .phase,
        SettingRowPhase::Dirty
    );

    apply(&mut pane, &duplicate.0);
    assert_eq!(
        pane.runner.state().rows[&duplicate]
            .session
            .snapshot()
            .phase,
        SettingRowPhase::Conflict
    );
    assert!(
        pane.runner
            .state()
            .status
            .contains("changed since this edit began")
    );
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
}

#[test]
fn dirty_rebind_is_disabled_and_a_queued_clean_rebind_is_rechecked() {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let mut pane = SettingsPane::with_live_settings(root.path().to_path_buf(), Some(live));
    let key = shellbar_row_key(&pane);
    click_action(&mut pane, "toggle-editor", "");

    let clean_rebind = {
        let dom = pane.dom_ref();
        let matches = descendants(&dom, dom.document())
            .into_iter()
            .filter(|&node| {
                attr(&dom, node, "data-action").as_deref() == Some("rebind")
                    && attr(&dom, node, "data-component").as_deref() == Some(&key.0)
                    && attr(&dom, node, "aria-label").as_deref() == Some("Rebind: UI zoom")
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        matches[0]
    };
    dispatch_node(&mut pane, clean_rebind); // Queue an intent while its render says clean.
    toggle_draft_off(&mut pane, &key.0);
    pane.flush_editor_commands();

    let descriptor = pane
        .runner
        .state()
        .arrangement
        .accepted()
        .components
        .iter()
        .find(|descriptor| descriptor.key == key)
        .unwrap();
    assert_eq!(descriptor.binding["setting_id"], VISIBLE_ID);
    assert!(
        pane.runner
            .state()
            .editor
            .notice
            .contains("Apply or discard")
    );

    let dirty_rebind = {
        let dom = pane.dom_ref();
        descendants(&dom, dom.document())
            .into_iter()
            .find(|&node| {
                attr(&dom, node, "data-action").as_deref() == Some("rebind")
                    && attr(&dom, node, "data-component").as_deref() == Some(&key.0)
                    && attr(&dom, node, "aria-label").as_deref() == Some("Rebind: UI zoom")
            })
            .expect("dirty row still offers the blocked target")
    };
    assert!(attr(&pane.dom_ref(), dirty_rebind, "disabled").is_some());
    assert!(pane.runner.state().rows[&key].session.snapshot().dirty);
}

#[test]
fn delete_cancel_keeps_dirty_row_then_discard_delete_undo_makes_a_new_clean_instance() {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let mut pane = SettingsPane::with_live_settings(root.path().to_path_buf(), Some(live));
    let key = shellbar_row_key(&pane);
    click_action(&mut pane, "toggle-editor", "");
    toggle_draft_off(&mut pane, &key.0);
    let original_node = instance_root(&pane, &key.0);

    click_action(&mut pane, "delete", &key.0);
    assert!(pane.runner.state().editor.confirmation.is_some());
    click_labelled_action(&mut pane, "Cancel");
    assert!(pane.runner.state().editor.confirmation.is_none());
    assert_eq!(instance_root(&pane, &key.0), original_node);
    assert!(pane.runner.state().rows[&key].session.snapshot().dirty);

    click_action(&mut pane, "delete", &key.0);
    click_labelled_action(&mut pane, "Discard edit and delete");
    assert!(!pane.runner.state().rows.contains_key(&key));
    assert!(
        !pane
            .dom_ref()
            .all_with_class(pane.dom_ref().document(), "settings-owned-row")
            .iter()
            .any(
                |&node| attr(&pane.dom_ref(), node, "data-setting-instance").as_deref()
                    == Some(&key.0)
            )
    );

    click_action(&mut pane, "undo", "");
    assert!(pane.runner.state().rows.contains_key(&key));
    let recreated = instance_root(&pane, &key.0);
    assert_ne!(
        recreated, original_node,
        "Undo creates a fresh retained component node"
    );
    let state = pane.runner.state().rows[&key].session.snapshot();
    assert!(!state.dirty);
    assert_eq!(state.draft_value, SettingValue::Boolean(true));
}

#[test]
fn saving_arrangement_persists_structure_without_serializing_or_clearing_row_drafts() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = connected_pane(&app, pane_id, root.path(), &live);
    let original = shellbar_row_key(&pane);
    let duplicate = add_duplicate_shellbar_row(&mut pane);
    toggle_draft_off(&mut pane, &original.0);

    click_action(&mut pane, "save", "");
    let state_before = pane.runner.state().rows[&original].session.snapshot();
    assert!(state_before.dirty);
    assert_eq!(state_before.draft_value, SettingValue::Boolean(false));
    let actions = live.take_actions();
    assert_eq!(actions.len(), 1);
    let (request_id, config, view) = match &actions[0] {
        crate::action::Action::SetSettingsPaneRecord((
            action_pane,
            admission,
            request,
            config,
            view,
        )) => {
            assert_eq!(*action_pane, pane_id);
            assert_eq!(
                *admission,
                app.settings_pane_record_admission(pane_id).unwrap()
            );
            (*request, config.clone(), view.clone())
        },
        other => panic!("unexpected queued action: {other:?}"),
    };
    let encoded = serde_json::to_string(&config).unwrap();
    for runtime_field in ["draft", "dirty", "pending_generation", "baseline_spec"] {
        assert!(
            !encoded.contains(runtime_field),
            "runtime field leaked: {runtime_field}"
        );
    }
    assert!(
        config.payload["composition"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|descriptor| descriptor["key"] == duplicate.0)
    );
    assert_eq!(view.schema.as_str(), "turnstone.settings.view");

    let effects = app.update(actions.into_iter().next().unwrap());
    assert!(
        effects.contains(&crate::action::Effect::PersistSettingsPaneRecord {
            pane: pane_id,
            request_id,
        })
    );
    let session_dir = app.session_dir();
    crate::panes::store::save_frisket_layout(&session_dir, &app.frisket).unwrap();
    pane.arrangement_saved(request_id, Ok(()));
    assert!(pane.runner.state().editor.notice.contains("saved"));
    assert!(pane.runner.state().rows[&original].session.snapshot().dirty);
    assert!(
        !ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );

    let restored = crate::panes::store::load_frisket_layout(&session_dir)
        .unwrap()
        .unwrap();
    let frame = restored.outer_workspace().unwrap();
    let spec: crate::panes::PaneSpec =
        serde_json::from_value(frame.descriptors[&pane_id.0].clone()).unwrap();
    let view: PaneViewState =
        serde_json::from_value(frame.view_states[&pane_id.0].clone()).unwrap();
    let restarted_live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut restarted =
        SettingsPane::with_live_settings(root.path().to_path_buf(), Some(restarted_live));
    restarted.adopt_record(spec.config, view, true).unwrap();
    let instances = restarted
        .runner
        .state()
        .rows
        .iter()
        .filter(|(_, row)| row.setting_id == VISIBLE_ID)
        .collect::<Vec<_>>();
    assert_eq!(
        instances.len(),
        2,
        "the authored duplicate survives restart"
    );
    assert!(
        instances
            .iter()
            .all(|(_, row)| !row.session.snapshot().dirty)
    );
    assert_eq!(
        restarted.runner.state().rows[&original]
            .session
            .snapshot()
            .draft_value,
        SettingValue::Boolean(true),
        "the unsaved per-row draft is not persisted"
    );
}

#[test]
fn invalid_restored_record_keeps_live_row_dom_and_draft_untouched() {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let mut pane = SettingsPane::with_live_settings(root.path().to_path_buf(), Some(live));
    let key = shellbar_row_key(&pane);
    toggle_draft_off(&mut pane, &key.0);
    let old_node = instance_root(&pane, &key.0);
    let (mut config, view) = pane.current_record().unwrap();
    config.schema = crate::panes::PaneConfigSchemaId::new("invalid.settings.schema");

    assert!(pane.adopt_record(config, view, true).is_err());
    assert_eq!(instance_root(&pane, &key.0), old_node);
    let snapshot = pane.runner.state().rows[&key].session.snapshot();
    assert!(snapshot.dirty);
    assert_eq!(snapshot.draft_value, SettingValue::Boolean(false));
}

#[path = "p3_tests/movement.rs"]
mod movement;
