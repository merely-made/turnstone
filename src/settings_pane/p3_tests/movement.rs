// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use layout_dom_api::LayoutDomMut;

struct PaneHostState {
    mount: cambium::RetainedMount,
    attached: bool,
}

type PaneHostView = Box<dyn AnyView<PaneHostState, (), GenetCtx, GenetElement>>;
type PaneHostRunner =
    GenetAppRunner<PaneHostState, fn(&PaneHostState) -> PaneHostView, PaneHostView, ()>;

fn pane_host_view(state: &PaneHostState) -> PaneHostView {
    if state.attached {
        Box::new(state.mount.view::<PaneHostState, ()>())
    } else {
        Box::new(el::<_, PaneHostState, ()>("div", "Settings pane removed"))
    }
}
fn action_named(pane: &mut SettingsPane, action: &str, label: &str, component: &str) {
    let node = {
        let dom = pane.dom_ref();
        let found = descendants(&dom, dom.document())
            .into_iter()
            .filter(|&node| {
                attr(&dom, node, "data-action").as_deref() == Some(action)
                    && attr(&dom, node, "aria-label").as_deref() == Some(label)
                    && attr(&dom, node, "data-component").as_deref() == Some(component)
            })
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "one {action} control labeled {label}");
        found[0]
    };
    click_node(pane, node);
}

fn make_group(pane: &mut SettingsPane, key: &ComponentKey) {
    click_action(pane, "toggle-editor", "");
    click_action(pane, "add-group", "");
    action_named(pane, "move-to", "Move to group Group 1", &key.0);
    assert!(pane.runner.state().arrangement.accepted().placement.iter().any(
        |node| matches!(node, cambium::composition::PlacementNode::Group { key, children }
            if key.0 == "Group 1" && children.iter().any(|child| matches!(child,
                cambium::composition::PlacementNode::Component { key } if key.0 == INSTANCE_PREFIX)))
    ));
}

fn toggle_group(pane: &mut SettingsPane) {
    let node = {
        let dom = pane.dom_ref();
        let found = descendants(&dom, dom.document())
            .into_iter()
            .filter(|&node| {
                attr(&dom, node, "data-action").as_deref() == Some("toggle-container")
                    && attr(&dom, node, "data-container-kind").as_deref() == Some("group")
                    && attr(&dom, node, "data-container-key").as_deref() == Some("Group 1")
            })
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "one toggle for the target group");
        found[0]
    };
    click_node(pane, node);
}

#[test]
fn real_row_move_collapse_and_reveal_preserve_control_session_and_external_conflict() {
    let root = tempfile::tempdir().unwrap();
    let (app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = connected_pane(&app, pane_id, root.path(), &live);
    let mut external = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&pane);

    toggle_draft_off(&mut pane, &key.0);
    let row = instance_root(&pane, &key.0);
    let toggle = control(&pane, "toggle", &key.0);
    let session = pane.runner.state().rows[&key].session.clone();
    make_group(&mut pane, &key);
    assert_eq!(instance_root(&pane, &key.0), row);
    assert_eq!(control(&pane, "toggle", &key.0), toggle);
    assert_eq!(pane.runner.state().rows[&key].session, session);
    assert_eq!(session.snapshot().draft_value, SettingValue::Boolean(false));

    toggle_group(&mut pane);
    let route = pane
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .expect("retained hidden route");
    assert!(!route.visible);
    assert!(session.snapshot().dirty);

    let external_key = shellbar_row_key(&external);
    toggle_draft_off(&mut external, &external_key.0);
    apply(&mut external, &external_key.0);
    pane.sync(640.0, 780.0);
    assert!(
        !pane
            .runner
            .state()
            .composition_lifecycle
            .route(&key)
            .unwrap()
            .visible
    );
    assert_eq!(session.snapshot().phase, SettingRowPhase::Dirty);
    assert_eq!(session.snapshot().draft_value, SettingValue::Boolean(false));
    assert_eq!(
        session.snapshot().observed_spec.value,
        SettingValue::Boolean(false)
    );

    toggle_group(&mut pane);
    assert_eq!(
        pane.runner
            .state()
            .composition_lifecycle
            .route(&key)
            .unwrap()
            .visible,
        true
    );
    assert_eq!(instance_root(&pane, &key.0), row);
    assert_eq!(control(&pane, "toggle", &key.0), toggle);
    assert_eq!(pane.runner.state().rows[&key].session, session);
    assert!(session.snapshot().dirty);
    apply(&mut pane, &key.0);
    assert_eq!(session.snapshot().phase, SettingRowPhase::Conflict);
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
}

#[test]
fn save_ack_is_request_scoped_and_does_not_mark_later_arrangement_edit_saved() {
    let root = tempfile::tempdir().unwrap();
    let (app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&pane);
    make_group(&mut pane, &key);

    let before = pane.current_record().unwrap();
    click_action(&mut pane, "save", "");
    let request = pane.runner.state().editor.save_request.unwrap();
    assert_eq!(
        pane.runner.state().saving_record,
        Some((request, before.0.clone(), before.1.clone()))
    );
    let notice = pane.runner.state().editor.notice.clone();

    pane.arrangement_saved(uuid::Uuid::new_v4(), Err("unrelated response".into()));
    assert_eq!(pane.runner.state().editor.save_request, Some(request));
    assert_eq!(pane.runner.state().editor.notice, notice);

    click_action(&mut pane, "add-group", "");
    let after_edit = pane.current_record().unwrap();
    assert_ne!(after_edit, before);
    pane.arrangement_saved(request, Ok(()));
    assert_eq!(pane.runner.state().saved_record, Some(before));
    assert!(pane.runner.state().editor.unsaved);
}

#[test]
fn reload_discard_preflights_later_pending_row_before_resetting_earlier_draft() {
    let root = tempfile::tempdir().unwrap();
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = SettingsPane::with_live_settings(root.path().to_path_buf(), Some(live));
    let original = shellbar_row_key(&pane);
    let duplicate = add_duplicate_shellbar_row(&mut pane);
    let matching = pane
        .runner
        .state()
        .rows
        .iter()
        .filter(|(_, row)| row.setting_id == VISIBLE_ID)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    assert_eq!(matching, vec![duplicate.clone(), original.clone()]);

    toggle_draft_off(&mut pane, &duplicate.0);
    toggle_draft_off(&mut pane, &original.0);
    let earlier = pane.runner.state().rows[&duplicate].session.clone();
    let later = pane.runner.state().rows[&original].session.clone();
    let generation = later.snapshot().edit_generation;
    assert!(later.begin_apply(generation));
    let revision = pane.runner.state().arrangement.revision();

    click_action(&mut pane, "reload-saved", "");
    click_labelled_action(&mut pane, "Discard edits and reload");

    assert!(earlier.snapshot().dirty);
    assert_eq!(earlier.snapshot().draft_value, SettingValue::Boolean(false));
    assert_eq!(later.snapshot().pending_generation, Some(generation));
    assert!(later.snapshot().dirty);
    assert_eq!(pane.runner.state().arrangement.revision(), revision);
    assert!(pane.runner.state().rows.contains_key(&duplicate));
    assert!(
        pane.runner
            .state()
            .editor
            .notice
            .contains("No edits were applied or discarded")
    );
}

#[test]
fn destination_first_whole_pane_move_preserves_live_row_and_applies_once() {
    let root = tempfile::tempdir().unwrap();
    let provider = ApplicationSettingsProvider::load(root.path()).unwrap();
    let live = LiveSettingsHandle::from_provider(provider);
    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let mount = cambium::RetainedMount::new(dom.clone());
    let mut pane = SettingsPane::new_in(Some(mount.clone()), root.path().to_path_buf(), Some(live));
    let key = shellbar_row_key(&pane);

    let mut host_a = PaneHostRunner::new(
        dom.clone(),
        pane_host_view as fn(&PaneHostState) -> PaneHostView,
        PaneHostState {
            mount: mount.clone(),
            attached: true,
        },
    );
    let row = instance_root(&pane, &key.0);
    let toggle = control(&pane, "toggle", &key.0);
    toggle_draft_off(&mut pane, &key.0);
    let session = pane.runner.state().rows[&key].session.clone();
    let before_guard = pane.runner.state().rows[&key]
        .binding
        .as_ref()
        .unwrap()
        .guard()
        .clone();

    let mut host_b = PaneHostRunner::new(
        dom.clone(),
        pane_host_view as fn(&PaneHostState) -> PaneHostView,
        PaneHostState {
            mount: mount.clone(),
            attached: true,
        },
    );
    let destination = host_b.root();
    assert_eq!(dom.borrow().parent(mount.node()), Some(destination));
    host_a.update(|state| state.attached = false);
    dom.borrow_mut().collect([]);
    pane.sync(640.0, 780.0);

    assert_eq!(dom.borrow().parent(mount.node()), Some(destination));
    assert_eq!(instance_root(&pane, &key.0), row);
    assert_eq!(control(&pane, "toggle", &key.0), toggle);
    assert_eq!(pane.runner.state().rows[&key].session, session);
    assert!(session.snapshot().dirty);
    assert_eq!(session.snapshot().draft_value, SettingValue::Boolean(false));

    apply(&mut pane, &key.0);
    assert_eq!(session.snapshot().phase, SettingRowPhase::Clean);
    let after_guard = pane.runner.state().rows[&key]
        .binding
        .as_ref()
        .unwrap()
        .guard()
        .clone();
    assert_eq!(after_guard.revision, before_guard.revision + 1);
    assert!(
        ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
    drop(host_b);
    drop(host_a);
    drop(pane);
}

#[test]
fn failed_save_echo_is_adopted_and_arrangement_remains_retryable() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&pane);
    make_group(&mut pane, &key);
    let captured = pane.current_record().unwrap();

    click_action(&mut pane, "save", "");
    let action = live.take_actions().into_iter().next().unwrap();
    let request = match &action {
        crate::action::Action::SetSettingsPaneRecord((_, _, request, _, _)) => *request,
        other => panic!("unexpected queued action: {other:?}"),
    };
    let effects = app.update(action);
    assert!(
        effects.contains(&crate::action::Effect::PersistSettingsPaneRecord {
            pane: pane_id,
            request_id: request,
        })
    );

    pane.arrangement_saved(request, Err("injected frame write failure".into()));
    assert!(pane.runner.state().editor.unsaved);
    pane.sync_for_pane(&app, pane_id, 640.0, 780.0);

    assert_eq!(pane.runner.state().pending_record, None);
    assert_eq!(pane.runner.state().loaded_record, Some(captured.clone()));
    assert!(pane.runner.state().saving_record.is_none());
    assert!(pane.runner.state().editor.unsaved);
    assert_eq!(pane.current_record().unwrap(), captured);
    click_action(&mut pane, "save", "");
    let retry = live.take_actions().into_iter().next().unwrap();
    assert!(
        matches!(retry, crate::action::Action::SetSettingsPaneRecord((retry_pane, admission, retry_request, _, _))
        if retry_pane == pane_id
            && admission == app.settings_pane_record_admission(pane_id).unwrap()
            && retry_request != request)
    );
    assert!(pane.runner.state().editor.save_request.is_some());
}

#[test]
fn successful_save_ack_then_sync_keeps_newer_local_group_edit_unsaved() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, pane_id) = app_with_settings(root.path());
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = connected_pane(&app, pane_id, root.path(), &live);
    let key = shellbar_row_key(&pane);
    make_group(&mut pane, &key);

    click_action(&mut pane, "save", "");
    let action = live.take_actions().into_iter().next().unwrap();
    let request = match &action {
        crate::action::Action::SetSettingsPaneRecord((_, _, request, _, _)) => *request,
        other => panic!("unexpected queued action: {other:?}"),
    };
    let effects = app.update(action);
    assert!(
        effects.contains(&crate::action::Effect::PersistSettingsPaneRecord {
            pane: pane_id,
            request_id: request,
        })
    );
    pane.arrangement_saved(request, Ok(()));
    let saved = pane.runner.state().saved_record.clone().unwrap();

    click_action(&mut pane, "add-group", "");
    let newer = pane.current_record().unwrap();
    assert_ne!(newer, saved);
    pane.sync_for_pane(&app, pane_id, 640.0, 780.0);

    assert_eq!(pane.runner.state().loaded_record, Some(saved.clone()));
    assert_eq!(pane.runner.state().saved_record, Some(saved));
    assert_eq!(pane.runner.state().pending_record, None);
    assert_eq!(pane.current_record().unwrap(), newer);
    assert!(pane.runner.state().editor.unsaved);
}

#[test]
fn missing_saved_binding_can_be_rebound_through_the_real_editor() {
    let root = tempfile::tempdir().unwrap();
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut pane = SettingsPane::with_live_settings(root.path().to_path_buf(), Some(live));
    let key = shellbar_row_key(&pane);
    let (mut config, view) = pane.current_record().unwrap();
    let descriptors = config.payload["composition"]["components"]
        .as_array_mut()
        .expect("encoded component list");
    let descriptor = descriptors
        .iter_mut()
        .find(|descriptor| descriptor["key"] == key.0)
        .expect("shellbar descriptor");
    descriptor["binding"]["reference"] = serde_json::json!("missing.settings.source");
    descriptor["binding"]["setting_id"] = serde_json::json!("missing.shellbar.visible");

    pane.adopt_record(config, view, true).unwrap();
    assert!(!pane.runner.state().rows.contains_key(&key));
    assert!(
        pane.runner
            .state()
            .arrangement
            .accepted()
            .components
            .iter()
            .any(|descriptor| descriptor.key == key)
    );

    let target_label = pane
        .runner
        .state()
        .snapshots
        .get(crate::settings_provider::APPLICATION_REFERENCE)
        .unwrap()
        .specs
        .iter()
        .find(|entry| entry.spec.id == VISIBLE_ID)
        .unwrap()
        .spec
        .label
        .clone();
    click_action(&mut pane, "toggle-editor", "");
    action_named(
        &mut pane,
        "rebind",
        &format!("Rebind: {target_label}"),
        &key.0,
    );

    let row = pane.runner.state().rows.get(&key).expect("rebound row");
    assert_eq!(row.setting_id, VISIBLE_ID);
    assert_eq!(row.session.snapshot().phase, SettingRowPhase::Clean);
    assert_eq!(
        row.session.snapshot().draft_value,
        SettingValue::Boolean(true)
    );
}

#[test]
fn adjusting_a_reviewed_draft_still_requires_and_admits_the_explicit_retry() {
    let root = tempfile::tempdir().unwrap();
    let live =
        LiveSettingsHandle::from_provider(ApplicationSettingsProvider::load(root.path()).unwrap());
    let mut first = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
    let mut second = SettingsPane::with_live_settings(root.path().into(), Some(live));
    let key = shellbar_row_key(&first);
    toggle_draft_off(&mut first, &key.0);
    toggle_draft_off(&mut second, &key.0);
    apply(&mut second, &key.0);
    first.sync(640.0, 780.0);
    apply(&mut first, &key.0);
    let review = control(&first, "setting-review-mine", &key.0);
    click_node(&mut first, review);
    let toggle = control(&first, "toggle", &key.0);
    click_node(&mut first, toggle);
    assert_eq!(
        first.runner.state().rows[&key].session.snapshot().phase,
        SettingRowPhase::ReviewingMine
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
    assert!(
        !ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .settings()
            .shellbar_hidden
    );
}
