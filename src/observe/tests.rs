// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Action;
use crate::content::{CapabilityStatus, ContentFacts, DocumentCapabilityFacts};

/// Weld's page-zoom limit as `Shell::weld` reports it. Copied rather than
/// imported: that lane is behind the `weld` feature and these tests run
/// without it, so the copy is the thing to re-check when the lane changes.
const WELD_PAGE_ZOOM_DETAIL: &str = "the requested scale is applied as a CEF zoom level, but Windows runs CEF's UI thread separately so the effective level cannot be read back";

#[test]
fn snapshot_reads_focus_omnibar_and_content_coherently() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".to_string()));
    app.update(Action::OmnibarOpen { command: true });
    // Search ranks the person's kept and recent commands first (the
    // command set's order), so the query is specific enough to keep its
    // command within the row limit.
    for c in "res".chars() {
        app.update(Action::OmnibarChar(c));
    }

    let snap = snapshot(&app);
    let focused = snap.focused.expect("the opened node is focused");
    assert_eq!(focused.url, "mere://alpha");
    assert!(snap.omnibar.open);
    assert_eq!(snap.omnibar.text, ">res");
    assert!(
        snap.omnibar
            .suggestions
            .iter()
            .any(|s| s.contains("Reseed layout")),
        "suggestion rows render as display strings: {:?}",
        snap.omnibar.suggestions
    );
    assert_eq!(snap.node_count, 1);
    assert!(snap.content.is_empty(), "no content lifecycle yet");
}

#[test]
fn snapshot_projects_focused_document_capabilities() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_live(
        node,
        Some(ContentFacts {
            engine: "weld.chromium".into(),
            lane: crate::content::ContentLane::Surface,
            structure: None,
            lineage: None,
            capabilities: DocumentCapabilityFacts {
                find_in_page: CapabilityStatus::Supported,
                // Weld's real limit today: the scale IS applied; only the
                // read-back is missing.
                page_zoom: CapabilityStatus::Partial {
                    detail: WELD_PAGE_ZOOM_DETAIL.into(),
                },
                page_capture: CapabilityStatus::Partial {
                    detail: "visible viewport only".into(),
                },
                navigation: CapabilityStatus::Supported,
            },
        }),
    );

    let capabilities = snapshot(&app)
        .document_capabilities
        .expect("focused live capabilities");
    assert_eq!(capabilities.find_in_page, "supported");
    assert_eq!(
        capabilities.page_zoom,
        format!("partial: {WELD_PAGE_ZOOM_DETAIL}")
    );
    assert_eq!(capabilities.page_capture, "partial: visible viewport only");
    assert_eq!(capabilities.navigation, "supported");
}

/// The snapshot discloses the REQUESTED page zoom, and the change is an
/// observable event. A hosted surface has no read-back, so the applied
/// half stays absent rather than echoing the request.
#[test]
fn snapshot_reports_the_requested_page_zoom() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/zoom".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    assert_eq!(
        snapshot(&app).focused.expect("focused").page_zoom_percent,
        100,
        "a node that never zoomed sits at the default scale"
    );

    app.content.note_live(
        node,
        Some(ContentFacts {
            engine: "weld.chromium".into(),
            lane: crate::content::ContentLane::Surface,
            structure: None,
            lineage: None,
            capabilities: DocumentCapabilityFacts {
                page_zoom: CapabilityStatus::Partial {
                    detail: WELD_PAGE_ZOOM_DETAIL.into(),
                },
                ..Default::default()
            },
        }),
    );
    app.update(Action::PageZoomIn { member: node });
    app.update(Action::PageZoomIn { member: node });
    assert_eq!(
        snapshot(&app).focused.expect("focused").page_zoom_percent,
        125
    );
    assert_eq!(
        snapshot(&app)
            .focused
            .expect("focused")
            .applied_page_zoom_percent,
        None,
        "Weld never reported an effective level, so none is invented"
    );
    let described: Vec<String> = app.take_events().iter().map(AppEvent::describe).collect();
    assert!(
        described
            .iter()
            .any(|event| event == &format!("page-zoom-changed {node} 125%")),
        "{described:?}"
    );

    app.update(Action::PageZoomReset { member: node });
    assert_eq!(
        snapshot(&app).focused.expect("focused").page_zoom_percent,
        100
    );
}

/// A retained session answers a zoom request with the level it settled on,
/// and the snapshot carries that beside the request.
#[test]
fn snapshot_reports_a_retained_engine_applied_page_zoom() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/zoom".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_live(
        node,
        Some(ContentFacts {
            engine: "genet.livery".into(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: DocumentCapabilityFacts {
                page_zoom: CapabilityStatus::Supported,
                ..Default::default()
            },
        }),
    );
    app.update(Action::PageZoomIn { member: node });
    app.content.note_page_zoom(
        node,
        crate::content::PageZoomFacts {
            requested: 1.1,
            applied: 1.1,
            min: 0.25,
            max: 5.0,
        },
    );
    let focused = snapshot(&app).focused.expect("focused");
    assert_eq!(focused.page_zoom_percent, 110);
    assert_eq!(focused.applied_page_zoom_percent, Some(110));

    // The read-back belongs to the session, so closing takes it along.
    app.content.note_closed(node);
    assert_eq!(
        snapshot(&app)
            .focused
            .expect("focused")
            .applied_page_zoom_percent,
        None
    );
}

#[test]
fn semantic_actions_emit_events() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".to_string()));
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarClose);
    app.update(Action::ToggleNodeContent);
    let described: Vec<String> = app.take_events().iter().map(AppEvent::describe).collect();
    assert!(described.iter().any(|e| e == "address-opened mere://alpha"));
    assert!(described.iter().any(|e| e == "omnibar-opened"));
    assert!(described.iter().any(|e| e == "omnibar-closed"));
    assert!(
        described
            .iter()
            .any(|e| e.starts_with("content ") && e.ends_with(" requested")),
        "{described:?}"
    );
    assert!(app.take_events().is_empty(), "take drains");
}
