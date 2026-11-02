// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Action;

/// One REAL Livery session over an inline body — no network, and no stub
/// standing in for the capability report the palette gate actually reads.
fn livery_session() -> Box<dyn inker::DocumentSession<netrender::Scene>> {
    crate::shell::standard_content_engines()
        .spawn(
            inker::routing::ENGINE_GENET_LIVERY,
            &SessionSpawnRequest::new("https://example.test/zoom")
                .with_viewport(800, 600)
                .with_body("<html><body><p>zoom me</p></body></html>")
                .with_content_type("text/html"),
        )
        .expect("the Livery lane spawns from an inline body")
}

fn opened_node(app: &mut crate::app::App) -> uuid::Uuid {
    app.update(Action::OpenAddress("https://example.test/zoom".to_string()));
    app.graph_runtimes.focused_member().expect("a focused node")
}

/// The gate reads capability facts, and a live Livery session now reports
/// page zoom as supported — so the three rows are offered without any
/// engine-id special case in the palette.
#[test]
fn a_live_livery_session_is_offered_the_zoom_rows() {
    let mut app = crate::app::App::test_stub();
    let node = opened_node(&mut app);
    let mut session = livery_session();
    let facts = app_document_capabilities(session.document_capabilities());
    assert_eq!(facts.page_zoom, crate::content::CapabilityStatus::Supported);
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: inker::routing::ENGINE_GENET_LIVERY.to_string(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: facts,
        }),
    );
    let labels: Vec<String> = app
        .available_actions()
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    for row in ["Zoom in", "Zoom out", "Reset zoom"] {
        assert!(
            labels.iter().any(|label| label == row),
            "{row} is offered to a retained Livery session; got {labels:?}"
        );
    }

    // And the row the palette offers reaches the session, which answers
    // with the level it settled on.
    let effects = app.update(Action::PageZoomIn { member: node });
    let scale = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ScaleContent {
                node: target,
                scale,
            } if *target == node => Some(*scale),
            _ => None,
        })
        .expect("zoom in asks for a content scale");
    apply_retained_page_scale(&mut app, node, scale, session.as_mut());
    let zoom = app
        .content
        .page_zoom(node)
        .expect("Livery reads its level back");
    assert_eq!(zoom.requested, 1.1);
    assert_eq!(zoom.applied, 1.1);
    assert_eq!((zoom.min, zoom.max), (0.25, 5.0));
}

/// Reset persists `None`, but a live session must still be told to go back
/// to 1.0 — the request is cleared, the document is not left magnified.
#[test]
fn reset_returns_a_live_retained_session_to_the_default_scale() {
    let mut app = crate::app::App::test_stub();
    let node = opened_node(&mut app);
    let mut session = livery_session();
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: inker::routing::ENGINE_GENET_LIVERY.to_string(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: app_document_capabilities(session.document_capabilities()),
        }),
    );
    app.update(Action::PageZoomIn { member: node });
    app.update(Action::PageZoomIn { member: node });
    apply_retained_page_scale(&mut app, node, 1.25, session.as_mut());
    assert_eq!(
        app.content.page_zoom(node).map(|zoom| zoom.applied),
        Some(1.25)
    );

    let effects = app.update(Action::PageZoomReset { member: node });
    assert!(
        effects.contains(&Effect::ScaleContent {
            node,
            scale: crate::app::PAGE_ZOOM_DEFAULT,
        }),
        "reset still commands the live session back to 100%"
    );
    assert!(
        app.browser
            .get(node)
            .and_then(|state| state.page_scale)
            .is_none(),
        "reset clears the persisted request rather than storing 1.0"
    );
    apply_retained_page_scale(
        &mut app,
        node,
        crate::app::PAGE_ZOOM_DEFAULT,
        session.as_mut(),
    );
    assert_eq!(
        app.content.page_zoom(node).map(|zoom| zoom.applied),
        Some(1.0)
    );
}

/// A lane without the control refuses, and the handler must absorb it: the
/// capability gate makes this unreachable through the UI, but a chord or a
/// script aimed at one must not panic or invent an applied level.
#[test]
fn an_unsupported_retained_lane_is_absorbed_quietly() {
    let mut app = crate::app::App::test_stub();
    let node = opened_node(&mut app);
    let mut session = crate::shell::standard_content_engines()
        .spawn(
            inker::routing::ENGINE_NEMATIC_GEMTEXT,
            &SessionSpawnRequest::new("gemini://example.test/zoom")
                .with_viewport(800, 600)
                .with_body("# heading\n"),
        )
        .expect("the gemtext lane spawns from an inline body");
    assert!(matches!(
        session.set_page_zoom(1.25),
        Err(inker::SessionError::Unsupported(_))
    ));
    apply_retained_page_scale(&mut app, node, 1.25, session.as_mut());
    assert!(
        app.content.page_zoom(node).is_none(),
        "a refusal leaves no applied level behind"
    );
    assert!(
        !app.take_events().iter().any(|event| matches!(
            event,
            crate::observe::AppEvent::AffordanceUnavailable { what, .. } if *what == "page-zoom"
        )),
        "an unsupported lane is a quiet debug, not a reported refusal"
    );
}

/// The spawn seed point: a node carrying a persisted request hands it to a
/// freshly spawned session, and a node at the default asks for nothing.
#[test]
fn a_fresh_retained_session_is_seeded_with_the_persisted_request() {
    let mut app = crate::app::App::test_stub();
    let node = opened_node(&mut app);
    let mut session = livery_session();
    replay_retained_page_scale(&mut app, node, session.as_mut());
    assert!(
        app.content.page_zoom(node).is_none(),
        "a node at the default scale asks a fresh session for nothing"
    );

    app.browser.entry(node).page_scale = Some(1.5);
    replay_retained_page_scale(&mut app, node, session.as_mut());
    assert_eq!(
        app.content.page_zoom(node).map(|zoom| zoom.applied),
        Some(1.5)
    );
}
