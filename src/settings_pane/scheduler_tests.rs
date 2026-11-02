// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::super::*;
use cambium::composition_scheduler::InputVector;
use layout_dom_api::{LayoutDom, LocalName, Namespace};

fn fixture() -> (
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

fn attribute(dom: &ScriptedDom, node: genet_scripted_dom::NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(str::to_owned)
}

fn row_control(pane: &SettingsPane, setting_id: &str, class: &str) -> genet_scripted_dom::NodeId {
    let dom = pane.dom_ref();
    let node = dom
        .all_with_class(dom.document(), class)
        .into_iter()
        .find(|node| {
            let mut parent = Some(*node);
            while let Some(candidate) = parent {
                if attribute(&dom, candidate, "data-setting").as_deref() == Some(setting_id) {
                    return true;
                }
                parent = dom.parent(candidate);
            }
            false
        })
        .unwrap_or_else(|| panic!("{class} control for {setting_id}"));
    node
}

fn click_row(pane: &mut SettingsPane, setting_id: &str, class: &str) {
    let node = row_control(pane, setting_id, class);
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
}

fn vector(pane: &SettingsPane, setting_id: &str) -> InputVector {
    pane.runner
        .state()
        .published_inputs
        .borrow()
        .vector(&composition::component_key(setting_id))
        .cloned()
        .unwrap_or_else(|| panic!("input vector for {setting_id}"))
}

fn revision(vector: &InputVector, domain: &str) -> u64 {
    vector
        .dependencies()
        .iter()
        .find(|dependency| dependency.domain() == domain)
        .unwrap_or_else(|| panic!("dependency {domain}"))
        .revision()
}

fn local_generation(vector: &InputVector) -> u64 {
    vector
        .local()
        .iter()
        .find(|stamp| stamp.name() == "row-session")
        .expect("session input")
        .generation()
}

#[test]
fn panels_publish_domain_dependencies_and_keep_user_intents_fifo() {
    let (_root, live, mut first, mut second) = fixture();
    let shell_before = vector(&first, "chrome.shellbar.visible");
    let second_shell_before = vector(&second, "chrome.shellbar.visible");
    let behavior_before = vector(&first, "behaviors.cascade_budget");

    // A local edit changes only that instance's session generation. It does
    // not poll the provider or change its captured owner revision.
    click_row(&mut first, "chrome.shellbar.visible", "toggle");
    let dirty = vector(&first, "chrome.shellbar.visible");
    assert!(local_generation(&dirty) > local_generation(&shell_before));
    assert_eq!(
        local_generation(&vector(&second, "chrome.shellbar.visible")),
        local_generation(&second_shell_before),
        "same source setting in another pane has an independent local input"
    );
    assert_eq!(
        revision(&dirty, "shell-chrome"),
        revision(&shell_before, "shell-chrome")
    );

    // The second pane commits an unrelated settings domain through the same
    // shared owner. Refreshing the first pane advances that dependency only.
    let behavior_revision_before_write =
        revision(&vector(&second, "behaviors.cascade_budget"), "behaviors");
    second.runner.update(|state| {
        apply_value(state, "behaviors.cascade_budget", SettingValue::Number(9.0));
    });
    assert_eq!(
        revision(&vector(&second, "behaviors.cascade_budget"), "behaviors"),
        behavior_revision_before_write + 1,
        "one user Apply causes one domain revision"
    );
    first.sync(640.0, 780.0);
    let shell_after_unrelated = vector(&first, "chrome.shellbar.visible");
    let behavior_after = vector(&first, "behaviors.cascade_budget");
    assert_eq!(
        revision(&shell_after_unrelated, "shell-chrome"),
        revision(&dirty, "shell-chrome")
    );
    assert_eq!(
        revision(&behavior_after, "behaviors"),
        revision(&behavior_before, "behaviors") + 1
    );

    // The first pane's draft survives that unrelated revision. The second
    // pane then commits the same domain, so the first row retains the draft
    // while its observed revision advances for guarded Apply to adjudicate.
    let shell_revision_before_write =
        revision(&vector(&second, "chrome.shellbar.visible"), "shell-chrome");
    click_row(&mut second, "chrome.shellbar.visible", "toggle");
    click_row(&mut second, "chrome.shellbar.visible", "setting-apply");
    assert_eq!(
        revision(&vector(&second, "chrome.shellbar.visible"), "shell-chrome"),
        shell_revision_before_write + 1,
        "one actual row Apply causes one domain revision"
    );
    first.sync(640.0, 780.0);
    let after_same_domain = vector(&first, "chrome.shellbar.visible");
    assert_eq!(
        revision(&after_same_domain, "shell-chrome"),
        revision(&shell_after_unrelated, "shell-chrome") + 1
    );
    assert_eq!(
        first
            .runner
            .state()
            .rows
            .get(&composition::component_key("chrome.shellbar.visible"))
            .unwrap()
            .session
            .snapshot()
            .draft_value,
        SettingValue::Boolean(false)
    );

    // User intents remain on their separate FIFO queue and are never merged
    // with the coalescing invalidation table.
    let action = crate::action::Action::OpenThemeWorkshop;
    live.request_action(action.clone());
    live.request_action(action.clone());
    let queued = live.take_actions();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued, vec![action.clone(), action]);
    assert_eq!(*first.runner.state().input_queue_error.borrow(), None);
}

#[test]
fn hidden_row_keeps_its_pending_revision_until_reveal_evaluates_it() {
    use arrangement::ArrangementOperation;
    use cambium::composition::{ContainerKey, ContainerKind};

    let (_root, _live, mut first, mut second) = fixture();
    let key = composition::component_key("chrome.shellbar.visible");
    let root = ContainerKey {
        kind: ContainerKind::Root,
        key: String::new(),
    };
    first.runner.update(|state| {
        let prepared = state
            .arrangement
            .preview(ArrangementOperation::Hide(key.clone()))
            .unwrap();
        state.arrangement.commit(prepared).unwrap();
    });
    assert_eq!(
        first
            .runner
            .state()
            .composition_lifecycle
            .route(&key)
            .unwrap()
            .visible,
        false
    );

    click_row(&mut second, "chrome.shellbar.visible", "toggle");
    click_row(&mut second, "chrome.shellbar.visible", "setting-apply");
    first.sync(640.0, 780.0);
    let hidden_vector = first
        .runner
        .state()
        .published_inputs
        .borrow()
        .vector(&key)
        .cloned()
        .expect("hidden input remains in complete table");
    assert!(revision(&hidden_vector, "shell-chrome") > 0);
    assert_eq!(
        first
            .runner
            .state()
            .input_queue
            .as_ref()
            .unwrap()
            .pending_components(),
        1
    );

    first.runner.update(|state| {
        let prepared = state
            .arrangement
            .preview(ArrangementOperation::Show {
                key: key.clone(),
                container: root,
                index: 0,
            })
            .unwrap();
        state.arrangement.commit(prepared).unwrap();
    });
    let route = first
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .unwrap();
    assert!(route.visible);
    assert_eq!(
        first
            .runner
            .state()
            .input_queue
            .as_ref()
            .unwrap()
            .pending_components(),
        0
    );
}
