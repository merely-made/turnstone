// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! R1 source admission and completion-bound Keep controls.

use super::App;
use crate::action::{Action, Effect, StoredSourceDocument, Update};
use crate::observe::AppEvent;
use crate::source_capture::{CapturePhase, CaptureRequest, SourceCaptureTarget};
use crate::{session, source_capture, surface_controls};
use chartulary::{AcceptAll, FacetId};
use std::path::Path;

const URL_A: &str = "https://r1-keep.example/a";
const URL_B: &str = "https://r1-keep.example/b";
const HTML: &str = "<html><main>R1 source fixture</main></html>";

fn app_at(root: &Path) -> App {
    let mut app = App::test_stub_at(root.to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(root));
    let effects = app.update(Action::NewSession);
    let session = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(*id),
            _ => None,
        })
        .expect("NewSession writes a manifest and asks the app to adopt it");
    app.adopt_session(session);
    app
}

fn visit(app: &mut App, url: &str) -> uuid::Uuid {
    let key = app.graph_runtimes.visit(url);
    app.graph_runtimes.graph().get_node(key).unwrap().id
}

fn document() -> crate::content::FetchedDocument {
    crate::content::FetchedDocument {
        bytes: HTML.as_bytes().to_vec(),
        body: HTML.into(),
        content_type: Some("text/html; charset=utf-8".into()),
        effective_url: Some(URL_A.into()),
        acquired_at_ms: 1_700_000_000_001,
    }
}

fn admit(app: &mut App, node: uuid::Uuid, url: &str) {
    let mut document = document();
    document.effective_url = Some(url.into());
    app.content
        .note_fetched(node, url.into(), document, HTML.len());
}

fn target(app: &App, node: uuid::Uuid) -> SourceCaptureTarget {
    app.source_capture_target(node)
        .expect("complete current HTML source is offered")
}

fn begin(app: &mut App, target: SourceCaptureTarget) -> CaptureRequest {
    app.update(Action::CaptureSourceDocument { target })
        .into_iter()
        .find_map(|effect| match effect {
            Effect::CaptureSourceDocument { request, .. } => Some(request),
            _ => None,
        })
        .expect("a current offer creates a capture request")
}

fn stored() -> StoredSourceDocument {
    StoredSourceDocument {
        raw_manifest: "r1-raw-manifest".into(),
        annotation_manifest: "r1-annotation-manifest".into(),
    }
}

fn deposit(app: &mut App, request: &CaptureRequest) -> Vec<Effect> {
    app.apply_update(Update::SourceDocumentCaptured {
        capture_generation: app.source_capture_generation,
        request: request.clone(),
        result: Ok(stored()),
    })
}

fn kept(app: &App, node: uuid::Uuid) -> bool {
    let graph = app.graph_runtimes.graph();
    surface_controls::read(graph, node, surface_controls::Control::Keep)
}

fn keep_events(app: &mut App, node: uuid::Uuid) -> usize {
    app.take_events()
        .into_iter()
        .filter(|event| matches!(event, AppEvent::NodeKept(actual) if *actual == node))
        .count()
}

#[test]
fn source_offer_is_complete_only_and_unavailable_without_settled_html() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    assert!(app.source_capture_target(node).is_err());
    assert!(
        app.source_capture_status(Some(node))
            .starts_with("Unavailable:")
    );

    app.content.note_streamed(
        node,
        URL_A.into(),
        URL_A.into(),
        Some("text/html".into()),
        HTML.as_bytes(),
    );
    assert!(
        app.source_capture_target(node).is_err(),
        "a partial response is not offered"
    );
    assert!(
        app.source_capture_status(Some(node))
            .starts_with("Unavailable:")
    );

    admit(&mut app, node, URL_A);
    assert!(app.source_capture_target(node).is_ok());
    assert_eq!(app.source_capture_status(Some(node)), "Ready");
}

#[test]
fn target_stays_bound_to_a_across_cursor_a_b_a_and_completion_keeps_only_a() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node_a = visit(&mut app, URL_A);
    admit(&mut app, node_a, URL_A);
    let offer = target(&app, node_a);
    let request = begin(&mut app, offer);
    assert!(
        request.keep,
        "fresh R1 requests carry completion-bound Keep"
    );

    let node_b = visit(&mut app, URL_B);
    let _cursor_b = app.graph_runtimes.focused_member();
    let _cursor_a = app.graph_runtimes.visit(URL_A);
    assert_eq!(app.graph_runtimes.focused_member(), Some(node_a));
    assert_eq!(request.node, node_a);
    let effects = deposit(&mut app, &request);

    assert_eq!(
        effects.iter().filter(|effect| matches!(effect, Effect::PersistSourceCapture { request: id } if *id == request.id)).count(),
        1,
        "one admitted completion requests exactly one owning save"
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::PersistSourceCapture { .. }))
            .count(),
        1
    );
    assert!(kept(&app, node_a));
    assert!(!kept(&app, node_b), "focus/cursor movement cannot Keep B");
    let events = app.take_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AppEvent::NodeKept(actual) if *actual == node_a))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AppEvent::NodeKept(actual) if *actual == node_b))
            .count(),
        0
    );
    assert_eq!(
        source_capture::references(app.graph_runtimes.facets(), node_a),
        vec![stored().annotation_manifest]
    );
    assert!(source_capture::references(app.graph_runtimes.facets(), node_b).is_empty());
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::PendingSave
    );
    assert!(
        !app.take_events()
            .iter()
            .any(|event| matches!(event, AppEvent::SourceDocumentCaptured { .. })),
        "deposit is not Saved success before R0 acknowledgement"
    );

    app.apply_update(Update::SourceCaptureSaved {
        capture_generation: app.source_capture_generation,
        request: request.id,
        result: Ok(()),
    });
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Saved);
    assert_eq!(app.take_events().iter().filter(|event| matches!(event, AppEvent::SourceDocumentCaptured { node, .. } if *node == node_a)).count(), 1);
}

#[test]
fn equal_response_replacement_is_a_new_admission_and_invalidates_the_offer() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let old_target = target(&app, node);
    let request = begin(&mut app, old_target.clone());
    let old_source = old_target.source.clone();

    admit(&mut app, node, URL_A);
    let replacement = target(&app, node);
    assert_eq!(
        old_target.source, replacement.source,
        "bytes and acquisition time match"
    );
    assert_ne!(
        old_target.source_admission, replacement.source_admission,
        "a new complete-response admission has distinct authority"
    );

    let effects = deposit(&mut app, &request);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::PersistSourceCapture { .. }))
    );
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
    assert!(!kept(&app, node));
    assert!(matches!(
        app.source_captures[&request.id].phase,
        CapturePhase::Failed
    ));
    assert_eq!(
        app.source_captures[&request.id].stored.as_ref(),
        Some(&stored())
    );
    assert_eq!(old_source, replacement.source);

    let retried = app.update(Action::RetrySourceDocumentCapture {
        request: request.id,
    });
    assert!(
        !retried
            .iter()
            .any(|effect| matches!(effect, Effect::CaptureSourceDocument { .. }))
    );
    assert!(!kept(&app, node));
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
}

#[test]
fn transient_deposit_failure_can_retry_with_the_same_source_admission() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let offer = target(&app, node);
    let request = begin(&mut app, offer);

    app.apply_update(Update::SourceDocumentCaptured {
        capture_generation: app.source_capture_generation,
        request: request.clone(),
        result: Err("temporary actor failure".into()),
    });
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert!(app.source_captures[&request.id].stored.is_none());

    let retried = app.update(Action::RetrySourceDocumentCapture {
        request: request.id,
    });
    assert!(retried.iter().any(|effect| matches!(
        effect,
        Effect::CaptureSourceDocument { request: replay, .. } if replay == &request
    )));
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::Depositing
    );
    assert!(!kept(&app, node));
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
}

#[test]
fn stale_offer_refusal_cannot_be_retried_into_a_capture_effect() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let stale_offer = target(&app, node);

    // Even an identical response and timestamp is a fresh admission token.
    admit(&mut app, node, URL_A);
    let refused = app.update(Action::CaptureSourceDocument {
        target: stale_offer,
    });
    assert!(
        !refused
            .iter()
            .any(|effect| matches!(effect, Effect::CaptureSourceDocument { .. }))
    );
    let (request_id, attempt) = app
        .source_captures
        .iter()
        .find(|(_, attempt)| attempt.request.node == node)
        .expect("refused offers still have an attributable status record");
    assert_eq!(attempt.phase, CapturePhase::Failed);
    let request_id = *request_id;

    let retried = app.update(Action::RetrySourceDocumentCapture {
        request: request_id,
    });
    assert!(
        !retried
            .iter()
            .any(|effect| matches!(effect, Effect::CaptureSourceDocument { .. }))
    );
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
    assert!(!kept(&app, node));
}

#[test]
fn queued_capture_effect_is_refused_after_source_admission_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let offer = target(&app, node);
    let request = begin(&mut app, offer);

    // The Effect has been accepted and queued, but its actor call has not
    // happened. A replacement response invalidates the admission authority.
    admit(&mut app, node, URL_A);
    let refusal = app.source_capture_owner(&request).unwrap_err();
    assert!(refusal.contains("source response changed"), "{refusal}");
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
    assert!(!kept(&app, node));
}

#[test]
fn wrong_generation_or_session_cannot_admit_a_completion() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let offer = target(&app, node);
    let request = begin(&mut app, offer);
    let stale_generation = uuid::Uuid::new_v4();
    assert!(
        app.apply_update(Update::SourceDocumentCaptured {
            capture_generation: stale_generation,
            request: request.clone(),
            result: Ok(stored()),
        })
        .is_empty()
    );
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());

    let mut wrong_session = request.clone();
    wrong_session.session = crate::panes::SessionId::new();
    assert!(app.source_capture_owner(&wrong_session).is_err());
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
}

#[test]
fn malformed_keep_facet_refuses_attachment_without_mutation() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let keep_id = FacetId::new("turnstone.controls.v1.keep");
    app.graph_runtimes
        .active_canvas_mut()
        .facets_mut()
        .set(
            node,
            keep_id.clone(),
            serde_json::json!("malformed"),
            &AcceptAll,
        )
        .unwrap();
    let offer = target(&app, node);
    let request = begin(&mut app, offer);
    assert!(request.keep);

    let effects = deposit(&mut app, &request);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::PersistSourceCapture { .. }))
    );
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
    assert_eq!(
        app.graph_runtimes.facets().get(&node, &keep_id),
        Some(&serde_json::json!("malformed"))
    );
    assert_eq!(keep_events(&mut app, node), 0);
}

#[test]
fn legacy_r0_request_defaults_to_no_keep_and_does_not_promote() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let offer = target(&app, node);
    let mut request = begin(&mut app, offer);

    // Missing `keep` in a pre-R1 receipt deserializes as false. Match the
    // in-memory request as if this old receipt was being replayed.
    let legacy: CaptureRequest = serde_json::from_value(serde_json::json!({
        "id": uuid::Uuid::new_v4(), "session": app.session_id, "graph": app.graph_runtimes.active_graph(),
        "node": node, "url": URL_A, "source": null
    })).unwrap();
    assert!(!legacy.keep);
    request.keep = legacy.keep;
    app.source_captures.get_mut(&request.id).unwrap().request = request.clone();
    let effects = deposit(&mut app, &request);
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::PersistSourceCapture { request: id } if *id == request.id)
    ));
    assert!(!kept(&app, node));
    assert_eq!(
        app.source_captures[&request.id].phase,
        CapturePhase::PendingSave
    );
}

#[test]
fn queued_capture_effect_cannot_acquire_a_new_a_b_a_opening_generation() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let session_a = app.session_id;
    let node = visit(&mut app, URL_A);
    app.persist_session_graph().unwrap();
    admit(&mut app, node, URL_A);
    let offered = target(&app, node);
    let effects = app.update(Action::CaptureSourceDocument { target: offered });
    let (request, generation) = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::CaptureSourceDocument {
                request,
                capture_generation,
            } => Some((request, capture_generation)),
            _ => None,
        })
        .unwrap();
    assert!(
        app.source_capture_dispatch_owner(&request, generation)
            .is_ok()
    );
    let effects = app.update(Action::NewSession);
    let session_b = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(id),
            _ => None,
        })
        .unwrap();
    app.adopt_session(session_b);
    app.adopt_session(session_a);
    assert_ne!(generation, app.source_capture_generation);
    assert!(
        app.source_capture_dispatch_owner(&request, generation)
            .unwrap_err()
            .contains("earlier session opening")
    );
    assert!(!kept(&app, node));
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
}

#[test]
fn hosted_engine_cannot_capture_a_background_fetch_as_its_own_source() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let offered = target(&app, node);
    let request = begin(&mut app, offered);
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: "scrying.web".into(),
            lane: crate::content::ContentLane::Surface,
            structure: None,
            lineage: None,
            capabilities: Default::default(),
        }),
    );
    let reason = app.source_capture_target(node).unwrap_err();
    assert!(reason.contains("System webview"));
    assert!(reason.contains("does not provide an exact fetched HTML response"));
    assert!(app.source_capture_owner(&request).is_err());
    deposit(&mut app, &request);
    assert_eq!(app.source_captures[&request.id].phase, CapturePhase::Failed);
    assert!(!kept(&app, node));
    assert!(source_capture::references(app.graph_runtimes.facets(), node).is_empty());
}

#[test]
fn terminal_without_current_response_provenance_cannot_borrow_a_previous_source() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let node = visit(&mut app, URL_A);
    admit(&mut app, node, URL_A);
    let previous = target(&app, node);
    let request = fetch::next_fetch_request_id();
    app.content.begin_fetch(node, request);
    app.apply_update(Update::PageFetched {
        request,
        node,
        url: URL_A.into(),
        result: Ok(crate::action::FetchedPage {
            content_type: Some("text/html".into()),
            content_disposition: None,
            bytes: b"new body without observed final URL".to_vec(),
            body: "new body without observed final URL".into(),
        }),
    });
    let document = app.content.fetched(node, URL_A).unwrap();
    assert!(document.effective_url.is_none());
    assert_ne!(
        crate::source_capture::SourceIdentity::of(document),
        previous.source
    );
    assert!(app.source_capture_target(node).is_err());
}
