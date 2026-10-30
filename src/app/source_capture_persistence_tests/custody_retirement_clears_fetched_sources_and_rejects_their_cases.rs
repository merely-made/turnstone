// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn custody_retirement_clears_fetched_sources_and_rejects_their_late_answer() {
    let mut app = App::test_stub();
    let node = uuid::Uuid::new_v4();
    let old_generation = app.source_capture_generation;
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([1; 32]),
        crate::place::SharedContainerId([2; 32]),
        crate::place::ChatSpaceId([3; 32]),
        "hall",
    )
    .unwrap();
    let private_group = crate::place::CapturedContentGroup {
        extraction_schema: "fixture-schema".into(),
        normalization: "fixture-normalization".into(),
        reader_profile: "fixture-reader".into(),
        canonical_text_hash: "fixture-hash".into(),
        canonical_text_iri: "urn:fixture:private-capture".into(),
        canonical_text: "private capture canary".into(),
        contributions: Vec::new(),
    };
    let mut snapshot = crate::place::OfflinePlaceSnapshot::default();
    snapshot.captured.groups.push(private_group);
    app.place = crate::place::PlaceState::Offline {
        binding,
        generation: 3,
        snapshot,
    };
    app.content.begin_fetch(node, 41);
    app.content.note_fetched(
        node,
        "https://private.example/source".into(),
        crate::content::FetchedDocument {
            bytes: b"private capture canary".to_vec(),
            content_type: Some("text/html".into()),
            body: "private capture canary".into(),
            effective_url: Some("https://private.example/source".into()),
            acquired_at_ms: 7,
        },
        22,
    );
    assert!(
        app.content
            .source_capture_candidate(node, "https://private.example/source")
            .is_some()
    );

    app.retire_private_custody();

    assert_ne!(app.source_capture_generation, old_generation);
    let crate::place::PlaceState::Offline { snapshot, .. } = &app.place else {
        panic!("App custody retirement leaves the host place lifecycle to Shell");
    };
    assert_eq!(snapshot.captured, Default::default());
    assert!(
        app.content
            .source_capture_candidate(node, "https://private.example/source")
            .is_none()
    );
    assert!(!app.content.finish_fetch(node, 41));
}

#[test]
fn source_capture_success_waits_for_owning_save_and_duplicate_completions_are_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Depositing
    );
    let effects = deposited(&mut app, &request);
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::PersistSourceCapture { request: id } if *id == request.id
    )));
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::PendingSave
    );
    assert_eq!(
        saved_events(&mut app),
        0,
        "artifact custody is not a save acknowledgement"
    );
    deposited(&mut app, &request);
    assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
    assert_eq!(saved_events(&mut app), 0);
    let result = app.persist_source_capture(request.id);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        saved_events(&mut app),
        0,
        "the reducer must receive the correlated result"
    );
    acknowledge(&mut app, &request, result);
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Saved);
    assert_eq!(saved_events(&mut app), 1);
    acknowledge(&mut app, &request, Ok(()));
    deposited(&mut app, &request);
    assert_eq!(
        saved_events(&mut app),
        0,
        "one completion emits one saved event"
    );
    assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
    assert!(
        app.node_is_kept(node),
        "pre-existing Keep survives the save protocol"
    );
}

#[test]
fn deliberate_identical_capture_requests_remain_distinct_ordered_occurrences() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let first = begin(&mut app, node);
    deposited(&mut app, &first);
    let result = app.persist_source_capture(first.id);
    acknowledge(&mut app, &first, result);
    let second = begin(&mut app, node);
    assert_ne!(first.id, second.id);
    deposited(&mut app, &second);
    let result = app.persist_source_capture(second.id);
    acknowledge(&mut app, &second, result);
    assert_eq!(
        references(&app, node),
        vec![stored().annotation_manifest, stored().annotation_manifest,],
        "content identity must not deduplicate deliberate capture gestures"
    );
    assert_eq!(saved_events(&mut app), 2);
}

#[test]
fn required_file_write_failure_is_attributable_and_retry_preserves_previous_evidence() {
    for fail_facets in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let previous = begin(&mut app, node);
        deposited(&mut app, &previous);
        let result = app.persist_source_capture(previous.id);
        acknowledge(&mut app, &previous, result);
        assert_eq!(saved_events(&mut app), 1);
        let directory = app.session_dir();
        let graph_path = directory.join(pandect::session_graph_store::GRAPH_FILE);
        let graph_before = std::fs::read(&graph_path).unwrap();
        let facets_path = pandect::node_facets_path(&directory);
        let facets_before = std::fs::read(&facets_path).unwrap();
        let request = begin(&mut app, node);
        deposited(&mut app, &request);
        let blocked = if fail_facets {
            &facets_path
        } else {
            &graph_path
        }
        .to_path_buf();
        let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
        let result = app.persist_source_capture(request.id);
        assert!(result.is_err(), "the actual required file write must fail");
        acknowledge(&mut app, &request, result);
        assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
        assert!(app.source_captures[&request.id].error.is_some());
        assert_eq!(saved_events(&mut app), 0);
        assert_eq!(blocked.read_preserved(&graph_path).unwrap(), graph_before);
        if fail_facets {
            assert_eq!(blocked.read_preserved(&facets_path).unwrap(), facets_before);
        }
        assert_eq!(
            app.source_captures[&request.id].stored.as_ref().unwrap(),
            &stored()
        );
        blocked.restore();
        retry(&mut app, &request);
        deposited(&mut app, &request);
        let result = app.persist_source_capture(request.id);
        assert!(result.is_ok(), "{result:?}");
        acknowledge(&mut app, &request, result);
        assert_eq!(saved_events(&mut app), 1);
        assert_eq!(
            references(&app, node),
            vec![stored().annotation_manifest, stored().annotation_manifest,],
            "the previous saved gesture survives; the retried gesture appears once"
        );
        assert!(app.node_is_kept(node));
    }
}

#[test]
fn recovered_receipt_retries_partial_facet_graph_write_without_duplicate_attachment() {
    for fail_facets in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let request = begin(&mut app, node);
        deposited(&mut app, &request);
        let directory = app.session_dir();
        let blocked = if fail_facets {
            pandect::node_facets_path(&directory)
        } else {
            // Exercise the real interruption boundary directly: the exact
            // candidate facet sidecar has reached disk, but its matching graph
            // envelope has not. Current admission validates the existing graph
            // before writing facets, so a blocked graph target must not be used
            // to pretend the writer can newly produce this partial state.
            let candidate = app.graph_runtimes.facets().clone();
            pandect::save_node_facets(&directory, &candidate).unwrap();
            directory.join(pandect::session_graph_store::GRAPH_FILE)
        }
        .to_path_buf();
        let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
        let result = app.persist_source_capture(request.id);
        assert!(result.is_err());
        acknowledge(&mut app, &request, result);
        assert_eq!(saved_events(&mut app), 0);
        drop(app);
        blocked.restore();
        let mut reopened = app_at(root.path());
        reopened
            .sessions
            .load_from_disk(session::sessions_root(root.path()))
            .unwrap();
        reopened.adopt_session(request.session);
        assert!(!reopened.session_load_refused());
        assert_eq!(
            references(&reopened, node).len(),
            usize::from(!fail_facets),
            "a graph-write failure can leave the new facet committed"
        );
        reopened.apply_update(Update::SourceCaptureRecovered {
            capture_generation: reopened.source_capture_generation,
            request: request.clone(),
            stored: Some(stored()),
        });
        assert_eq!(
            reopened.source_captures[&request.id].phase,
            CapturePhase::Recoverable
        );
        assert_eq!(
            saved_events(&mut reopened),
            0,
            "receipt recovery is not final success"
        );
        retry(&mut reopened, &request);
        deposited(&mut reopened, &request);
        let result = reopened.persist_source_capture(request.id);
        assert!(result.is_ok(), "{result:?}");
        acknowledge(&mut reopened, &request, result);
        assert_eq!(saved_events(&mut reopened), 1);
        assert_eq!(
            references(&reopened, node),
            vec![stored().annotation_manifest]
        );
        assert!(reopened.node_is_kept(node));
    }
}

#[test]
fn stale_target_or_wrong_session_never_attaches_or_acknowledges_saved() {
    for wrong_session in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let request = begin(&mut app, node);
        if wrong_session {
            let effects = app.update(Action::NewSession);
            let id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::SwitchSession { id } => Some(*id),
                    _ => None,
                })
                .unwrap();
            app.adopt_session(id);
        } else {
            app.update(Action::ContentNavigationCommitted {
                member: node,
                url: "https://capture-save.example/replaced".into(),
            });
        }
        deposited(&mut app, &request);
        assert_ne!(app.source_captures[&request.id].phase, CapturePhase::Saved);
        assert!(references(&app, node).is_empty());
        assert!(app.persist_source_capture(request.id).is_err());
        acknowledge(&mut app, &request, Ok(()));
        assert_eq!(
            saved_events(&mut app),
            0,
            "unmatched success cannot admit an invalid owner"
        );
    }
}

#[test]
fn unsolicited_or_substituted_deposit_and_save_completions_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    let mut unsolicited = request.clone();
    unsolicited.id = uuid::Uuid::new_v4();
    deposited(&mut app, &unsolicited);
    acknowledge(&mut app, &unsolicited, Ok(()));
    let mut substituted = request.clone();
    substituted.url = "https://capture-save.example/substitution".into();
    deposited(&mut app, &substituted);
    acknowledge(&mut app, &request, Ok(()));
    assert!(references(&app, node).is_empty());
    assert_eq!(saved_events(&mut app), 0);
}

#[test]
fn replaced_response_at_same_address_cannot_complete_an_earlier_request() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let url = "https://capture-save.example/evidence";
    let document = crate::content::FetchedDocument {
        bytes: b"<p>first response</p>".to_vec(),
        body: "<p>first response</p>".into(),
        content_type: Some("text/html".into()),
        effective_url: Some(url.into()),
        acquired_at_ms: 7,
    };
    app.content
        .note_fetched(node, url.into(), document.clone(), 7);
    let request = begin(&mut app, node);
    assert!(request.source.is_some());
    app.content.note_fetched(
        node,
        url.into(),
        crate::content::FetchedDocument {
            acquired_at_ms: 8,
            ..document
        },
        8,
    );
    deposited(&mut app, &request);
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert!(references(&app, node).is_empty());
    assert_eq!(saved_events(&mut app), 0);
}

#[test]
fn disappeared_live_response_is_refused_but_verified_recovered_receipt_can_retry_offline() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let url = "https://capture-save.example/evidence";
    app.content.note_fetched(
        node,
        url.into(),
        crate::content::FetchedDocument {
            bytes: b"<p>retained source</p>".to_vec(),
            body: "<p>retained source</p>".into(),
            content_type: Some("text/html".into()),
            effective_url: Some(url.into()),
            acquired_at_ms: 7,
        },
        7,
    );
    let request = begin(&mut app, node);
    assert!(request.source.is_some());
    app.content.forget_fetched(node);
    deposited(&mut app, &request);
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert!(references(&app, node).is_empty());
    assert_eq!(saved_events(&mut app), 0);
    drop(app);

    let mut reopened = app_at(root.path());
    reopened
        .sessions
        .load_from_disk(session::sessions_root(root.path()))
        .unwrap();
    reopened.adopt_session(request.session);
    reopened.apply_update(Update::SourceCaptureRecovered {
        capture_generation: reopened.source_capture_generation,
        request: request.clone(),
        stored: Some(stored()),
    });
    assert!(
        reopened
            .content
            .source_capture_candidate(node, url)
            .is_none()
    );
    retry(&mut reopened, &request);
    // This update models the trail owner's verified receipt replay, not a
    // replacement network response. Actual Fjall verification has its own tests.
    deposited(&mut reopened, &request);
    let result = reopened.persist_source_capture(request.id);
    assert!(result.is_ok(), "{result:?}");
    acknowledge(&mut reopened, &request, result);
    assert_eq!(saved_events(&mut reopened), 1);
    assert_eq!(
        references(&reopened, node),
        vec![stored().annotation_manifest]
    );
}

#[test]
fn receipt_acknowledgement_failure_can_retry_same_saved_attachment_without_live_response() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let url = "https://capture-save.example/evidence";
    app.content.note_fetched(
        node,
        url.into(),
        crate::content::FetchedDocument {
            bytes: b"<p>acknowledgement recovery</p>".to_vec(),
            body: "<p>acknowledgement recovery</p>".into(),
            content_type: Some("text/html".into()),
            effective_url: Some(url.into()),
            acquired_at_ms: 17,
        },
        17,
    );
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    app.persist_source_capture(request.id).unwrap();
    acknowledge(
        &mut app,
        &request,
        Err("completed receipt acknowledgement failed".into()),
    );
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert_eq!(saved_events(&mut app), 0);
    app.content.forget_fetched(node);
    retry(&mut app, &request);
    deposited(&mut app, &request);
    let result = app.persist_source_capture(request.id);
    assert!(result.is_ok(), "{result:?}");
    acknowledge(&mut app, &request, result);
    assert_eq!(saved_events(&mut app), 1);
    assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
    assert!(app.node_is_kept(node));
}

#[test]
fn missing_sole_reference_refuses_save_retry_and_late_ack_without_repairing_the_ledger() {
    for detach_after_file_save in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let request = begin(&mut app, node);
        deposited(&mut app, &request);
        let ledger = request_ledger(&app, &request);
        if detach_after_file_save {
            app.persist_source_capture(request.id).unwrap();
        }
        overwrite_capture_facet(
            &mut app,
            &request,
            source_capture::SOURCE_CAPTURE_REFERENCES_FACET,
            serde_json::json!([]),
        );
        assert!(source_capture::attached(app.graph_runtimes.facets(), &request).is_none());
        let result = if detach_after_file_save {
            Ok(())
        } else {
            app.persist_source_capture(request.id)
        };
        if !detach_after_file_save {
            assert!(result.is_err());
        }
        acknowledge(&mut app, &request, result);
        assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
        assert_eq!(saved_events(&mut app), 0);
        retry(&mut app, &request);
        deposited(&mut app, &request);
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Failed,
            "retry must refuse an inconsistent ledger instead of inventing a missing occurrence"
        );
        assert_eq!(saved_events(&mut app), 0);
        assert_eq!(request_ledger(&app, &request), ledger);
        assert!(references(&app, node).is_empty());
    }
}
