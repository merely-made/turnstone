// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::super::*;
use cambium::composition_scheduler::{DependencyStamp, InputVector, LocalInputStamp};
use cambium::reactive_diagnostics::ProducerRef;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use mere_surface_api::settings::SettingsOwnerIncarnation;

const SETTING_ID: &str = "chrome.shellbar.visible";

fn input_vector(pane: &SettingsPane) -> InputVector {
    pane.runner
        .state()
        .published_inputs
        .borrow()
        .vector(&composition::component_key(SETTING_ID))
        .cloned()
        .expect("shell visibility row input")
}

fn local_generation(input: &InputVector) -> u64 {
    input
        .local()
        .iter()
        .find(|stamp| stamp.name() == "row-session")
        .expect("row session input")
        .generation()
}

fn toggle(pane: &mut SettingsPane) {
    let dom = pane.dom_ref();
    let node = dom
        .all_with_class(dom.document(), "toggle")
        .into_iter()
        .find(|node| {
            let mut current = Some(*node);
            while let Some(candidate) = current {
                if dom.attribute(
                    candidate,
                    &Namespace::default(),
                    &LocalName::from("data-setting"),
                ) == Some(SETTING_ID)
                {
                    return true;
                }
                current = dom.parent(candidate);
            }
            false
        })
        .expect("toggle for shell visibility");
    drop(dom);
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((1.0, 1.0)));
    pane.flush_editor_commands();
}

#[test]
fn opaque_owner_incarnations_keep_dependencies_and_local_edits_live() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = SettingsPane::new(root.path().to_path_buf());
    let opaque = "owner incarnation: rev/blue".to_owned();
    pane.runner.update(|state| {
        state
            .snapshots
            .get_mut(APPLICATION_REFERENCE)
            .expect("application snapshot")
            .owner = SettingsOwnerIncarnation(opaque.clone());
    });

    let first = input_vector(&pane);
    let shell_chrome = first
        .dependencies()
        .iter()
        .find(|stamp| stamp.domain() == "shell-chrome")
        .expect("shell-chrome dependency");
    assert_ne!(shell_chrome.owner_incarnation(), 0);
    let first_token = shell_chrome.owner_incarnation();
    let source_id = shell_chrome.source_id();
    let revision = shell_chrome.revision();
    assert_eq!(
        pane.runner
            .state()
            .owner_tokens
            .borrow()
            .get(&source_id)
            .map(|(owner, token)| (owner.as_str(), *token)),
        Some((opaque.as_str(), first_token))
    );

    toggle(&mut pane);
    let edited = input_vector(&pane);
    assert!(
        local_generation(&edited) > local_generation(&first),
        "opaque owner identity must not erase the row-session dependency"
    );
    let edited_dependency = edited
        .dependencies()
        .iter()
        .find(|stamp| stamp.domain() == "shell-chrome")
        .unwrap();
    assert_eq!(edited_dependency.owner_incarnation(), first_token);
    assert_eq!(edited_dependency.revision(), revision);

    // A second opaque incarnation receives a fresh monotonic token while the
    // source id remains unchanged. This also makes the owner axis observable
    // as a dependency change to the retained component.
    let replacement = "owner-incarnation-without-uuid-format".to_owned();
    pane.runner.update(|state| {
        state
            .snapshots
            .get_mut(APPLICATION_REFERENCE)
            .unwrap()
            .owner = SettingsOwnerIncarnation(replacement.clone());
    });
    let replaced = input_vector(&pane);
    let replaced_dependency = replaced
        .dependencies()
        .iter()
        .find(|stamp| stamp.domain() == "shell-chrome")
        .unwrap();
    assert_eq!(replaced_dependency.source_id(), source_id);
    assert_ne!(replaced_dependency.owner_incarnation(), first_token);
    assert!(
        replaced_dependency.owner_incarnation() > first_token,
        "owner tokens advance without recycling on rebind"
    );
}

#[test]
fn revision_cause_matching_is_scoped_to_source_and_owner_incarnation() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = SettingsPane::new(root.path().to_path_buf());
    let source_id = pane.runner.state().source_ids[APPLICATION_REFERENCE];
    let token = composition::owner_token(
        pane.runner.state(),
        source_id,
        "opaque cause owner incarnation",
    )
    .unwrap();
    let cause = ProducerRef {
        producer_id: 41,
        sequence: 7,
    };
    pane.runner.update(|state| {
        state
            .revision_causes
            .insert((source_id, token, "shell-chrome".into(), 1), cause);
    });
    let state = pane.runner.state();
    let local = vec![LocalInputStamp::new("row-session", 1).unwrap()];
    let previous = InputVector::new(
        vec![DependencyStamp::new(source_id, token, "shell-chrome", 0).unwrap()],
        local.clone(),
    )
    .unwrap();
    let next = InputVector::new(
        vec![DependencyStamp::new(source_id, token, "shell-chrome", 1).unwrap()],
        local.clone(),
    )
    .unwrap();
    assert_eq!(
        composition::changed_input_cause(state, &previous, &next),
        vec![(cause, next.dependencies()[0].clone())]
    );

    let wrong_source = InputVector::new(
        vec![DependencyStamp::new(source_id + 1, token, "shell-chrome", 1).unwrap()],
        local.clone(),
    )
    .unwrap();
    assert!(composition::changed_input_cause(state, &previous, &wrong_source).is_empty());

    let wrong_owner = InputVector::new(
        vec![DependencyStamp::new(source_id, token + 1, "shell-chrome", 1).unwrap()],
        local,
    )
    .unwrap();
    assert!(composition::changed_input_cause(state, &previous, &wrong_owner).is_empty());
}

#[test]
fn exhausted_presentation_generation_wraps_to_a_distinct_input_and_reports_gap() {
    let root = tempfile::tempdir().unwrap();
    let mut pane = SettingsPane::new(root.path().to_path_buf());
    let key = composition::component_key(SETTING_ID);
    let old = input_vector(&pane);
    pane.runner.update(|state| {
        let snapshot = state.rows[&key].session.snapshot();
        state
            .presentation_cache
            .borrow_mut()
            .insert(key.clone(), (snapshot, u64::MAX));
    });
    let maxed = input_vector(&pane);
    assert_eq!(local_generation(&maxed), u64::MAX);

    toggle(&mut pane);
    let after = input_vector(&pane);
    assert_eq!(local_generation(&after), 0);
    assert_ne!(maxed, after, "overflow must invalidate the retained row");
    assert_ne!(old, after);
    assert_eq!(
        *pane.runner.state().input_queue_error.borrow(),
        Some(cambium::composition_scheduler::CompositionInputError::GenerationExhausted)
    );
}
