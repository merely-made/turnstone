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
        result: result.map_err(Into::into),
    });
}

fn overwrite_capture_facet(
    app: &mut App,
    request: &CaptureRequest,
    facet: &str,
    value: serde_json::Value,
) {
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(request.graph).unwrap().canvas,
        |facets| {
            facets.set(
                request.node,
                chartulary::FacetId::new(facet),
                value,
                &chartulary::AcceptAll,
            )
        },
    )
    .unwrap()
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

fn finish_real_process_stage(stage: &str) -> ! {
    println!("TURNSTONE_R0_REAL_STAGE_PASSED:{stage}");
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    std::process::exit(0);
}

#[path = "source_capture_persistence_tests/custody_retirement_clears_fetched_sources_and_rejects_their_cases.rs"]
mod custody_retirement_clears_fetched_sources_and_rejects_their_cases;
#[path = "source_capture_persistence_tests/one_of_two_identical_manifest_references_cannot_satisfy_cases.rs"]
mod one_of_two_identical_manifest_references_cannot_satisfy_cases;
#[path = "source_capture_persistence_tests/source_capture_real_stack_process_worker_cases.rs"]
mod source_capture_real_stack_process_worker_cases;
#[path = "source_capture_persistence_tests/warm_reopen_with_intact_artifact_receipt_demotes_saved_cases.rs"]
mod warm_reopen_with_intact_artifact_receipt_demotes_saved_cases;
