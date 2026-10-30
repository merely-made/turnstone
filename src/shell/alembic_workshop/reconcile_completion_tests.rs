// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use alembic::workshop::{Destination, Review};
use athanor::{
    workshop::{Workshop, evaluate},
    workshop_destination as destination,
};
use mere::kernel::graph::{
    Graph,
    apply::{GraphDeltaResult, apply_graph_delta},
};
use muniment::{Backend, MemoryBackend, StoreError, TransactFn, WriteOp};
use servitor::{AuthorityProvider, Cap, Mode, Subject};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct LostAckStore {
    inner: MemoryBackend,
    fail_after_commit: Arc<AtomicBool>,
}
#[async_trait::async_trait]
impl Backend for LostAckStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        self.inner.put(key, value).await
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        self.inner.apply(ops).await
    }
    async fn transact(&self, transaction: TransactFn) -> Result<(), StoreError> {
        self.inner.transact(transaction).await?;
        if self.fail_after_commit.swap(false, Ordering::SeqCst) {
            return Err(StoreError::NotTransactional);
        }
        Ok(())
    }
}
struct Granted;
impl AuthorityProvider for Granted {
    fn covers(&self, _: Subject, _: &Cap, _: Mode) -> bool {
        true
    }
}

#[test]
fn reconciled_note_is_live_even_when_helper_ack_commits_then_errors() {
    pollster::block_on(async {
        let owner = Owner {
            host: "turnstone-recovery-test".into(),
            workspace: uuid::Uuid::new_v4().to_string(),
            session: uuid::Uuid::new_v4().to_string(),
        };
        let mut original = Graph::default();
        for delta in crate::alembic_workshop::example_seed_deltas(&original) {
            assert!(!matches!(
                apply_graph_delta(&mut original, delta),
                GraphDeltaResult::Refused(_)
            ));
        }
        let selected = crate::alembic_workshop::example_choices(&original)
            .into_iter()
            .map(|choice| choice.key)
            .collect::<Vec<_>>();
        let inputs =
            crate::alembic_workshop::capture_public_examples(&original, &owner, &selected).unwrap();
        let store = LostAckStore {
            inner: MemoryBackend::new(),
            fail_after_commit: Arc::new(AtomicBool::new(false)),
        };
        let workshop = Workshop::new(store.clone(), owner.clone(), [3; 16]);
        workshop.install([1; 32]).await.unwrap();
        workshop.begin(1, inputs, &Granted, 1).await.unwrap();
        for step in 0..3 {
            let dispatch = workshop.dispatch(1, &Granted, 2 + step * 2).await.unwrap();
            workshop
                .observe(evaluate(dispatch).unwrap(), &Granted, 3 + step * 2)
                .await
                .unwrap();
        }
        let proposal = workshop.view(Some(1)).await.unwrap().proposal.unwrap();
        let acceptance = workshop
            .accept(
                1,
                Destination {
                    owner: owner.clone(),
                    graph: owner.workspace.clone(),
                    revision: destination::graph_revision(&original).unwrap(),
                },
                [2; 32],
                &Granted,
                10,
            )
            .await
            .unwrap();
        let destination::GraphPreparation::New { deltas, receipt } = destination::prepare_graph(
            &original,
            &owner,
            &owner.workspace,
            &destination::graph_revision(&original).unwrap(),
            &acceptance,
            &proposal,
            [2; 32],
            &Granted,
            10,
            true,
        )
        .unwrap() else {
            panic!("new destination expected")
        };
        let mut saved = original.clone();
        for delta in deltas {
            assert!(!matches!(
                apply_graph_delta(&mut saved, delta),
                GraphDeltaResult::Refused(_)
            ));
        }
        // The destination committed but its caller has not installed that
        // graph or recorded the helper acknowledgement yet.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("graph.json");
        std::fs::write(&path, serde_json::to_vec(&saved.to_snapshot_at(0)).unwrap()).unwrap();
        let snapshot = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let recovered = Graph::try_from_recorded_snapshot(&snapshot).unwrap();
        assert_eq!(
            destination::query_graph(&recovered, &owner, &owner.workspace, &acceptance, &proposal)
                .unwrap(),
            Some(receipt.clone())
        );
        let live = RefCell::new(original);
        store.fail_after_commit.store(true, Ordering::SeqCst);
        let result = install_destination_then_acknowledge(
            || {
                *live.borrow_mut() = recovered;
            },
            || pollster::block_on(workshop.acknowledge(1, receipt.clone())),
        );
        assert!(
            result.is_err(),
            "fault must occur after the actual journal commit"
        );
        assert!(
            matches!(
                workshop.view(Some(1)).await.unwrap().review,
                Some(Review::Added { .. })
            ),
            "acknowledgement actually committed before its reply failed"
        );
        let note = destination::result_surface(&acceptance);
        assert!(
            live.borrow().get_node_by_id(note).is_some(),
            "a proven durable note disappeared from the live owner after a lost helper acknowledgement"
        );
        // A fresh journal owner and the cold destination still agree on
        // the one operation, so retry never requires adding a second note.
        let reopened = Workshop::new(store, owner.clone(), [3; 16]);
        assert!(
            matches!(reopened.view(Some(1)).await.unwrap().review, Some(Review::Added { receipt: held, .. }) if held == receipt)
        );
        assert_eq!(live.borrow().node_count(), saved.node_count());
    });
}
