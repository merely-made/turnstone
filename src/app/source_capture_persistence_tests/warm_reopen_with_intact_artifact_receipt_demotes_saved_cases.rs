// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn warm_reopen_with_intact_artifact_receipt_demotes_saved_when_owning_attachment_is_detached() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    let result = app.persist_source_capture(request.id);
    acknowledge(&mut app, &request, result);
    assert_eq!(saved_events(&mut app), 1);
    overwrite_capture_facet(
        &mut app,
        &request,
        source_capture::SOURCE_CAPTURE_REFERENCES_FACET,
        serde_json::json!([]),
    );
    app.persist_session_graph().unwrap();
    let effects = app.update(Action::NewSession);
    let other = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.adopt_session(other);
    app.adopt_session(request.session);
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Recoverable
    );
    app.take_events();
    // The actor verified its artifact receipt; the app must independently
    // reject the now-missing owning attachment despite prior in-memory Saved.
    app.apply_update(Update::SourceCaptureRevalidated {
        capture_generation: app.source_capture_generation,
        request: request.clone(),
        stored: stored(),
    });
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Recoverable
    );
    assert!(app.source_captures[&request.id].error.is_some());
    assert_eq!(saved_events(&mut app), 0);
    retry(&mut app, &request);
    deposited(&mut app, &request);
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Failed,
        "receipt validity cannot reconstruct a deliberately detached reference"
    );
    assert_eq!(saved_events(&mut app), 0);
    assert!(references(&app, node).is_empty());
}

#[test]
fn warm_reopen_cannot_retain_saved_when_inventory_fails_or_the_store_never_answers() {
    for inventory_fails in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let request = begin(&mut app, node);
        deposited(&mut app, &request);
        let result = app.persist_source_capture(request.id);
        acknowledge(&mut app, &request, result);
        assert_eq!(saved_events(&mut app), 1);
        let effects = app.update(Action::NewSession);
        let other = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::SwitchSession { id } => Some(*id),
                _ => None,
            })
            .unwrap();
        app.adopt_session(other);
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Saved,
            "adopting B does not invalidate A's unrelated inventory"
        );
        app.adopt_session(request.session);
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Recoverable
        );
        app.take_events();
        if inventory_fails {
            app.apply_update(Update::SourceCaptureRecoveryFailed {
                capture_generation: app.source_capture_generation,
                error: "the current session receipt inventory is unreadable".into(),
            });
            assert!(app.take_events().iter().any(|event| matches!(
                event,
                AppEvent::InteractionMissed {
                    what: "recover-source-captures",
                    ..
                }
            )));
        }
        // No inventory event models an unavailable store; neither that absence
        // nor a global inventory error can retain the old Saved qualification.
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Recoverable
        );
        assert!(app.source_captures[&request.id].error.is_some());
        assert_eq!(app.source_captures[&request.id].request, request);
        assert_eq!(
            app.source_captures[&request.id].stored.as_ref(),
            Some(&stored())
        );
        assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
        assert_eq!(saved_events(&mut app), 0);
        app.apply_update(Update::SourceCaptureRevalidated {
            capture_generation: app.source_capture_generation,
            request: request.clone(),
            stored: stored(),
        });
        assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Saved);
        assert!(app.source_captures[&request.id].error.is_none());
        assert_eq!(
            saved_events(&mut app),
            0,
            "revalidation restores qualification without repeating success"
        );
        assert!(
            app.update(Action::RetrySourceDocumentCapture {
                request: request.id
            })
            .is_empty()
        );
    }
}

#[test]
fn session_adoption_requires_explicit_retry_for_unfinished_capture_operations() {
    for pending_save in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let request = begin(&mut app, node);
        if pending_save {
            deposited(&mut app, &request);
        }
        let generation = app.source_capture_generation;
        let original_stored = app.source_captures[&request.id].stored.clone();
        app.adopt_session(request.session);
        assert_ne!(app.source_capture_generation, generation);
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Recoverable
        );
        assert_eq!(app.source_captures[&request.id].request, request);
        assert_eq!(app.source_captures[&request.id].stored, original_stored);
        assert_eq!(saved_events(&mut app), 0);
    }
}

#[test]
fn queued_capture_updates_from_a_prior_opening_cannot_requalify_a_warm_reopened_session() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    let result = app.persist_source_capture(request.id);
    acknowledge(&mut app, &request, result);
    assert_eq!(saved_events(&mut app), 1);
    let old_generation = app.source_capture_generation;
    let effects = app.update(Action::NewSession);
    let other = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.adopt_session(other);
    let middle_generation = app.source_capture_generation;
    app.adopt_session(request.session);
    assert_ne!(app.source_capture_generation, old_generation);
    assert_ne!(app.source_capture_generation, middle_generation);
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Recoverable
    );
    app.take_events();
    let stale = vec![
        Update::SourceCaptureSaved {
            capture_generation: old_generation,
            request: request.id,
            result: Ok(()),
        },
        Update::SourceCaptureRevalidated {
            capture_generation: old_generation,
            request: request.clone(),
            stored: stored(),
        },
        Update::SourceCaptureRecovered {
            capture_generation: old_generation,
            request: request.clone(),
            stored: None,
        },
        Update::SourceCaptureRecoveryFailed {
            capture_generation: old_generation,
            error: "obsolete opening inventory failure".into(),
        },
        Update::SourceDocumentCaptured {
            capture_generation: old_generation,
            request: request.clone(),
            result: Ok(stored()),
        },
    ];
    for update in stale {
        assert!(app.apply_update(update).is_empty());
        assert!(
            app.take_events().is_empty(),
            "obsolete actor updates must not emit current progress or failures"
        );
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Recoverable
        );
        assert_eq!(
            app.source_captures[&request.id].stored.as_ref(),
            Some(&stored())
        );
    }
    app.apply_update(Update::SourceCaptureRevalidated {
        capture_generation: app.source_capture_generation,
        request: request.clone(),
        stored: stored(),
    });
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Saved);
    assert_eq!(
        saved_events(&mut app),
        0,
        "only this opening's verified inventory quietly restores Saved"
    );

    app.adopt_session(request.session);
    retry(&mut app, &request);
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Depositing
    );
    app.take_events();
    assert!(
        app.apply_update(Update::SourceDocumentCaptured {
            capture_generation: old_generation,
            request: request.clone(),
            result: Ok(stored()),
        })
        .is_empty()
    );
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Depositing
    );
    assert!(app.take_events().is_empty());
    deposited(&mut app, &request);
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::PendingSave
    );
    assert!(
        app.apply_update(Update::SourceCaptureSaved {
            capture_generation: old_generation,
            request: request.id,
            result: Ok(()),
        })
        .is_empty()
    );
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::PendingSave
    );
    assert_eq!(saved_events(&mut app), 0);
    let result = app.persist_source_capture(request.id);
    assert!(result.is_ok(), "{result:?}");
    acknowledge(&mut app, &request, result);
    assert_eq!(saved_events(&mut app), 1);
    assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
}

#[test]
fn source_capture_save_process_boundary_worker() {
    let Ok(root) = std::env::var("TURNSTONE_R0_PROCESS_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let stage = std::env::var("TURNSTONE_R0_PROCESS_STAGE").unwrap();
    let handoff = root.join("capture-request.json");
    match stage.as_str() {
        "fail" => {
            let (mut app, node) = fixture(root);
            let request = begin(&mut app, node);
            deposited(&mut app, &request);
            std::fs::write(&handoff, serde_json::to_vec(&request).unwrap()).unwrap();
            let blocked = app
                .session_dir()
                .join(pandect::session_graph_store::GRAPH_FILE)
                .to_path_buf();
            // Model a process exit after the exact candidate sidecar became
            // durable but before the matching graph envelope. The current
            // writer validates the existing graph before starting that pair,
            // so this crash boundary must be seeded explicitly in the fixture.
            let candidate = app.graph_runtimes.facets().clone();
            pandect::save_node_facets(&app.session_dir(), &candidate).unwrap();
            let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
            let result = app.persist_source_capture(request.id);
            assert!(result.is_err());
            acknowledge(&mut app, &request, result);
            assert_eq!(saved_events(&mut app), 0);
            blocked.restore();
        },
        "retry" | "verify" => {
            let request: CaptureRequest =
                serde_json::from_slice(&std::fs::read(&handoff).unwrap()).unwrap();
            let mut app = app_at(root);
            app.sessions
                .load_from_disk(session::sessions_root(root))
                .unwrap();
            app.adopt_session(request.session);
            assert!(!app.session_load_refused());
            assert_eq!(
                references(&app, request.node),
                vec![stored().annotation_manifest]
            );
            assert!(app.node_is_kept(request.node));
            if stage == "retry" {
                app.apply_update(Update::SourceCaptureRecovered {
                    capture_generation: app.source_capture_generation,
                    request: request.clone(),
                    stored: Some(stored()),
                });
                assert_eq!(saved_events(&mut app), 0);
                retry(&mut app, &request);
                deposited(&mut app, &request);
                let result = app.persist_source_capture(request.id);
                assert!(result.is_ok(), "{result:?}");
                acknowledge(&mut app, &request, result);
                assert_eq!(saved_events(&mut app), 1);
                assert_eq!(
                    references(&app, request.node),
                    vec![stored().annotation_manifest]
                );
            }
        },
        other => panic!("unknown process stage {other}"),
    }
}

#[test]
fn source_capture_failure_retry_and_second_reopen_cross_fresh_processes() {
    let root = tempfile::tempdir().unwrap();
    for stage in ["fail", "retry", "verify"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                format!(
                    "{}::source_capture_save_process_boundary_worker",
                    module_path!().split_once("::").unwrap().1
                )
                .as_str(),
                "--nocapture",
            ])
            .env("TURNSTONE_R0_PROCESS_ROOT", root.path())
            .env("TURNSTONE_R0_PROCESS_STAGE", stage)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "process stage {stage} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "the child must actually execute its exact acceptance helper"
        );
    }
}
