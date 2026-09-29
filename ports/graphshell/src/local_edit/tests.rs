// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef};
use muniment::{Backend, MemoryBackend, StoreError, TransactFn, WriteOp};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn persona() -> SelectedPersonaRef {
    SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-h3".into(),
    }
}

#[test]
fn saved_detail_edit_reopens_same_session_and_member() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona())
            .await
            .unwrap();
        app.host.persist(1).await.unwrap();
        app.mount_local().unwrap();
        let session = app.host.graph_session().id();
        let id = app
            .host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        let saved = save_metadata(
            &mut app,
            id,
            "  Tree saved title  ",
            "beta, alpha, beta, ",
            2,
        )
        .await
        .unwrap();
        assert_eq!(saved.title, "Tree saved title");
        assert_eq!(saved.tags, ["alpha", "beta"]);
        let reopened = GraphshellApp::open_or_fixture(backend, persona())
            .await
            .unwrap();
        assert!(reopened.host.was_reopened());
        assert_eq!(reopened.host.graph_session().id(), session);
        assert_eq!(metadata(&reopened, id).unwrap(), saved);
    });
}

#[test]
fn stale_detail_member_does_not_mutate_or_write() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona())
            .await
            .unwrap();
        app.host.persist(1).await.unwrap();
        let keys = backend.list("").await.unwrap();
        let mut stored = Vec::new();
        for key in &keys {
            stored.push((key.clone(), backend.get(key).await.unwrap()));
        }
        let mut before = app.host.graph().to_snapshot();
        before.timestamp_secs = 0;
        let error = save_metadata(&mut app, Uuid::nil(), "wrong", "wrong", 2)
            .await
            .unwrap_err();
        assert!(error.contains("no longer exists"));
        let mut after = app.host.graph().to_snapshot();
        after.timestamp_secs = 0;
        assert_eq!(
            serde_json::to_value(after).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert_eq!(backend.list("").await.unwrap(), keys);
        for (key, bytes) in stored {
            assert_eq!(backend.get(&key).await.unwrap(), bytes);
        }
    });
}

#[derive(Clone)]
struct RejectWrites {
    inner: MemoryBackend,
    reject: Arc<AtomicBool>,
}

impl RejectWrites {
    fn check(&self) -> Result<(), StoreError> {
        if self.reject.load(Ordering::SeqCst) {
            Err(StoreError::Backend("rejected detail write".into()))
        } else {
            Ok(())
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for RejectWrites {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.check()?;
        self.inner.put(key, bytes).await
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.check()?;
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        self.check()?;
        self.inner.apply(ops).await
    }
    async fn transact(&self, work: TransactFn) -> Result<(), StoreError> {
        self.check()?;
        self.inner.transact(work).await
    }
}

#[test]
fn rejected_detail_save_reports_failure_and_can_retry() {
    pollster::block_on(async {
        let backend = RejectWrites {
            inner: MemoryBackend::new(),
            reject: Arc::new(AtomicBool::new(false)),
        };
        let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona())
            .await
            .unwrap();
        app.host.persist(1).await.unwrap();
        let id = app
            .host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        let before = metadata(&app, id).unwrap();
        backend.reject.store(true, Ordering::SeqCst);
        assert!(
            save_metadata(&mut app, id, "retry me", "retry", 2)
                .await
                .unwrap_err()
                .contains("rejected detail write")
        );
        let stored = GraphshellApp::open_or_fixture(backend.inner.clone(), persona())
            .await
            .unwrap();
        assert_eq!(metadata(&stored, id).unwrap(), before);
        backend.reject.store(false, Ordering::SeqCst);
        save_metadata(&mut app, id, "retry me", "retry", 3)
            .await
            .unwrap();
        let stored = GraphshellApp::open_or_fixture(backend, persona())
            .await
            .unwrap();
        assert_eq!(metadata(&stored, id).unwrap().title, "retry me");
    });
}

#[test]
fn canvas_metadata_refresh_preserves_geometry_camera_selection_and_play_state() {
    let app = GraphshellApp::fixture(MemoryBackend::new(), persona()).unwrap();
    let id = app
        .host
        .graph()
        .get_node_by_url(FIXTURE_WEB_ADDRESS)
        .unwrap()
        .1
        .id;
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    let positions = canvas
        .graph()
        .nodes()
        .enumerate()
        .map(|(index, (_, node))| (node.id, (index as f32 * 17.0, index as f32 * 9.0)))
        .collect::<Vec<_>>();
    canvas.seed_cartography(positions);
    canvas.set_selected_members(&[id]);
    canvas.set_camera(mere::canvas::CameraView {
        offset: (31.0, 23.0),
        zoom: 1.4,
    });
    canvas.set_physics_paused(false);
    let before = canvas.cartography_geometry();
    let camera = canvas.camera();
    let mut saved = metadata(&app, id).unwrap();
    saved.title = "Refreshed title".into();
    saved.tags = vec!["only-new-tag".into()];
    sync_canvas_metadata(&mut canvas, &saved).unwrap();
    assert_eq!(canvas.cartography_geometry(), before);
    assert_eq!(canvas.camera(), camera);
    assert_eq!(canvas.selected_members(), [id]);
    assert!(!canvas.physics_paused());
    let node = canvas.graph().get_node_by_id(id).unwrap().1;
    assert_eq!(node.title, saved.title);
    assert_eq!(node.tags.iter().cloned().collect::<Vec<_>>(), saved.tags);
}
