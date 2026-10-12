// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::app::GraphshellApp;
use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
use muniment::{Backend, MemoryBackend, StoreError, TransactFn, WriteOp};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use uuid::Uuid;

fn id(n: u128) -> CouplingId {
    CouplingId::from_uuid(Uuid::from_u128(n))
}
fn persona() -> SelectedPersonaRef {
    SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:coupling-qualification".into(),
    }
}
fn coupling<B: Backend>(app: &GraphshellApp<B>, n: u128) -> Coupling {
    app.host
        .graph()
        .couplings()
        .find(|c| c.id == id(n))
        .unwrap()
        .clone()
}

#[derive(Clone)]
struct ControlledStore {
    store: MemoryBackend,
    mode: Arc<AtomicU8>,
}
#[async_trait::async_trait]
impl Backend for ControlledStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.store.get(key).await
    }
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        self.store.put(key, value).await
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.store.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.store.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.store.scan(start, end).await
    }
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        self.store.apply(ops).await
    }
    async fn transact(&self, f: TransactFn) -> Result<(), StoreError> {
        match self.mode.load(Ordering::SeqCst) {
            1 => Err(StoreError::Backend("injected write failure".into())),
            2 => Err(StoreError::NotTransactional),
            _ => self.store.transact(f).await,
        }
    }
}

#[test]
fn exact_strength_save_and_history_reopen_preserve_siblings_and_other_edits() {
    pollster::block_on(async {
        let store = MemoryBackend::new();
        let mut app = GraphshellApp::open_or_field_inspection_fixture(store.clone(), persona())
            .await
            .unwrap();
        app.host.persist(10).await.unwrap();
        let sibling = coupling(&app, 202);
        let other = coupling(&app, 203);
        let mut draft = StrengthDraft::begin(app.host.graph(), id(201)).unwrap();
        draft.set_text("0.125".into()).unwrap();
        app.host.save_coupling_strength(&draft, 11).await.unwrap();
        assert_eq!(coupling(&app, 201).strength, 0.125);
        assert_eq!(coupling(&app, 202), sibling);
        assert_eq!(coupling(&app, 203), other);
        // An unrelated source edit uses the ordinary channel and must survive
        // the coupling's saved undo, not become a whole-graph rollback.
        let member = app.host.graph().nodes().next().unwrap().1.id;
        app.host
            .edit_node(member, "Unrelated title", std::iter::empty())
            .unwrap();
        app.host.persist(12).await.unwrap();
        assert!(app.host.coupling_history(id(201), false, 13).await.unwrap());
        assert_eq!(coupling(&app, 201).strength, 0.02);
        draft.text = "1".repeat(129);
        assert!(
            draft
                .candidate(app.host.graph())
                .unwrap_err()
                .contains("128")
        );
        assert_eq!(
            app.host.graph().get_node_by_id(member).unwrap().1.title,
            "Unrelated title"
        );
        let mut reopened = GraphshellApp::open_or_field_inspection_fixture(store, persona())
            .await
            .unwrap();
        assert_eq!(coupling(&reopened, 201).strength, 0.02);
        assert!(
            reopened
                .host
                .coupling_history(id(201), true, 14)
                .await
                .unwrap()
        );
        assert_eq!(coupling(&reopened, 201).strength, 0.125);
        assert_eq!(coupling(&reopened, 202), sibling);
        assert_eq!(coupling(&reopened, 203), other);
    });
}

#[test]
fn failures_and_nontransactional_backend_preserve_graph_history_and_draft() {
    pollster::block_on(async {
        let store = ControlledStore {
            store: MemoryBackend::new(),
            mode: Arc::new(AtomicU8::new(0)),
        };
        let mut app = GraphshellApp::open_or_field_inspection_fixture(store.clone(), persona())
            .await
            .unwrap();
        app.host.persist(10).await.unwrap();
        let before = coupling(&app, 201);
        let revision = app.host.projection_revision();
        let mut draft = StrengthDraft::begin(app.host.graph(), id(201)).unwrap();
        draft.set_text("0.5".into()).unwrap();
        for mode in [1, 2] {
            store.mode.store(mode, Ordering::SeqCst);
            assert!(app.host.save_coupling_strength(&draft, 11).await.is_err());
            assert_eq!(coupling(&app, 201), before);
            assert_eq!(app.host.projection_revision(), revision);
            assert!(!app.host.has_unstored());
            assert_eq!(draft.text, "0.5");
        }
        store.mode.store(0, Ordering::SeqCst);
        assert!(
            !app.host.coupling_history(id(201), false, 12).await.unwrap(),
            "failed apply did not install history"
        );
        app.host.save_coupling_strength(&draft, 13).await.unwrap();
        store.mode.store(1, Ordering::SeqCst);
        assert!(app.host.coupling_history(id(201), false, 14).await.is_err());
        assert_eq!(coupling(&app, 201).strength, 0.5);
        store.mode.store(0, Ordering::SeqCst);
        assert!(app.host.coupling_history(id(201), false, 15).await.unwrap());
        assert_eq!(coupling(&app, 201), before);
    });
}

#[test]
fn stale_durable_session_refuses_before_overwriting_a_competing_save() {
    pollster::block_on(async {
        let store = MemoryBackend::new();
        let mut first = GraphshellApp::open_or_field_inspection_fixture(store.clone(), persona())
            .await
            .unwrap();
        first.host.persist(10).await.unwrap();
        let mut second = GraphshellApp::open_or_field_inspection_fixture(store.clone(), persona())
            .await
            .unwrap();
        let mut draft = StrengthDraft::begin(first.host.graph(), id(201)).unwrap();
        draft.set_text("0.5".into()).unwrap();
        let mut newer = StrengthDraft::begin(second.host.graph(), id(201)).unwrap();
        newer.set_text("0.25".into()).unwrap();
        second
            .host
            .save_coupling_strength(&newer, 11)
            .await
            .unwrap();
        assert!(
            first
                .host
                .save_coupling_strength(&draft, 12)
                .await
                .unwrap_err()
                .contains("changed")
        );
        assert_eq!(coupling(&first, 201).strength, 0.02);
        assert_eq!(draft.text, "0.5");
        assert!(!first.host.has_unstored());
        let reopened = GraphshellApp::open_or_field_inspection_fixture(store, persona())
            .await
            .unwrap();
        assert_eq!(coupling(&reopened, 201).strength, 0.25);
    });
}

#[test]
fn invalid_and_changed_targets_keep_private_history_without_graph_mutation() {
    use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
    pollster::block_on(async {
        let mut app =
            GraphshellApp::open_or_field_inspection_fixture(MemoryBackend::new(), persona())
                .await
                .unwrap();
        app.host.persist(10).await.unwrap();
        let mut draft = StrengthDraft::begin(app.host.graph(), id(201)).unwrap();
        for text in ["NaN", "inf", "1e99", "words"] {
            draft.set_text(text.into()).unwrap();
            assert!(draft.candidate(app.host.graph()).is_err());
        }
        draft.set_text("0.1".into()).unwrap();
        draft.undo(false);
        assert_eq!(draft.text, "words");
        draft.undo(true);
        assert_eq!(draft.text, "0.1");
        let mut graph = app.host.graph().clone();
        apply_graph_delta(&mut graph, GraphDelta::RetractCoupling { id: id(201) });
        assert!(draft.candidate(&graph).unwrap_err().contains("withdrawn"));
        assert_eq!(coupling(&app, 201).strength, 0.02);
    });
}
