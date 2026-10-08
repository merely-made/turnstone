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
    use mere::kernel::graph::Author;
    use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
    let mut app = GraphshellApp::fixture(MemoryBackend::new(), persona()).unwrap();
    let id = app
        .host
        .create_address("https://canvas-metadata.test/page", "original")
        .unwrap();
    app.host
        .edit_node(id, "original", ["owned-old-tag".into()])
        .unwrap();
    let key = app.host.graph().get_node_key_by_id(id).unwrap();
    app.host.mutate_product_graph(|graph| {
        graph.write_as(Author::person("urn:mere:canvas-metadata-peer"), |graph| {
            apply_graph_delta(
                graph,
                GraphDelta::InsertNodeTag {
                    key,
                    tag: "peer-tag".into(),
                },
            );
        });
    });
    let peer = app
        .host
        .graph()
        .to_snapshot()
        .resource_edges
        .into_iter()
        .filter_map(|edge| edge.semantic)
        .flat_map(|bucket| bucket.statements)
        .find(|claim| claim.provenance_iri.as_deref() == Some("urn:mere:canvas-metadata-peer"))
        .expect("peer supplied an independent tagging assertion");
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
    app.host
        .edit_node(id, "Refreshed title", ["only-new-tag".into()])
        .unwrap();
    let saved = metadata(&app, id).unwrap();
    assert_eq!(saved.tags, ["only-new-tag", "peer-tag"]);
    sync_canvas_metadata_from_graph(&mut canvas, app.host.graph(), &saved).unwrap();
    assert_eq!(canvas.cartography_geometry(), before);
    assert_eq!(canvas.camera(), camera);
    assert_eq!(canvas.selected_members(), [id]);
    assert!(!canvas.physics_paused());
    let (key, node) = canvas.graph().get_node_by_id(id).unwrap();
    assert_eq!(node.title, saved.title);
    assert_eq!(
        canvas
            .graph()
            .node_content_tags(key)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        saved.tags
    );
    assert!(
        canvas
            .graph()
            .to_snapshot()
            .resource_edges
            .iter()
            .filter_map(|edge| edge.semantic.as_ref())
            .flat_map(|bucket| &bucket.statements)
            .any(|claim| *claim == peer),
        "the peer's exact handle, source, time and label survive the refresh"
    );
}

#[test]
fn hosted_canvas_metadata_keeps_exact_tag_claims_peer_sources_and_navigation_resources() {
    use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
    use mere::kernel::graph::resource::TAGGED_WITH_IRI;
    use mere::kernel::graph::{Author, CapturedDelta};
    let mut app = GraphshellApp::fixture(MemoryBackend::new(), persona()).unwrap();
    let first = app
        .host
        .create_address("https://metadata.test/page", "page")
        .unwrap();
    let alias = app
        .host
        .create_address("https://metadata.test/page#alias", "alias")
        .unwrap();
    let key = app.host.graph().get_node_key_by_id(first).unwrap();
    app.host.mutate_product_graph(|graph| {
        graph.write_as(Author::person("urn:mere:metadata-peer"), |graph| {
            apply_graph_delta(
                graph,
                GraphDelta::InsertNodeTag {
                    key,
                    tag: "Peer".into(),
                },
            );
        })
    });
    app.host.edit_node(first, "page", ["Own".into()]).unwrap();
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas.seed_cartography([(first, (17.0, 9.0)), (alias, (29.0, 13.0))]);
    canvas.set_selected_members(&[first]);
    canvas.set_physics_paused(false);
    let geometry = canvas.cartography_geometry();
    let camera = canvas.camera();
    app.host
        .edit_node(first, "updated", Vec::<String>::new())
        .unwrap();
    let saved = metadata(&app, first).unwrap();
    assert_eq!(saved.tags, ["Peer"]);
    sync_canvas_metadata_from_graph(&mut canvas, app.host.graph(), &saved).unwrap();
    let resource = app.host.graph().shown_resource_id(key).unwrap().to_string();
    let tag_claims = |graph: &mere::kernel::graph::Graph, resource: &str| {
        graph
            .to_snapshot()
            .resource_edges
            .into_iter()
            .filter(|edge| edge.from_node_id == resource)
            .filter_map(|edge| edge.semantic)
            .flat_map(|bucket| bucket.statements)
            .filter(|claim| claim.predicate == TAGGED_WITH_IRI)
            .collect::<Vec<_>>()
    };
    let expected = tag_claims(app.host.graph(), &resource);
    assert_eq!(expected.len(), 1);
    for collision in [false, true] {
        let mut carried = canvas.graph().clone();
        let mut property =
            mere::kernel::types::NodeProperty::new("urn:test:predicate".into(), "exact".into())
                .with_metadata(Some("urn:test:source".into()), Some(42));
        property.statement_id = if collision {
            expected[0].statement_id.clone()
        } else {
            "surface-literal-handle".into()
        };
        carried
            .facets_mut()
            .set(
                first,
                chartulary::FacetId::new(mere::kernel::graph::node_facets::SEMANTIC_PROPERTIES),
                serde_json::to_value(vec![property.clone(), property]).unwrap(),
                &chartulary::AcceptAll,
            )
            .unwrap();
        carried
            .facets_mut()
            .set(
                first,
                chartulary::FacetId::new("extension.origin-note"),
                serde_json::json!({"statement_id":expected[0].statement_id}),
                &chartulary::AcceptAll,
            )
            .unwrap();
        let mut probe = Canvas::with_graph(carried.clone());
        let revision = probe.graph().revision();
        let title_only = [CapturedDelta::ReplaySetNodeTitleById {
            node_id: first.to_string(),
            title: "guarded update".into(),
        }];
        let result = probe.refresh_recorded_metadata(&title_only);
        assert_eq!(result.is_err(), collision);
        assert_eq!(tag_claims(probe.graph(), &resource), expected);
        assert_eq!(probe.graph().facets(), carried.facets());
        if collision {
            assert_eq!(probe.graph().revision(), revision);
            assert_eq!(
                probe.graph().get_node(key).unwrap().title,
                carried.get_node(key).unwrap().title
            );
        } else {
            assert_eq!(result, Ok(true));
            assert_eq!(probe.graph().get_node(key).unwrap().title, "guarded update");
            assert_eq!(
                probe.graph().revision(),
                revision,
                "title metadata does not alter structural revision"
            );
            assert_eq!(probe.refresh_recorded_metadata(&title_only), Ok(false));
            assert_eq!(probe.graph().revision(), revision);
        }
    }
    assert_eq!(
        expected[0].provenance_iri.as_deref(),
        Some("urn:mere:metadata-peer")
    );
    assert_eq!(tag_claims(canvas.graph(), &resource), expected);
    assert_eq!(canvas.cartography_geometry(), geometry);
    assert_eq!(canvas.camera(), camera);
    assert_eq!(canvas.selected_members(), [first]);
    assert!(!canvas.physics_paused());
    let alias_key = canvas.graph().get_node_key_by_id(alias).unwrap();
    assert_eq!(
        canvas
            .graph()
            .node_content_tags(alias_key)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        ["Peer"]
    );
    let before = canvas.graph().revision();
    sync_canvas_metadata_from_graph(&mut canvas, app.host.graph(), &saved).unwrap();
    assert_eq!(canvas.graph().revision(), before);
    let mut wrong = saved.clone();
    wrong.title = "different from source".into();
    assert!(sync_canvas_metadata_from_graph(&mut canvas, app.host.graph(), &wrong).is_err());
    assert_eq!(canvas.graph().revision(), before);

    app.host.mutate_product_graph(|graph| {
        apply_graph_delta(
            graph,
            GraphDelta::NavigateNode {
                key,
                url: "https://metadata.test/next".into(),
            },
        )
    });
    let next = metadata(&app, first).unwrap();
    assert!(next.tags.is_empty());
    sync_canvas_metadata_from_graph(&mut canvas, app.host.graph(), &next).unwrap();
    assert_ne!(
        canvas.graph().shown_resource_id(key),
        Some(uuid::Uuid::parse_str(&resource).unwrap())
    );
    assert_eq!(
        tag_claims(canvas.graph(), &resource),
        expected,
        "the alias keeps the earlier resource's claims"
    );
    assert!(canvas.graph().node_content_tags(key).unwrap().is_empty());
    let unchanged = canvas.cartography_geometry();
    assert!(
        canvas
            .refresh_recorded_metadata(&[CapturedDelta::ReplaySetNodeUrlById {
                node_id: first.to_string(),
                new_url: "https://unrelated.test/".into(),
            }])
            .is_err()
    );
    assert_eq!(canvas.cartography_geometry(), unchanged);
    assert!(
        canvas
            .refresh_recorded_metadata(&[CapturedDelta::ReplaySetShownResourceById {
                surface_id: first.to_string(),
                resource_id: Some(uuid::Uuid::nil().to_string()),
            }])
            .is_err()
    );
}
