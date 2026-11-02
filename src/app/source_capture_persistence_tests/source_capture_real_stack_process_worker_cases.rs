// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Intentionally exits without destructors: the following process must recover
/// Fjall's acknowledged writes without relying on a dropped store or actor
/// lifecycle flush. This is an abrupt process-exit receipt, not power loss.
#[test]
fn source_capture_real_stack_process_worker() {
    let Ok(root) = std::env::var("TURNSTONE_R0_REAL_PROCESS_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let stage = std::env::var("TURNSTONE_R0_REAL_PROCESS_STAGE").unwrap();
    let handoff = root.join("real-capture-handoff.json");
    let (mut app, request, original_stored, document) = if stage == "fail-facets" {
        let (mut app, node) = fixture(root);
        // R1 starts unkept: its new Keep and exact reference must travel
        // together through both real file faults and four abrupt processes.
        let graph = app.graph_runtimes.active_graph();
        assert!(crate::surface_controls::set(
            app.graph_runtimes.canvas_mut(graph).unwrap(),
            node,
            crate::surface_controls::Control::Keep,
            false
        ));
        app.persist_session_graph().unwrap();
        assert!(!app.node_is_kept(node));
        let url = "https://capture-save.example/evidence";
        let body = "<html><main><p>saved source kestrel — Ångström</p></main></html>";
        let document = crate::content::FetchedDocument {
            bytes: body.as_bytes().to_vec(),
            body: body.into(),
            content_type: Some("text/html; charset=utf-8".into()),
            effective_url: Some(url.into()),
            acquired_at_ms: 1_700_000_000_123,
        };
        app.content
            .note_fetched(node, url.into(), document.clone(), document.bytes.len());
        let request = begin(&mut app, node);
        assert!(request.source.is_some());
        (app, request, None, Some(document))
    } else {
        let (request, stored): (CaptureRequest, StoredSourceDocument) =
            serde_json::from_slice(&std::fs::read(&handoff).unwrap()).unwrap();
        let mut app = app_at(root);
        app.sessions
            .load_from_disk(session::sessions_root(root))
            .unwrap();
        app.adopt_session(request.session);
        assert!(!app.session_load_refused());
        assert_eq!(
            app.node_is_kept(request.node),
            stage != "fail-graph",
            "Keep and the reference commit in the same required facet write"
        );
        assert_eq!(
            references(&app, request.node).len(),
            usize::from(stage != "fail-graph")
        );
        assert!(
            app.content
                .source_capture_candidate(request.node, &request.url)
                .is_none(),
            "the fresh process has no fetched response and must replay local evidence"
        );
        (app, request, Some(stored), None)
    };
    let directory = crate::trail_memory::memory_dir(&app.session_dir());
    let wake: armillary::Wake = std::sync::Arc::new(|| {});
    let (actor, updates) = crate::trail_memory::spawn_trail_for_session(
        wake,
        Some(directory.clone()),
        Default::default(),
        app.source_capture_generation,
    );
    assert!(
        actor.command(crate::trail_memory::TrailCommand::SetCustody(Some(
            crate::trail_memory::custody_test_fixture::custody()
        )))
    );
    let (ready, ready_rx) = std::sync::mpsc::sync_channel(1);
    assert!(actor.command(crate::trail_memory::TrailCommand::CustodyReady(ready)));
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("test custody must be installed before recovery or capture");
    if stage == "verify-reopen" {
        let expected = original_stored.as_ref().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let update = updates
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .expect("completed receipt must revalidate in a fresh owner");
            match update {
                Update::SourceCaptureRevalidated {
                    capture_generation,
                    request: revalidated,
                    stored,
                } => {
                    assert_eq!(revalidated, request);
                    assert_eq!(capture_generation, app.source_capture_generation);
                    assert_eq!(&stored, expected);
                    app.apply_update(Update::SourceCaptureRevalidated {
                        capture_generation,
                        request: revalidated,
                        stored,
                    });
                    break;
                },
                Update::SourceCaptureRecovered { .. } => {
                    panic!("an acknowledged receipt must not reopen as retryable")
                },
                Update::SourceCaptureRecoveryFailed { error, .. } => {
                    panic!("completed receipt integrity: {error}")
                },
                other => {
                    app.apply_update(other);
                },
            }
        }
        assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Saved);
        assert_eq!(
            saved_events(&mut app),
            0,
            "revalidation must not repeat the original Saved event"
        );
        assert!(
            app.update(Action::RetrySourceDocumentCapture {
                request: request.id
            })
            .is_empty(),
            "a completed receipt must not offer an unnecessary recovery retry"
        );
        assert_eq!(
            references(&app, request.node),
            vec![expected.annotation_manifest.clone()]
        );
        assert_eq!(
            source_capture::attached(app.graph_runtimes.facets(), &request),
            Some(expected.clone())
        );
        finish_real_process_stage(&stage);
    }
    if let Some(expected) = &original_stored {
        receive_real_recovery(&mut app, &updates, &request, expected);
        retry(&mut app, &request);
    }
    assert!(
        actor.command(crate::trail_memory::TrailCommand::CaptureSourceDocument {
            capture_generation: app.source_capture_generation,
            request: request.clone(),
            directory: directory.clone(),
            document,
        })
    );
    let stored = receive_real_deposit(&mut app, &updates, &request);
    if let Some(expected) = original_stored {
        assert_eq!(stored, expected);
    }
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::PendingSave
    );
    assert_eq!(saved_events(&mut app), 0);
    assert_eq!(
        references(&app, request.node),
        vec![stored.annotation_manifest.clone()]
    );
    assert!(app.node_is_kept(request.node));
    assert_eq!(
        source_capture::attached(app.graph_runtimes.facets(), &request),
        Some(stored.clone())
    );
    if stage == "fail-facets" {
        std::fs::write(
            &handoff,
            serde_json::to_vec(&(request.clone(), stored.clone())).unwrap(),
        )
        .unwrap();
    }
    match stage.as_str() {
        "fail-facets" | "fail-graph" => {
            let graph = app
                .session_dir()
                .join(pandect::session_graph_store::GRAPH_FILE);
            let graph_before = std::fs::read(&graph).unwrap();
            let target = if stage == "fail-facets" {
                pandect::node_facets_path(&app.session_dir())
            } else {
                // The following fresh process must recover the exact sidecar
                // that would have committed before a graph replacement was
                // interrupted. Seed that post-facet/pre-graph boundary here;
                // the writer now rejects a blocked graph before starting the
                // pair and cannot itself create this state under this fault.
                let candidate = app.graph_runtimes.facets().clone();
                pandect::save_node_facets(&app.session_dir(), &candidate).unwrap();
                graph.clone()
            };
            let blocked = target.to_path_buf();
            let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
            let result = app.persist_source_capture(request.id);
            assert!(
                result.is_err(),
                "actual required write must fail at {stage}"
            );
            acknowledge(&mut app, &request, result.map_err(|e| e.to_string()));
            assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
            assert_eq!(saved_events(&mut app), 0);
            assert_eq!(blocked.read_preserved(&graph).unwrap(), graph_before);
            blocked.restore();
        },
        "retry-save" => {
            let result = app.persist_source_capture(request.id);
            assert!(result.is_ok(), "real receipt retry save: {result:?}");
            assert_eq!(saved_events(&mut app), 0);
            assert!(actor.command(
                crate::trail_memory::TrailCommand::AcknowledgeSourceCapture {
                    capture_generation: app.source_capture_generation,
                    request: request.clone(),
                    directory,
                }
            ));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                let update = updates
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .expect("trail owner must persist and acknowledge completed save receipt");
                match update {
                    Update::SourceCaptureSaved {
                        capture_generation,
                        request: id,
                        result,
                    } => {
                        assert_eq!(id, request.id);
                        assert_eq!(capture_generation, app.source_capture_generation);
                        assert!(result.is_ok(), "receipt acknowledgement: {result:?}");
                        acknowledge(&mut app, &request, result.map_err(|e| e.to_string()));
                        break;
                    },
                    other => {
                        app.apply_update(other);
                    },
                }
            }
            assert_eq!(saved_events(&mut app), 1);
            acknowledge(&mut app, &request, Ok(()));
            assert_eq!(saved_events(&mut app), 0);
            assert_eq!(
                references(&app, request.node),
                vec![stored.annotation_manifest]
            );
        },
        other => panic!("unexpected real process stage {other}"),
    }
    finish_real_process_stage(&stage);
}

#[test]
fn real_fjall_capture_failure_retry_and_reopen_cross_abrupt_process_exits() {
    let root = tempfile::tempdir().unwrap();
    for stage in ["fail-facets", "fail-graph", "retry-save", "verify-reopen"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                format!(
                    "{}::source_capture_real_stack_process_worker",
                    module_path!().split_once("::").unwrap().1
                )
                .as_str(),
                "--nocapture",
            ])
            .env("TURNSTONE_R0_REAL_PROCESS_ROOT", root.path())
            .env("TURNSTONE_R0_REAL_PROCESS_STAGE", stage)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "real stack stage {stage} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(&format!("TURNSTONE_R0_REAL_STAGE_PASSED:{stage}")),
            "the child must execute all real-stack assertions before abrupt exit"
        );
    }
}
