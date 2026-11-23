// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use athanor::alembic::workshop::{Destination, Review, WorkshopView};
use athanor::workshop_destination as destination;
use kernel::graph::apply::{GraphDeltaResult, apply_graph_delta};
use muniment::{Backend, DirectoryBackend, StoreError, TransactFn, WriteOp};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct LostAckStore {
    inner: DirectoryBackend,
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
#[test]
fn reconciled_note_is_live_even_when_helper_ack_commits_then_errors() {
    const COLD_ROOT: &str = "TURNSTONE_TEST_ALEMBIC_COLD_ROOT";
    if let Some(root) = std::env::var_os(COLD_ROOT) {
        verify_cold_owner(std::path::Path::new(&root));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    pollster::block_on(async {
        let mut app = crate::app::App::test_stub_at(path.join("profile"));
        let graph_id = app.graph_runtimes.active_graph();
        let owner = crate::alembic_workshop::owner(
            "turnstone-public-example",
            *graph_id.as_uuid(),
            *app.session_id.as_uuid(),
        );
        let canvas = &mut app.graph_runtimes.get_mut(graph_id).unwrap().canvas;
        canvas.ingest_graph(|graph| {
            for delta in crate::alembic_workshop::example_seed_deltas(graph) {
                assert!(!matches!(
                    apply_graph_delta(graph, delta),
                    GraphDeltaResult::Refused(_)
                ));
            }
            true
        });
        let original = canvas.graph().clone();
        let selected = crate::alembic_workshop::example_choices(&original)
            .into_iter()
            .map(|choice| choice.key)
            .collect::<Vec<_>>();
        let inputs =
            crate::alembic_workshop::capture_public_examples(&original, &owner, &selected).unwrap();
        let store = LostAckStore {
            inner: DirectoryBackend::open(path.join("workshop")).unwrap(),
            fail_after_commit: Arc::new(AtomicBool::new(false)),
        };
        let install = installation(&owner);
        let human = [2; 32];
        let now = crate::denizen::now_ms;
        let authority = crate::alembic_workshop::authority(&owner, install, human, &inputs, now());
        let gate = |p: &_| workshop_passage_current(&app, &owner, None, p);
        let mut driver = Driver::new(store.clone(), owner.clone(), install, Arc::new(|| {}));
        driver.open(subject(&owner)).await.unwrap();
        driver
            .begin(inputs.clone(), &authority, &now, &gate)
            .await
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while driver.busy() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not finish"
            );
            driver.poll(&authority, &now, &gate).await.unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let view = driver.refresh().await.unwrap();
        assert_eq!(view.review, Some(Review::AwaitingReview));
        assert!(view.turns.is_empty());
        assert_eq!(view.usage.tokens, 0);
        let proposal = view.proposal.unwrap();
        assert_eq!(proposal.sources, inputs);
        let target = Destination {
            owner: owner.clone(),
            graph: owner.workspace.clone(),
            revision: destination::graph_revision(&original).unwrap(),
        };
        let sources = inputs.iter().map(|p| p.source.clone()).collect::<Vec<_>>();
        let authority = crate::alembic_workshop::authority_for_destination(
            &owner,
            install,
            human,
            &target,
            &sources,
            now(),
        );
        let workshop = &driver.workshop;
        let acceptance = workshop
            .accept(1, target, human, &authority, now())
            .await
            .unwrap();
        let destination::GraphPreparation::New { deltas, receipt } = destination::prepare_graph(
            &original,
            &owner,
            &owner.workspace,
            &destination::graph_revision(&original).unwrap(),
            &acceptance,
            &proposal,
            human,
            &authority,
            now(),
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
        {
            let writer =
                crate::session_persistence::SessionPersistence::writable(path.into(), None)
                    .unwrap();
            writer
                .save_graph_with(
                    path,
                    &saved,
                    || pandect::save_node_facets(path, saved.facets()),
                    |placement| {
                        crate::session_graph_file::save_profiled(
                            &path.join("graph.json"),
                            &saved,
                            placement,
                        )
                    },
                )
                .unwrap();
        }
        let recovered = crate::session::try_load_session_graph(path)
            .unwrap()
            .unwrap()
            .graph;
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
        let view = workshop.view(Some(1)).await.unwrap();
        assert_eq!(live.borrow().node_count(), saved.node_count());
        std::fs::write(
            path.join("expected.json"),
            serde_json::to_vec(&(owner, view, saved.node_count())).unwrap(),
        )
        .unwrap();
    });
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "reconciled_note_is_live_even_when_helper_ack_commits_then_errors",
            "--nocapture",
        ])
        .env(COLD_ROOT, path)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(path.join("verified").exists(), "cold verifier did not run");
}

fn verify_cold_owner(path: &std::path::Path) {
    let (owner, expected, count): (Owner, WorkshopView, usize) =
        serde_json::from_slice(&std::fs::read(path.join("expected.json")).unwrap()).unwrap();
    let store = DirectoryBackend::open(path.join("workshop")).unwrap();
    let driver = Driver::new(store, owner.clone(), expected.installation, Arc::new(|| {}));
    let view = pollster::block_on(driver.open(subject(&owner))).unwrap();
    assert_eq!(view, expected);
    let Some(Review::Added {
        acceptance,
        receipt,
    }) = &view.review
    else {
        panic!("cold journal lost the acknowledged note")
    };
    let graph = crate::session::try_load_session_graph(path)
        .unwrap()
        .unwrap()
        .graph;
    assert_eq!(graph.node_count(), count);
    assert_eq!(
        destination::query_graph(
            &graph,
            &owner,
            &owner.workspace,
            acceptance,
            view.proposal.as_ref().unwrap()
        )
        .unwrap(),
        Some(receipt.clone())
    );
    std::fs::write(path.join("verified"), b"one recovered note").unwrap();
}
