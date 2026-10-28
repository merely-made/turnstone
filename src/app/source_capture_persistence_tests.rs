// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Request-correlated save controls over real session files. The artifact IDs
//! here are fixtures; trail-memory tests independently prove Fjall custody and
//! verified artifact resolution. These tests claim process restart, not power
//! loss survival.

use super::App;
use crate::action::{Action, Effect, StoredSourceDocument, Update};
use crate::observe::AppEvent;
use crate::source_capture::{CapturePhase, CaptureRequest};
use crate::{session, source_capture};
use std::path::Path;

fn app_at(root: &Path) -> App {
    let mut app = App::test_stub_at(root.to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(root));
    app
}

fn fixture(root: &Path) -> (App, uuid::Uuid) {
    let mut app = app_at(root);
    let effects = app.update(Action::NewSession);
    let id = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(*id),
            _ => None,
        })
        .expect("ordinary session creation publishes its manifest");
    app.adopt_session(id);
    let key = app
        .graph_runtimes
        .visit("https://capture-save.example/evidence");
    let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    app.update(Action::KeepNode { member: node });
    app.persist_session_graph().unwrap();
    app.take_events();
    (app, node)
}

fn begin(app: &mut App, node: uuid::Uuid) -> CaptureRequest {
    if app.source_capture_target(node).is_err() {
        let url = app
            .graph_runtimes
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .url()
            .to_owned();
        let body = "<html><main>capture fixture</main></html>";
        app.content.note_fetched(
            node,
            url.clone(),
            crate::content::FetchedDocument {
                bytes: body.as_bytes().to_vec(),
                body: body.into(),
                content_type: Some("text/html".into()),
                effective_url: Some(url),
                acquired_at_ms: 1,
            },
            body.len(),
        );
    }
    let target = app
        .source_capture_target(node)
        .expect("complete fixture source");
    let effects = app.update(Action::CaptureSourceDocument { target });
    effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::CaptureSourceDocument { request, .. } => Some(request),
            _ => None,
        })
        .expect("a current member yields a correlated deposit request")
}

fn stored() -> StoredSourceDocument {
    StoredSourceDocument {
        raw_manifest: "fixture-raw-manifest".into(),
        annotation_manifest: "fixture-annotation-manifest".into(),
    }
}

fn deposited(app: &mut App, request: &CaptureRequest) -> Vec<Effect> {
    app.apply_update(Update::SourceDocumentCaptured {
        capture_generation: app.source_capture_generation,
        request: request.clone(),
        result: Ok(stored()),
    })
}

fn saved_events(app: &mut App) -> usize {
    app.take_events()
        .into_iter()
        .filter(|event| matches!(event, AppEvent::SourceDocumentCaptured { .. }))
        .count()
}

fn references(app: &App, node: uuid::Uuid) -> Vec<String> {
    source_capture::references(app.graph_runtimes.facets(), node)
}

fn retry(app: &mut App, request: &CaptureRequest) {
    let effects = app.update(Action::RetrySourceDocumentCapture {
        request: request.id,
    });
    assert!(
        effects.iter().any(|effect| matches!(effect,
            Effect::CaptureSourceDocument { request: replay, .. } if replay == request
        )),
        "retry must replay the original identity and admitted evidence"
    );
}

fn acknowledge(app: &mut App, request: &CaptureRequest, result: Result<(), String>) {
    app.apply_update(Update::SourceCaptureSaved {
        capture_generation: app.source_capture_generation,
        request: request.id,
        result,
    });
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

fn overwrite_capture_facet(
    app: &mut App,
    request: &CaptureRequest,
    facet: &str,
    value: serde_json::Value,
) {
    app.graph_runtimes
        .get_mut(request.graph)
        .unwrap()
        .canvas
        .facets_mut()
        .set(
            request.node,
            chartulary::FacetId::new(facet),
            value,
            &chartulary::AcceptAll,
        )
        .unwrap();
}

fn request_ledger(app: &App, request: &CaptureRequest) -> serde_json::Value {
    app.graph_runtimes
        .get(request.graph)
        .unwrap()
        .canvas
        .graph()
        .facets()
        .get(
            &request.node,
            &chartulary::FacetId::new("capture.source-requests/v1"),
        )
        .unwrap()
        .clone()
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

    let donor =
        session::try_load_session_graph_locked(&session::session_dir(root.path(), donor_request.session))
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
            .args(["--exact", "app::source_capture_persistence_tests::source_capture_save_process_boundary_worker", "--nocapture"])
            .env("TURNSTONE_R0_PROCESS_ROOT", root.path())
            .env("TURNSTONE_R0_PROCESS_STAGE", stage)
            .output().unwrap();
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

fn receive_real_deposit(
    app: &mut App,
    updates: &std::sync::mpsc::Receiver<Update>,
    request: &CaptureRequest,
) -> StoredSourceDocument {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let update = updates
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("trail owner must acknowledge the exact source request");
        match update {
            Update::SourceDocumentCaptured {
                capture_generation,
                request: completed,
                result,
            } => {
                assert_eq!(&completed, request);
                assert_eq!(capture_generation, app.source_capture_generation);
                let stored = result.expect("real source deposit or checked receipt replay");
                app.apply_update(Update::SourceDocumentCaptured {
                    capture_generation,
                    request: completed,
                    result: Ok(stored.clone()),
                });
                return stored;
            },
            Update::SourceCaptureRecoveryFailed { error, .. } => {
                panic!("real receipt recovery failed: {error}")
            },
            other => {
                app.apply_update(other);
            },
        }
    }
}

fn receive_real_recovery(
    app: &mut App,
    updates: &std::sync::mpsc::Receiver<Update>,
    request: &CaptureRequest,
    stored: &StoredSourceDocument,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let update = updates
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("fresh trail owner must recover its durable request receipt");
        match update {
            Update::SourceCaptureRecovered {
                capture_generation,
                request: recovered,
                stored: recovered_stored,
            } => {
                assert_eq!(&recovered, request);
                assert_eq!(capture_generation, app.source_capture_generation);
                assert_eq!(recovered_stored.as_ref(), Some(stored));
                app.apply_update(Update::SourceCaptureRecovered {
                    capture_generation,
                    request: recovered,
                    stored: recovered_stored,
                });
                assert_eq!(
                    app.source_captures[&request.id].phase,
                    CapturePhase::Recoverable
                );
                assert_eq!(
                    saved_events(app),
                    0,
                    "receipt inventory is not Saved success"
                );
                return;
            },
            Update::SourceCaptureRecoveryFailed { error, .. } => {
                panic!("real receipt recovery failed: {error}")
            },
            other => {
                app.apply_update(other);
            },
        }
    }
}

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
                graph.clone()
            };
            let blocked = target.to_path_buf();
            let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
            let result = app.persist_source_capture(request.id);
            assert!(
                result.is_err(),
                "actual required write must fail at {stage}"
            );
            acknowledge(&mut app, &request, result);
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
                        acknowledge(&mut app, &request, result);
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

fn finish_real_process_stage(stage: &str) -> ! {
    println!("TURNSTONE_R0_REAL_STAGE_PASSED:{stage}");
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    std::process::exit(0);
}

#[test]
fn real_fjall_capture_failure_retry_and_reopen_cross_abrupt_process_exits() {
    let root = tempfile::tempdir().unwrap();
    for stage in ["fail-facets", "fail-graph", "retry-save", "verify-reopen"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "app::source_capture_persistence_tests::source_capture_real_stack_process_worker",
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
