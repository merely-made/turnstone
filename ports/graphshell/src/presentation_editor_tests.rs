// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use muniment::MemoryBackend;
use scenograph::presentation::{Property, ViewState};

fn session() -> PresentationSession {
    PresentationSession::new(Uuid::from_u128(41))
}
fn node(id: &str) -> Context {
    Context {
        id: id.into(),
        kind: PrimitiveKind::Node,
        facts: Default::default(),
        states: Default::default(),
        screen_size: Some(64.),
        previous_matches: Default::default(),
    }
}
fn add(session: &mut PresentationSession, kind: PrimitiveKind) {
    session.begin().unwrap();
    session
        .edit(None, 1000, |draft| {
            draft.add(kind);
        })
        .unwrap();
}
async fn save(session: &mut PresentationSession, backend: MemoryBackend, kind: SaveKind) {
    let prepared = session.prepare_save(kind).unwrap();
    let result = prepared.save(backend).await;
    session.finish_save(result).unwrap();
}

#[test]
fn malformed_input_is_undoable_but_never_applied_or_previewed() {
    let mut session = session();
    add(&mut session, PrimitiveKind::Node);
    let valid = session.rules().unwrap();
    session
        .edit(None, 1100, |d| d.rules[0].condition = "{".into())
        .unwrap();
    assert!(session.rules().is_err());
    assert!(session.preview(&node("one")).is_err());
    assert!(session.prepare_save(SaveKind::Apply).is_err());
    assert!(!session.busy());
    assert_eq!(session.committed(), &empty());
    session.undo_draft(false).unwrap();
    assert_eq!(session.rules().unwrap(), valid);
    session.undo_draft(true).unwrap();
    assert!(session.rules().is_err());
    session.discard().unwrap();
    assert_eq!(session.rules().unwrap(), empty());
}

#[test]
fn target_kind_unknown_properties_and_disabled_invalid_rules_are_refused() {
    let mut session = session();
    add(&mut session, PrimitiveKind::Node);
    session
        .edit(None, 1001, |d| {
            d.rules[0].condition =
                r#"{"kind":"not","condition":{"kind":"always","surprise":true}}"#.into()
        })
        .unwrap();
    assert!(
        session
            .rules()
            .unwrap_err()
            .iter()
            .any(|e| e.field.ends_with("condition.surprise"))
    );
    session
        .edit(None, 1002, |d| {
            d.rules[0].condition = r#"{"kind":"always"}"#.into();
            d.rules[0].target = r#"{"kind":"field","ids":[]}"#.into();
            d.rules[0].enabled = false;
        })
        .unwrap();
    assert!(
        session
            .rules()
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("incompatible"))
    );
    session
        .edit(None, 1003, |d| d.rules[0].priority = "2147483648".into())
        .unwrap();
    assert!(
        session
            .rules()
            .unwrap_err()
            .iter()
            .any(|e| e.field.ends_with("priority"))
    );
}

#[test]
fn reordering_equal_priority_rules_changes_the_winner_and_undo_restores_it() {
    let mut session = session();
    add(&mut session, PrimitiveKind::Node);
    add(&mut session, PrimitiveKind::Node);
    session
        .edit(None, 1010, |d| {
            d.rules[1].effects = r#"[{"kind":"details","visible":false}]"#.into()
        })
        .unwrap();
    let before = session.preview(&node("one")).unwrap();
    assert_eq!(before.winners[&Property::Details].rule_id, "node-2");
    session.edit(None, 1020, |d| d.rules.swap(0, 1)).unwrap();
    let after = session.preview(&node("one")).unwrap();
    assert_eq!(after.winners[&Property::Details].rule_id, "node-1");
    assert!(
        after
            .traces
            .iter()
            .any(|t| t.matched && t.winning_properties.is_empty())
    );
    session.undo_draft(false).unwrap();
    assert_eq!(session.preview(&node("one")).unwrap(), before);
}

#[test]
fn preview_does_not_change_context_and_hysteresis_is_explicit_per_occurrence() {
    let mut session = session();
    add(&mut session, PrimitiveKind::Node);
    session
        .edit(None, 1010, |d| {
            d.rules[0].target = r#"{"kind":"node","ids":["first","second"]}"#.into();
            d.rules[0].condition =
                r#"{"kind":"screen_size","enter_below":48,"exit_above":84}"#.into();
        })
        .unwrap();
    let mut first = node("first");
    first.previous_matches.insert("node-1".into());
    first.states.insert(ViewState::Selected);
    let before = first.clone();
    assert!(
        session
            .preview(&first)
            .unwrap()
            .matched_rules
            .contains("node-1")
    );
    assert_eq!(first, before);
    assert!(session.preview(&node("second")).unwrap().winners.is_empty());
    assert!(session.preview(&node("other")).unwrap().winners.is_empty());
    assert_eq!(session.committed(), &empty());
}

#[test]
fn one_acknowledged_apply_reopens_with_saved_undo_redo_scoped_to_this_graph() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        backend.put("graph/other", b"source owner").await.unwrap();
        let mut session = session();
        add(&mut session, PrimitiveKind::Node);
        add(&mut session, PrimitiveKind::Relation);
        add(&mut session, PrimitiveKind::Field);
        let draft = session.rules().unwrap();
        let proposal = session.prepare_save(SaveKind::Apply).unwrap();
        assert!(session.busy());
        assert_eq!(session.committed(), &empty());
        assert!(session.edit(None, 1001, |_| {}).is_err());
        assert!(session.discard().is_err());
        session
            .finish_save(proposal.save(backend.clone()).await)
            .unwrap();
        assert_eq!(session.committed(), &draft);
        assert!(!session.editing());
        assert_eq!(session.generation(), 1);
        let mut reopened = PresentationSession::load(backend.clone(), Uuid::from_u128(41))
            .await
            .unwrap();
        assert_eq!(reopened.committed(), &draft);
        save(&mut reopened, backend.clone(), SaveKind::Undo).await;
        assert_eq!(reopened.committed(), &empty());
        assert!(!reopened.can_undo(), "one apply is one saved step");
        let mut reopened = PresentationSession::load(backend.clone(), Uuid::from_u128(41))
            .await
            .unwrap();
        save(&mut reopened, backend.clone(), SaveKind::Redo).await;
        assert_eq!(reopened.committed(), &draft);
        assert_eq!(
            backend.get("graph/other").await.unwrap(),
            Some(b"source owner".to_vec())
        );
        let other = PresentationSession::load(backend.clone(), Uuid::from_u128(42))
            .await
            .unwrap();
        assert_eq!(other.committed(), &empty());
    });
}

#[test]
fn failed_acknowledgment_keeps_draft_and_allows_retry() {
    pollster::block_on(async {
        let mut session = session();
        add(&mut session, PrimitiveKind::Node);
        let draft = session.rules().unwrap();
        session.prepare_save(SaveKind::Apply).unwrap();
        assert!(
            session
                .finish_save(Err("injected store failure".into()))
                .is_err()
        );
        assert!(!session.busy());
        assert_eq!(session.rules().unwrap(), draft);
        assert_eq!(session.committed(), &empty());
        save(&mut session, MemoryBackend::new(), SaveKind::Apply).await;
        assert_eq!(session.committed(), &draft);
    });
}

#[test]
fn stale_workspace_write_refuses_atomically_and_preserves_both_drafts() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut first = session();
        let mut stale = session();
        add(&mut first, PrimitiveKind::Node);
        add(&mut stale, PrimitiveKind::Field);
        let stale_draft = stale.rules().unwrap();
        let prepared_stale = stale.prepare_save(SaveKind::Apply).unwrap();
        save(&mut first, backend.clone(), SaveKind::Apply).await;
        let result = prepared_stale.save(backend.clone()).await;
        assert!(result.as_ref().unwrap_err().contains("changed elsewhere"));
        assert!(stale.finish_save(result).is_err());
        assert_eq!(stale.rules().unwrap(), stale_draft);
        assert_eq!(stale.committed(), &empty());
        let loaded = PresentationSession::load(backend.clone(), Uuid::from_u128(41))
            .await
            .unwrap();
        assert_eq!(loaded.committed(), first.committed());
    });
}

#[test]
fn saved_history_and_identity_are_validated_before_reopen() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut session = session();
        add(&mut session, PrimitiveKind::Node);
        save(&mut session, backend.clone(), SaveKind::Apply).await;
        let key = slot(Uuid::from_u128(41));
        let bytes = backend.get(&key).await.unwrap().unwrap();
        let mut record: Value = serde_json::from_slice(&bytes).unwrap();
        record["undo"][0]["version"] = Value::from(99);
        let damaged = serde_json::to_vec(&record).unwrap();
        backend.put(&key, &damaged).await.unwrap();
        assert!(
            PresentationSession::load(backend.clone(), Uuid::from_u128(41))
                .await
                .is_err()
        );
        assert_eq!(backend.get(&key).await.unwrap().unwrap(), damaged);
        backend
            .put(&slot(Uuid::from_u128(42)), &bytes)
            .await
            .unwrap();
        assert!(
            PresentationSession::load(backend, Uuid::from_u128(42))
                .await
                .is_err()
        );
    });
}

#[test]
fn draft_and_saved_history_are_bounded_and_noop_apply_does_not_save() {
    pollster::block_on(async {
        let mut session = session();
        session.begin().unwrap();
        assert!(session.prepare_save(SaveKind::Apply).is_err());
        assert!(!session.can_undo());
        add(&mut session, PrimitiveKind::Node);
        assert!(
            session
                .edit(None, 1001, |d| d.rules[0].label = "x".repeat(RECORD_LIMIT))
                .is_err()
        );
        let backend = MemoryBackend::new();
        save(&mut session, backend.clone(), SaveKind::Apply).await;
        for i in 0..35 {
            session.begin().unwrap();
            session
                .edit(None, 1002, |d| d.rules[0].label = format!("Label {i}"))
                .unwrap();
            save(&mut session, backend.clone(), SaveKind::Apply).await;
        }
        for _ in 0..HISTORY_LIMIT {
            save(&mut session, backend.clone(), SaveKind::Undo).await;
        }
        assert!(!session.can_undo());
        assert!(session.can_redo());
    });
}
