// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn resource_content_inspector_follows_shared_resource_and_navigation() {
    let (mut app, first, second, resource) = resource_content_fixture();
    let pane = app.default_graph_pane();
    for member in [first, second] {
        assert!(app.graph_pane_select_member(pane, member));
        let lines = crate::inspector_view::inspector_lines(&app);
        assert!(
            lines.iter().any(|line| line == "Tags: Paper, paper"),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line == &format!("Node id: {member}"))
        );
    }

    app.update(Action::ContentNavigationCommitted {
        member: first,
        url: "https://example.test/next".into(),
    });
    let graph = app.graph_runtimes.graph();
    let first_key = graph.get_node_by_id(first).unwrap().0;
    let second_key = graph.get_node_by_id(second).unwrap().0;
    assert_ne!(graph.shown_resource_id(first_key), Some(resource));
    assert_eq!(graph.shown_resource_id(second_key), Some(resource));
    assert!(app.graph_pane_select_member(pane, first));
    assert!(
        crate::inspector_view::inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: none")
    );
    assert!(app.graph_pane_select_member(pane, second));
    assert!(
        crate::inspector_view::inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: Paper, paper")
    );

    // Returning the same Surface to the old resource restores the content
    // read without copying the sibling's tags into the Surface.
    app.update(Action::ContentNavigationCommitted {
        member: first,
        url: "https://example.test/page#three".into(),
    });
    let graph = app.graph_runtimes.graph();
    let key = graph.get_node_by_id(first).unwrap().0;
    assert_eq!(graph.shown_resource_id(key), Some(resource));
    assert!(graph.get_node(key).unwrap().tags.is_empty());
    assert!(app.graph_pane_select_member(pane, first));
    assert!(
        crate::inspector_view::inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: Paper, paper")
    );
}

#[test]
fn resource_content_recycle_capture_keeps_exact_labels_and_live_sibling() {
    let (mut app, first, second, resource) = resource_content_fixture();
    let effects = app.update(Action::DeleteFocusedNode);
    let record = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::RecordDeleted { record } => Some(record),
            _ => None,
        })
        .expect("delete stages recycle record");
    assert_eq!(record.node_id, first);
    assert_eq!(record.url, "https://EXAMPLE.test/page#one");
    let mut labels = record.tags.clone();
    labels.sort();
    assert_eq!(labels, vec!["Paper".to_string(), "paper".to_string()]);

    let graph = app.graph_runtimes.graph();
    assert!(graph.get_node_by_id(first).is_none());
    let sibling_key = graph.get_node_by_id(second).expect("sibling survives").0;
    assert_eq!(graph.shown_resource_id(sibling_key), Some(resource));
    assert_eq!(graph.surface_ids_showing_resource(resource), vec![second]);
    assert_eq!(graph.resource_tag_labels(resource).len(), 2);
    let reloaded = mere::kernel::graph::Graph::try_from_snapshot(&graph.to_snapshot())
        .expect("persisted live sibling association");
    let key = reloaded.get_node_by_id(second).unwrap().0;
    assert_eq!(reloaded.shown_resource_id(key), Some(resource));
    assert_eq!(
        reloaded.surface_ids_showing_resource(resource),
        vec![second]
    );
    assert_eq!(
        reloaded.resource_tag_labels(resource),
        graph.resource_tag_labels(resource)
    );
    let pane = app.default_graph_pane();
    assert!(app.graph_pane_select_member(pane, second));
    assert!(
        crate::inspector_view::inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: Paper, paper")
    );
    // RemovedRecord.tags is Vec<String>: this asserts labels only. It cannot
    // establish concept owners, assertion handles, taggers, or recovery scope.
}

#[test]
fn session_restore_applies_law_before_overlays_and_records_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::test_stub();
    app.data_root = dir.path().to_path_buf();
    let session_id = app.session_id;
    let session_dir = session::session_dir(dir.path(), session_id);
    // The flat view intent this host wrote before the saved scene.
    let write_intent = |law: mere::canvas::PhysicsLaw| {
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session::view_intent_path(&session_dir),
            serde_json::json!({
                "physics_law": law.id(),
                "physics_overlays": [mere::canvas::PhysicsOverlay::GridSnap.id()],
            })
            .to_string(),
        )
        .unwrap();
    };
    write_intent(mere::canvas::PhysicsLaw::Density);
    app.adopt_session(session_id);
    assert_eq!(
        app.graph_runtimes.physics_law(),
        mere::canvas::PhysicsLaw::Density
    );
    assert!(app.graph_runtimes.physics_overlays().is_empty());
    assert_eq!(
        app.physics_refusal().unwrap().refused,
        vec![mere::canvas::PhysicsOverlay::GridSnap]
    );
    assert!(
        app.take_events()
            .iter()
            .any(|event| matches!(event, AppEvent::PhysicsRefused(_)))
    );

    write_intent(mere::canvas::PhysicsLaw::Springs);
    app.adopt_session(session_id);
    assert_eq!(
        app.graph_runtimes.physics_overlays(),
        &[mere::canvas::PhysicsOverlay::GridSnap]
    );
    assert!(app.physics_refusal().is_none());
}

#[test]
fn capture_action_preserves_explicit_member_for_the_host() {
    let mut app = App::test_stub();
    let member = uuid::Uuid::new_v4();
    assert_eq!(
        app.update(Action::CapturePage { member }),
        vec![Effect::CaptureContent { node: member }]
    );
}

#[test]
fn source_capture_refuses_a_member_without_a_current_target() {
    let app = App::test_stub();
    assert!(app.source_capture_target(uuid::Uuid::new_v4()).is_err());
    assert!(
        !app.available_actions()
            .iter()
            .any(|(label, _)| label == "Save source and keep")
    );
}

#[test]
fn source_capture_carries_current_target_without_requesting_png() {
    let mut app = App::test_stub();
    let url = "https://example.test/source";
    let key = app.graph_runtimes.visit(url);
    let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    let request = requested_source_capture(&mut app, node);
    assert_eq!(request.node, node);
    assert_eq!(request.url, url);
    assert_eq!(request.session, app.session_id);
    assert_eq!(request.graph, app.graph_runtimes.active_graph());
}

#[test]
fn source_capture_completion_attaches_an_ordered_node_reference_series() {
    let mut app = App::test_stub();
    let url = "https://example.test/source";
    let key = app.graph_runtimes.visit(url);
    let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    for annotation_manifest in ["annotation-first", "annotation-second"] {
        let request = requested_source_capture(&mut app, node);
        assert_eq!(
            app.apply_update(Update::SourceDocumentCaptured {
                capture_generation: app.source_capture_generation,
                request: request.clone(),
                result: Ok(crate::action::StoredSourceDocument {
                    raw_manifest: format!("raw-{annotation_manifest}"),
                    annotation_manifest: annotation_manifest.into(),
                }),
            }),
            vec![
                Effect::PersistSourceCapture {
                    request: request.id
                },
                Effect::Redraw
            ]
        );
    }
    assert_eq!(
        crate::source_capture::references(app.graph_runtimes.facets(), node),
        vec![
            "annotation-first".to_string(),
            "annotation-second".to_string()
        ]
    );
    assert!(
        app.take_events().iter().all(|event| !matches!(
            event,
            crate::observe::AppEvent::SourceDocumentCaptured { .. }
        )),
        "attachment cannot report final saved success"
    );
}

#[test]
fn stale_or_failed_source_capture_completion_never_attaches_a_reference() {
    let mut app = App::test_stub();
    let url = "https://example.test/source";
    let key = app.graph_runtimes.visit(url);
    let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    let stale = requested_source_capture(&mut app, node);
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: "https://example.test/changed".into(),
    });
    assert_eq!(
        app.apply_update(Update::SourceDocumentCaptured {
            capture_generation: app.source_capture_generation,
            request: stale,
            result: Ok(crate::action::StoredSourceDocument {
                raw_manifest: "raw-stale".into(),
                annotation_manifest: "annotation-stale".into(),
            }),
        }),
        vec![Effect::Redraw]
    );
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::SourceDocumentCaptureFailed { .. }
    )));
    assert!(crate::source_capture::references(app.graph_runtimes.facets(), node).is_empty());
    let failed = requested_source_capture(&mut app, node);
    assert_eq!(
        app.apply_update(Update::SourceDocumentCaptured {
            capture_generation: app.source_capture_generation,
            request: failed,
            result: Err("eidetic write failed".into()),
        }),
        vec![Effect::Redraw]
    );
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::SourceDocumentCaptureFailed { .. }
    )));
    assert!(crate::source_capture::references(app.graph_runtimes.facets(), node).is_empty());
}

#[test]
fn retired_static_viewer_pins_migrate_to_livery() {
    let node = uuid::Uuid::new_v4();
    let mut states = pandect::browser_node_state::BrowserNodeStates::new();
    states.entry(node).viewer_override = Some(inker::routing::ENGINE_GENET_WEB.to_string());

    assert!(migrate_retired_viewer_overrides(&mut states));
    assert_eq!(
        states
            .get(node)
            .and_then(|state| state.viewer_override.as_deref()),
        Some(inker::routing::ENGINE_GENET_LIVERY)
    );
    assert!(!migrate_retired_viewer_overrides(&mut states));
}

#[test]
fn binary_page_response_becomes_durable_download_custody() {
    let mut app = App::test_stub();
    let url = "gemini://capsule.test/archive.bin";
    let key = app.graph_runtimes.visit(url);
    let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    app.content.note_requested(node);
    let request = fetch::next_fetch_request_id();
    app.content.begin_fetch(node, request);
    let _ = app.take_events();

    let bytes = b"\0exact\xffbytes".to_vec();
    let effects = app.apply_update(Update::PageFetched {
        request,
        node,
        url: url.to_string(),
        result: Ok(crate::action::FetchedPage {
            content_type: Some("application/octet-stream".into()),
            content_disposition: Some("attachment; filename=archive.bin".into()),
            bytes: bytes.clone(),
            body: String::from_utf8_lossy(&bytes).into_owned(),
        }),
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::StoreDownload {
            node: actual,
            url: actual_url,
            bytes: actual_bytes,
            ..
        } if *actual == node && actual_url == url && actual_bytes == &bytes
    )));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SpawnContent { .. })),
        "a binary response must not be sent to a document engine"
    );
    let facet = chartulary::FacetId::new(crate::content_classes::DOWNLOAD_FACET);
    let pending = app.graph_runtimes.facets().get(&node, &facet).unwrap();
    assert_eq!(pending["status"], "storing");
    assert_eq!(pending["byte_size"], bytes.len() as u64);

    let content = mere::kernel::graph::ContentHash::of(&bytes);
    let destination = "C:\\Downloads\\archive.bin".to_string();
    let complete = app.apply_update(Update::DownloadStored {
        node,
        url: url.to_string(),
        content_type: Some("application/octet-stream".into()),
        content_disposition: Some("attachment; filename=archive.bin".into()),
        received_at_ms: 42,
        byte_size: bytes.len() as u64,
        result: Ok(crate::action::StoredDownload {
            content,
            destination: destination.clone(),
            byte_size: bytes.len() as u64,
        }),
    });
    assert_eq!(complete, vec![Effect::SaveSession, Effect::Redraw]);
    assert_eq!(
        app.graph_runtimes
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .content,
        Some(content)
    );
    let completed = app.graph_runtimes.facets().get(&node, &facet).unwrap();
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["destination_path"], destination);
    assert_eq!(completed["content_hash"], content.to_hex());
    let events = app
        .take_events()
        .iter()
        .map(AppEvent::describe)
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("download-started"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("download-completed"))
    );
}

#[test]
fn place_binding_survives_switch_and_restart_as_a_worker_open() {
    let root = std::env::temp_dir().join(format!("turnstone-place-adopt-{}", uuid::Uuid::new_v4()));
    let mut app = App::test_stub();
    app.data_root = root.clone();

    let personal = crate::panes::SessionId::new();
    std::fs::create_dir_all(session::session_dir(&root, personal)).unwrap();
    app.adopt_session(personal);
    assert_eq!(app.place, crate::place::PlaceState::Personal);
    assert!(
        !session::place_binding_path(&app.session_dir()).exists(),
        "adopting a personal session does not invent a place sidecar"
    );

    let target = crate::panes::SessionId::new();
    let sdir = session::session_dir(&root, target);
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x33; 32]),
        "commons",
    )
    .unwrap();
    session::save_place_binding(&sdir, &binding).unwrap();
    let key = app.graph_runtimes.visit("https://shared.example");
    let shared_node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    session::save_session_graph(&sdir, app.graph_runtimes.graph());

    let effects = app.adopt_session(target);
    assert!(
        matches!(
            &app.place,
            crate::place::PlaceState::Opening {
                binding: actual,
                generation: 2,
            } if actual == &binding
        ),
        "the app exposes the worker open instead of presenting the cache as current"
    );
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::OpenPlace {
                session,
                generation: 2,
                binding: actual,
            } if *session == target && actual == &binding
        )),
        "adoption asks the shell-owned worker to materialize the retained domains"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_id(shared_node)
            .is_some(),
        "the cached graph remains immediately available"
    );

    // A fresh process cannot share the departed owner's writer lease.
    drop(app);
    let mut reopened = App::test_stub();
    reopened.data_root = root.clone();
    let effects = reopened.adopt_session(target);
    assert!(
        matches!(
            &reopened.place,
            crate::place::PlaceState::Opening {
                binding: actual,
                generation: 1,
            } if actual == &binding
        ),
        "a fresh app state requests the same durable binding"
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::OpenPlace {
            session,
            generation: 1,
            binding: actual,
        } if *session == target && actual == &binding
    )));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stale_place_update_from_a_departed_session_is_ignored() {
    let root = std::env::temp_dir().join(format!("turnstone-place-stale-{}", uuid::Uuid::new_v4()));
    let mut app = App::test_stub();
    app.data_root = root.clone();
    let first = crate::panes::SessionId::new();
    let second = crate::panes::SessionId::new();
    let first_binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x12; 32]),
        crate::place::ChatSpaceId([0x13; 32]),
        "first",
    )
    .unwrap();
    let second_binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x21; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x23; 32]),
        "second",
    )
    .unwrap();
    session::save_place_binding(&session::session_dir(&root, first), &first_binding).unwrap();
    session::save_place_binding(&session::session_dir(&root, second), &second_binding).unwrap();

    app.adopt_session(first);
    app.adopt_session(second);
    assert_eq!(app.session_id, second);
    let stale = crate::place::OfflinePlaceSnapshot {
        graph: crate::place::GraphCache {
            nodes: 99,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(
        app.apply_update(Update::PlaceOpened {
            session: first,
            generation: 1,
            result: Ok(stale),
        })
        .is_empty(),
        "a stale worker answer produces no follow-up effect"
    );
    assert!(
        matches!(
            &app.place,
            crate::place::PlaceState::Opening {
                binding,
                generation: 2,
            } if binding == &second_binding
        ),
        "the stale answer cannot replace the active session's opening"
    );

    let current = crate::place::OfflinePlaceSnapshot {
        graph: crate::place::GraphCache {
            nodes: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        app.apply_update(Update::PlaceOpened {
            session: second,
            generation: 2,
            result: Ok(current.clone()),
        }),
        vec![Effect::Redraw]
    );
    assert_eq!(
        app.place,
        crate::place::PlaceState::Offline {
            binding: second_binding,
            generation: 2,
            snapshot: current,
        }
    );
    let _ = std::fs::remove_dir_all(root);
}
