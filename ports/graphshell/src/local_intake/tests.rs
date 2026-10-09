// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef};
use crate::product::LOCAL_FILE_FACET;
use mere::canvas::{CameraView, Face, PhysicsChoice, PhysicsLaw, Role};
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

async fn stored(backend: &impl Backend) -> Vec<(String, Option<Vec<u8>>)> {
    let mut result = Vec::new();
    for key in backend.list("").await.unwrap() {
        result.push((key.clone(), backend.get(&key).await.unwrap()));
    }
    result
}

fn graph_bytes(graph: &Graph) -> Vec<u8> {
    let mut snapshot = graph.to_snapshot();
    snapshot.timestamp_secs = 0;
    serde_json::to_vec(&snapshot).unwrap()
}

#[test]
fn intake_admission_refuses_before_graph_or_store_changes() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona())
            .await
            .unwrap();
        app.host.persist(1).await.unwrap();
        let before = graph_bytes(app.host.graph());
        let changes = app.host.graph_session().changes().len();
        let durable = stored(&backend).await;

        assert!(create_address(&mut app, " \t ", "bad").is_err());
        assert!(create_file(&mut app, " ", None, None, b"abc").is_err());
        assert!(persist_intake(&mut app, Uuid::nil(), 2).await.is_err());

        assert_eq!(graph_bytes(app.host.graph()), before);
        assert_eq!(app.host.graph_session().changes().len(), changes);
        assert_eq!(stored(&backend).await, durable);
    });
}

#[test]
fn file_metadata_hashes_actual_bytes_and_preserves_picker_facts() {
    let data = file_metadata(
        "receipt.unknown",
        Some("application/x-receipt"),
        Some(42),
        b"abc",
    )
    .unwrap();
    assert_eq!(
        data.content_hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(data.name, "receipt.unknown");
    assert_eq!(data.media_type, "application/x-receipt");
    assert_eq!(data.byte_len, 3);
    assert_eq!(data.last_modified_ms, 42);

    let empty = file_metadata("empty", None, None, b"").unwrap();
    assert_eq!(
        empty.content_hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(empty.byte_len, 0);
    assert_eq!(empty.media_type, "");
    assert_eq!(empty.last_modified_ms, 0);
}

#[test]
fn address_and_file_intake_reopen_same_session_and_members() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona())
            .await
            .unwrap();
        app.host.persist(1).await.unwrap();
        let session = app.host.graph_session().id();
        let count = app.host.graph().node_count();

        let address =
            create_address(&mut app, " https://example.net/intake ", " Intake title ").unwrap();
        let saved_address = persist_intake(&mut app, address, 2).await.unwrap();
        assert_eq!(saved_address.address, "https://example.net/intake");
        assert_eq!(saved_address.title, "Intake title");
        assert_eq!(
            create_address(&mut app, "https://example.net/intake", "replacement").unwrap(),
            address
        );
        assert_eq!(metadata(&app, address).unwrap().title, "Intake title");

        let file = create_file(
            &mut app,
            "receipt.unknown",
            Some("application/x-receipt"),
            Some(42),
            b"abc",
        )
        .unwrap();
        let saved_file = persist_intake(&mut app, file, 3).await.unwrap();
        assert!(saved_file.address.starts_with("ni:"));
        assert_eq!(app.host.graph().node_count(), count + 2);
        assert_eq!(
            create_file(
                &mut app,
                "receipt.unknown",
                Some("application/x-receipt"),
                Some(42),
                b"abc"
            )
            .unwrap(),
            file
        );
        persist_intake(&mut app, file, 4).await.unwrap();
        assert_eq!(app.host.graph().node_count(), count + 2);

        let reopened = GraphshellApp::open_or_fixture(backend, persona())
            .await
            .unwrap();
        assert!(reopened.host.was_reopened());
        assert_eq!(reopened.host.graph_session().id(), session);
        assert_eq!(metadata(&reopened, address).unwrap(), saved_address);
        assert_eq!(metadata(&reopened, file).unwrap(), saved_file);
        let local_file = reopened
            .host
            .facet_value(&saved_file.address, LOCAL_FILE_FACET)
            .unwrap();
        assert_eq!(local_file["name"], "receipt.unknown");
        assert_eq!(local_file["last_modified_ms"], 42);
        assert_eq!(
            reopened
                .host
                .graph()
                .get_node_by_id(file)
                .unwrap()
                .1
                .media_type
                .as_deref(),
            Some("application/x-receipt")
        );
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
            Err(StoreError::Backend("rejected intake write".into()))
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
fn rejected_intake_write_retries_same_pending_member_without_duplicate() {
    pollster::block_on(async {
        let backend = RejectWrites {
            inner: MemoryBackend::new(),
            reject: Arc::new(AtomicBool::new(false)),
        };
        let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona())
            .await
            .unwrap();
        app.host.persist(1).await.unwrap();
        let count = app.host.graph().node_count();
        let before = stored(&backend).await;
        let id =
            create_address(&mut app, "https://example.net/retry-intake", "Retry title").unwrap();
        backend.reject.store(true, Ordering::SeqCst);
        assert!(
            persist_intake(&mut app, id, 2)
                .await
                .unwrap_err()
                .contains("rejected intake write")
        );
        assert_eq!(app.host.graph().node_count(), count + 1);
        assert!(app.host.graph().get_node_by_id(id).is_some());
        assert_eq!(stored(&backend).await, before);

        backend.reject.store(false, Ordering::SeqCst);
        let saved = persist_intake(&mut app, id, 3).await.unwrap();
        assert_eq!(app.host.graph().node_count(), count + 1);
        let reopened = GraphshellApp::open_or_fixture(backend, persona())
            .await
            .unwrap();
        assert_eq!(metadata(&reopened, id).unwrap(), saved);
        assert_eq!(reopened.host.graph().node_count(), count + 1);
    });
}

#[test]
fn intake_ingest_preserves_existing_layout_and_physics_while_selecting_new_member() {
    for paused in [false, true] {
        let mut app = GraphshellApp::fixture(MemoryBackend::new(), persona()).unwrap();
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        let positions = canvas
            .graph()
            .nodes()
            .enumerate()
            .map(|(i, (_, node))| (node.id, (i as f32 * 23.0, i as f32 * 11.0)))
            .collect::<Vec<_>>();
        canvas.seed_cartography(positions.clone());
        let original = canvas
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        canvas.set_selected_members(&[original]);
        canvas.set_member_role(original, Some(Role::Pinned));
        canvas.set_node_face(original, Face::Bare);
        // Charge, set through the canvas's spec and its flat view (F162).
        let mut spec = canvas.dynamics_spec().unwrap();
        PhysicsChoice {
            law: PhysicsLaw::Charge,
            ..PhysicsChoice::live(&canvas)
        }
        .write_into(&mut spec);
        canvas.set_dynamics_spec(&spec).unwrap();
        canvas.set_physics_paused(paused);
        canvas.set_camera(CameraView {
            offset: (31.0, 23.0),
            zoom: 1.4,
        });
        let camera = canvas.camera();
        let roles = canvas.arrangement_roles().clone();
        let choice = PhysicsChoice::live(&canvas);
        let old_key = canvas.graph().get_node_key_by_id(original).unwrap();

        let added =
            create_address(&mut app, "https://example.net/new-intake", "New intake").unwrap();
        sync_canvas_intake(&mut canvas, app.host.graph(), added).unwrap();

        let geometry = canvas.cartography_geometry();
        for (id, point) in &positions {
            assert_eq!(
                geometry
                    .iter()
                    .find(|(member, _)| member == id)
                    .map(|(_, point)| point),
                Some(*point)
            );
        }
        assert_eq!(canvas.camera(), camera);
        assert_eq!(canvas.arrangement_roles(), &roles);
        assert_eq!(PhysicsChoice::live(&canvas), choice);
        assert_eq!(canvas.physics_paused(), paused);
        assert_eq!(canvas.node_face(old_key), Face::Bare);
        assert_eq!(canvas.selected_members(), [added]);
        assert!(canvas.graph().get_node_by_id(added).is_some());
        assert!(geometry.iter().any(|(member, _)| member == added));
    }
}
