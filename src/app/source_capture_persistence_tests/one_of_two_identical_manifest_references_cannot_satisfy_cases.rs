// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn one_of_two_identical_manifest_references_cannot_satisfy_both_request_occurrences() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let first = begin(&mut app, node);
    deposited(&mut app, &first);
    let result = app.persist_source_capture(first.id);
    acknowledge(&mut app, &first, result);
    assert_eq!(saved_events(&mut app), 1);
    let second = begin(&mut app, node);
    deposited(&mut app, &second);
    let ledger = request_ledger(&app, &second);
    overwrite_capture_facet(
        &mut app,
        &second,
        source_capture::SOURCE_CAPTURE_REFERENCES_FACET,
        serde_json::json!([stored().annotation_manifest]),
    );
    assert!(source_capture::attached(app.graph_runtimes.facets(), &first).is_none());
    assert!(source_capture::attached(app.graph_runtimes.facets(), &second).is_none());
    let result = app.persist_source_capture(second.id);
    assert!(result.is_err());
    acknowledge(&mut app, &second, result);
    retry(&mut app, &second);
    deposited(&mut app, &second);
    assert_eq!(app.source_captures[&second.id].phase, CapturePhase::Failed);
    assert_eq!(saved_events(&mut app), 0);
    assert_eq!(request_ledger(&app, &second), ledger);
    assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
}

#[test]
fn detached_request_ledger_refuses_an_otherwise_successful_late_acknowledgement() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    app.persist_source_capture(request.id).unwrap();
    overwrite_capture_facet(
        &mut app,
        &request,
        "capture.source-requests/v1",
        serde_json::json!([]),
    );
    acknowledge(&mut app, &request, Ok(()));
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert_eq!(saved_events(&mut app), 0);
    assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
    assert_eq!(request_ledger(&app, &request), serde_json::json!([]));
}

#[test]
fn navigation_after_required_save_preserves_historical_capture_acknowledgement_and_revalidation() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    app.persist_source_capture(request.id).unwrap();
    let current_url = "https://capture-save.example/later-navigation";
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: current_url.into(),
    });
    acknowledge(&mut app, &request, Ok(()));
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Saved,
        "acknowledgement names the saved historical attachment, not the current URL"
    );
    assert_eq!(saved_events(&mut app), 1);
    app.persist_session_graph().unwrap();
    drop(app);
    let mut reopened = app_at(root.path());
    reopened
        .sessions
        .load_from_disk(session::sessions_root(root.path()))
        .unwrap();
    reopened.adopt_session(request.session);
    assert_eq!(
        reopened
            .graph_runtimes
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .url(),
        current_url
    );
    reopened.apply_update(Update::SourceCaptureRevalidated {
        capture_generation: reopened.source_capture_generation,
        request: request.clone(),
        stored: stored(),
    });
    assert_eq!(
        reopened.source_captures[&request.id].phase,
        CapturePhase::Saved,
        "verified saved history remains saved after ordinary member navigation"
    );
    assert_eq!(saved_events(&mut reopened), 0);
    assert!(
        reopened
            .update(Action::RetrySourceDocumentCapture {
                request: request.id
            })
            .is_empty()
    );
    assert_eq!(
        references(&reopened, node),
        vec![stored().annotation_manifest]
    );
    assert!(reopened.node_is_kept(node));
}

#[test]
fn failed_receipt_acknowledgement_reopens_and_retries_historical_attachment_after_navigation() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    app.persist_source_capture(request.id).unwrap();
    acknowledge(
        &mut app,
        &request,
        Err("completed receipt write failed before commit".into()),
    );
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert_eq!(saved_events(&mut app), 0);
    let current_url = "https://capture-save.example/navigated-before-recovery";
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: current_url.into(),
    });
    app.persist_session_graph().unwrap();
    drop(app);

    let mut reopened = app_at(root.path());
    reopened
        .sessions
        .load_from_disk(session::sessions_root(root.path()))
        .unwrap();
    reopened.adopt_session(request.session);
    assert_eq!(
        reopened
            .graph_runtimes
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .url(),
        current_url
    );
    assert_eq!(
        source_capture::attached(reopened.graph_runtimes.facets(), &request),
        Some(stored())
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
    assert_eq!(saved_events(&mut reopened), 0);
    retry(&mut reopened, &request);
    deposited(&mut reopened, &request);
    let result = reopened.persist_source_capture(request.id);
    assert!(
        result.is_ok(),
        "retry acknowledges the existing historical attachment: {result:?}"
    );
    acknowledge(&mut reopened, &request, result);
    assert_eq!(
        reopened.source_captures[&request.id].phase,
        CapturePhase::Saved
    );
    assert_eq!(saved_events(&mut reopened), 1);
    assert_eq!(reopened.source_captures[&request.id].request, request);
    assert_eq!(
        reopened.source_captures[&request.id].stored.as_ref(),
        Some(&stored())
    );
    assert_eq!(
        references(&reopened, node),
        vec![stored().annotation_manifest]
    );
    assert_eq!(
        reopened
            .graph_runtimes
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .url(),
        current_url
    );
    assert!(reopened.node_is_kept(node));
    acknowledge(&mut reopened, &request, Ok(()));
    assert_eq!(saved_events(&mut reopened), 0);
}

#[test]
fn session_fork_carries_evidence_and_foreign_facets_without_copying_capture_operation_admission() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, donor_node) = fixture(root.path());
    let donor_request = begin(&mut app, donor_node);
    let foreign_facet = "test.research-note/v1";
    let foreign_value =
        serde_json::json!({"title": "independent note", "labels": ["kestrel", "Ångström"]});
    overwrite_capture_facet(
        &mut app,
        &donor_request,
        foreign_facet,
        foreign_value.clone(),
    );
    deposited(&mut app, &donor_request);
    let result = app.persist_source_capture(donor_request.id);
    acknowledge(&mut app, &donor_request, result);
    assert_eq!(saved_events(&mut app), 1);
    let donor_ledger = request_ledger(&app, &donor_request);
    let effects = app.fork_session_from(donor_node);
    let child_session = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(*id),
            _ => None,
        })
        .expect("ordinary fork publishes its child and requests adoption");
    assert_ne!(child_session, donor_request.session);
    assert_eq!(
        request_ledger(&app, &donor_request),
        donor_ledger,
        "forking must not remove the donor's operation admission"
    );
    let child_directory = session::session_dir(root.path(), child_session);
    let child = session::try_load_session_graph_locked(&child_directory)
        .unwrap()
        .unwrap()
        .graph;
    let child_node = child.get_node_by_url(&donor_request.url).unwrap().1.id;
    assert_ne!(child_node, donor_node);
    assert_eq!(
        source_capture::references(child.facets(), child_node),
        vec![stored().annotation_manifest]
    );
    assert!(
        child
            .facets()
            .get(
                &child_node,
                &chartulary::FacetId::new("capture.source-requests/v1")
            )
            .is_none(),
        "copied evidence is not admission of the donor's request into the new session"
    );
    assert_eq!(
        child
            .facets()
            .get(&child_node, &chartulary::FacetId::new(foreign_facet)),
        Some(&foreign_value)
    );

    app.adopt_session(child_session);
    assert!(
        app.source_captures
            .values()
            .all(|attempt| attempt.request.session != child_session),
        "fork admission must not invent a child Saved receipt"
    );
    let child_request = begin(&mut app, child_node);
    assert_eq!(child_request.session, child_session);
    assert_ne!(child_request.id, donor_request.id);
    assert_ne!(child_request.graph, donor_request.graph);
    deposited(&mut app, &child_request);
    let result = app.persist_source_capture(child_request.id);
    assert!(
        result.is_ok(),
        "the child's first deliberate capture must admit cleanly: {result:?}"
    );
    acknowledge(&mut app, &child_request, result);
    assert_eq!(saved_events(&mut app), 1);
    assert_eq!(
        app.source_captures[&child_request.id].phase,
        CapturePhase::Saved
    );
    assert_eq!(
        references(&app, child_node),
        vec![stored().annotation_manifest, stored().annotation_manifest]
    );
    assert_eq!(
        source_capture::attached(app.graph_runtimes.facets(), &child_request),
        Some(stored())
    );
    assert_eq!(
        app.graph_runtimes
            .facets()
            .get(&child_node, &chartulary::FacetId::new(foreign_facet)),
        Some(&foreign_value)
    );

    let donor = session::try_load_session_graph_locked(&session::session_dir(
        root.path(),
        donor_request.session,
    ))
    .unwrap()
    .unwrap()
    .graph;
    assert_eq!(
        source_capture::references(donor.facets(), donor_node),
        vec![stored().annotation_manifest]
    );
    assert_eq!(
        donor.facets().get(
            &donor_node,
            &chartulary::FacetId::new("capture.source-requests/v1")
        ),
        Some(&donor_ledger)
    );
    assert_eq!(
        donor
            .facets()
            .get(&donor_node, &chartulary::FacetId::new(foreign_facet)),
        Some(&foreign_value)
    );
}

#[test]
fn warm_session_reopen_demotes_failed_fresh_verification_and_preserves_original_retry_identity() {
    for recovered_stored in [Some(stored()), None] {
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
        assert_ne!(app.session_id, request.session);
        app.adopt_session(request.session);
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Recoverable,
            "warm A-to-B-to-A adoption must await the current actor's verification"
        );
        app.take_events();
        // The current trail owner emits this when its saved receipt fails
        // verification or its recovered receipt is no longer complete.
        app.apply_update(Update::SourceCaptureRecovered {
            capture_generation: app.source_capture_generation,
            request: request.clone(),
            stored: recovered_stored,
        });
        assert_eq!(
            app.source_captures[&request.id].phase,
            CapturePhase::Recoverable
        );
        assert!(
            app.source_captures[&request.id]
                .error
                .as_ref()
                .unwrap()
                .contains("fresh session recovery")
        );
        assert_eq!(
            app.source_captures[&request.id].stored.as_ref(),
            Some(&stored()),
            "fresh inventory must not discard the originally admitted evidence identities"
        );
        assert_eq!(saved_events(&mut app), 0);
        assert!(
            app.available_actions()
                .iter()
                .any(|(_, action)| matches!(action,
                    Action::RetrySourceDocumentCapture { request: id } if *id == request.id
                ))
        );
        retry(&mut app, &request);
        deposited(&mut app, &request);
        let result = app.persist_source_capture(request.id);
        assert!(result.is_ok(), "{result:?}");
        acknowledge(&mut app, &request, result);
        assert_eq!(saved_events(&mut app), 1);
        assert_eq!(references(&app, node), vec![stored().annotation_manifest]);
        assert_eq!(app.source_captures[&request.id].request, request);
    }
}

#[test]
fn recovery_inventory_cannot_override_in_flight_or_differently_bound_requests() {
    for pending_save in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut app, node) = fixture(root.path());
        let request = begin(&mut app, node);
        if pending_save {
            deposited(&mut app, &request);
        }
        let phase = app.source_captures[&request.id].phase;
        app.apply_update(Update::SourceCaptureRecovered {
            capture_generation: app.source_capture_generation,
            request: request.clone(),
            stored: Some(stored()),
        });
        app.apply_update(Update::SourceCaptureRevalidated {
            capture_generation: app.source_capture_generation,
            request: request.clone(),
            stored: stored(),
        });
        assert_eq!(
            app.source_captures[&request.id].phase, phase,
            "stale recovery inventory must not supersede a live operation"
        );
        assert_eq!(saved_events(&mut app), 0);
    }
    let root = tempfile::tempdir().unwrap();
    let (mut app, node) = fixture(root.path());
    let request = begin(&mut app, node);
    deposited(&mut app, &request);
    let result = app.persist_source_capture(request.id);
    acknowledge(&mut app, &request, result);
    assert_eq!(saved_events(&mut app), 1);
    let mut different = request.clone();
    different.url = "https://capture-save.example/different-request".into();
    app.apply_update(Update::SourceCaptureRecovered {
        capture_generation: app.source_capture_generation,
        request: different.clone(),
        stored: None,
    });
    app.apply_update(Update::SourceCaptureRevalidated {
        capture_generation: app.source_capture_generation,
        request: different,
        stored: stored(),
    });
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Saved);
    assert_eq!(app.source_captures[&request.id].request, request);
    assert_eq!(
        app.source_captures[&request.id].stored.as_ref(),
        Some(&stored())
    );
    assert_eq!(saved_events(&mut app), 0);
}
