// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use muniment::MemoryBackend;
use scenograph::presentation::editor::SavedState;
use scenograph::presentation::{Context, PrimitiveKind};
use serde_json::Value;

fn graph() -> Uuid {
    Uuid::from_u128(41)
}

#[test]
fn temporary_product_owner_absence_does_not_retire_the_binding() {
    let graph = graph();
    assert!(!binding_changed(
        Some(graph),
        OwnerObservation::TemporarilyUnavailable
    ));
    assert!(binding_changed(Some(graph), OwnerObservation::NoProduct));
    assert!(binding_changed(
        Some(graph),
        OwnerObservation::Graph(Uuid::from_u128(42))
    ));
    assert!(binding_changed(None, OwnerObservation::Graph(graph)));
}
fn add(workspace: &mut RuleWorkspace, kind: PrimitiveKind) {
    workspace.begin().unwrap();
    workspace
        .edit(None, 1000, |draft| {
            draft.add(kind);
        })
        .unwrap();
}

#[test]
fn malformed_private_draft_is_undoable_but_never_applied_or_previewed() {
    let mut workspace = RuleWorkspace::new(SavedState::empty()).unwrap();
    add(&mut workspace, PrimitiveKind::Node);
    let valid = workspace.rules().unwrap();
    workspace
        .edit(None, 1100, |draft| draft.rules[0].condition = "{".into())
        .unwrap();
    assert!(workspace.rules().is_err());
    let context = Context {
        id: "n".into(),
        kind: PrimitiveKind::Node,
        facts: Default::default(),
        states: Default::default(),
        screen_size: None,
        previous_matches: Default::default(),
    };
    assert!(workspace.preview(&context).is_err());
    assert!(workspace.prepare(SaveKind::Apply).is_err());
    assert!(!workspace.busy());
    assert_eq!(workspace.committed(), &SavedState::empty().rules);
    workspace.undo_draft(false).unwrap();
    assert_eq!(workspace.rules().unwrap(), valid);
}

#[test]
fn stale_operation_completion_cannot_promote_a_new_transition() {
    let mut workspace = RuleWorkspace::new(SavedState::empty()).unwrap();
    add(&mut workspace, PrimitiveKind::Node);
    let first = workspace.prepare(SaveKind::Apply).unwrap();
    let stale_id = first.id;
    assert!(workspace.finish(stale_id, Ok(())).unwrap());
    add(&mut workspace, PrimitiveKind::Field);
    let second = workspace.prepare(SaveKind::Apply).unwrap();
    assert!(!workspace.finish(stale_id, Ok(())).unwrap());
    assert!(workspace.busy());
    assert!(
        workspace
            .finish(second.id, Err("write refused".into()))
            .is_err()
    );
    assert_eq!(workspace.committed(), &first.state.rules);
    assert!(workspace.editing());
}

#[test]
fn completion_from_a_replaced_workspace_is_ignored_even_at_same_sequence() {
    let mut old = RuleWorkspace::new(SavedState::empty()).unwrap();
    add(&mut old, PrimitiveKind::Node);
    let old_operation = old.prepare(SaveKind::Apply).unwrap();
    let mut replacement = RuleWorkspace::new(SavedState::empty()).unwrap();
    add(&mut replacement, PrimitiveKind::Field);
    let new_operation = replacement.prepare(SaveKind::Apply).unwrap();
    assert_ne!(old_operation.id, new_operation.id);
    assert!(!replacement.finish(old_operation.id, Ok(())).unwrap());
    assert!(replacement.busy());
    assert!(replacement.finish(new_operation.id, Ok(())).unwrap());
}

#[test]
fn load_save_reopen_undo_and_graph_scope_use_the_graphshell_record_adapter() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut loaded = load(backend.clone(), graph()).await.unwrap();
        add(&mut loaded.workspace, PrimitiveKind::Node);
        let draft = loaded.workspace.rules().unwrap();
        let prepared = prepare_save(
            &mut loaded.workspace,
            graph(),
            loaded.baseline.clone(),
            SaveKind::Apply,
        )
        .unwrap();
        let id = prepared.id();
        let baseline = prepared.save(backend.clone()).await.unwrap();
        assert!(loaded.workspace.finish(id, Ok(())).unwrap());
        loaded.baseline = Some(baseline);
        assert_eq!(loaded.workspace.committed(), &draft);
        let mut reopened = load(backend.clone(), graph()).await.unwrap();
        assert_eq!(reopened.workspace.committed(), &draft);
        let undo = prepare_save(
            &mut reopened.workspace,
            graph(),
            reopened.baseline.clone(),
            SaveKind::Undo,
        )
        .unwrap();
        let id = undo.id();
        let baseline = undo.save(backend.clone()).await.unwrap();
        reopened.workspace.finish(id, Ok(())).unwrap();
        reopened.baseline = Some(baseline);
        assert_eq!(reopened.workspace.committed(), &SavedState::empty().rules);
        assert_eq!(
            load(backend.clone(), Uuid::from_u128(42))
                .await
                .unwrap()
                .workspace
                .committed(),
            &SavedState::empty().rules
        );
    });
}

#[test]
fn conflict_reload_reconciles_latest_saved_state_without_losing_private_draft() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut first = load(backend.clone(), graph()).await.unwrap();
        let mut stale = load(backend.clone(), graph()).await.unwrap();
        add(&mut first.workspace, PrimitiveKind::Node);
        add(&mut stale.workspace, PrimitiveKind::Field);
        let private = stale.workspace.rules().unwrap();
        let stale_save = prepare_save(
            &mut stale.workspace,
            graph(),
            stale.baseline.clone(),
            SaveKind::Apply,
        )
        .unwrap();
        let first_save = prepare_save(
            &mut first.workspace,
            graph(),
            first.baseline.clone(),
            SaveKind::Apply,
        )
        .unwrap();
        let id = first_save.id();
        first.baseline = Some(first_save.save(backend.clone()).await.unwrap());
        first.workspace.finish(id, Ok(())).unwrap();
        let stale_id = stale_save.id();
        let result = stale_save.save(backend.clone()).await;
        assert!(matches!(result, Err(SaveFailure::Conflict(_))));
        assert!(
            stale
                .workspace
                .finish(stale_id, Err("stale baseline".into()))
                .is_err()
        );
        let latest = load(backend.clone(), graph()).await.unwrap();
        stale
            .workspace
            .reconcile(latest.workspace.saved_state().clone())
            .unwrap();
        stale.baseline = latest.baseline;
        assert_eq!(stale.workspace.committed(), first.workspace.committed());
        assert_eq!(stale.workspace.rules().unwrap(), private);
        let retry = prepare_save(
            &mut stale.workspace,
            graph(),
            stale.baseline.clone(),
            SaveKind::Apply,
        )
        .unwrap();
        let retry_id = retry.id();
        stale.baseline = Some(retry.save(backend.clone()).await.unwrap());
        stale.workspace.finish(retry_id, Ok(())).unwrap();
        assert_eq!(stale.workspace.committed(), &private);
    });
}

#[test]
fn saved_record_identity_and_validation_refuse_reopen_without_rewriting_bytes() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut loaded = load(backend.clone(), graph()).await.unwrap();
        add(&mut loaded.workspace, PrimitiveKind::Node);
        let proposal = prepare_save(
            &mut loaded.workspace,
            graph(),
            loaded.baseline.clone(),
            SaveKind::Apply,
        )
        .unwrap();
        let id = proposal.id();
        loaded.baseline = Some(proposal.save(backend.clone()).await.unwrap());
        loaded.workspace.finish(id, Ok(())).unwrap();
        let key = slot(graph());
        let bytes = backend.get(&key).await.unwrap().unwrap();
        let mut record: Value = serde_json::from_slice(&bytes).unwrap();
        record["graph_id"] = serde_json::to_value(Uuid::from_u128(42)).unwrap();
        let damaged = serde_json::to_vec(&record).unwrap();
        backend.put(&key, &damaged).await.unwrap();
        assert!(load(backend.clone(), graph()).await.is_err());
        assert_eq!(backend.get(&key).await.unwrap().unwrap(), damaged);
        record["graph_id"] = serde_json::to_value(graph()).unwrap();
        record["unexpected"] = Value::Bool(true);
        let unknown = serde_json::to_vec(&record).unwrap();
        backend.put(&key, &unknown).await.unwrap();
        assert!(load(backend.clone(), graph()).await.is_err());
        assert_eq!(backend.get(&key).await.unwrap().unwrap(), unknown);
    });
}
