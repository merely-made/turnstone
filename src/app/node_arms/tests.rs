// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::action::{Action, Effect};
use crate::app::App;

#[test]
fn opening_an_address_keeps_the_new_selection_through_pane_rendering() {
    let mut app = App::test_stub();
    let pane = app.default_graph_pane();

    app.update(Action::OpenAddress("https://example.com/".into()));
    let selected = app
        .graph_runtimes
        .focused_member()
        .expect("opening an address selects its node");

    // A graph frame restores pane-local state. The selection must survive
    // that restore or the next content action has no node to target.
    app.graph_pane_frame(pane, 800, 600)
        .expect("the default pane owns the active graph");
    assert_eq!(app.graph_pane_focused_member(pane), Some(selected));
    assert_eq!(app.graph_runtimes.focused_member(), Some(selected));
}

#[test]
fn committed_content_navigation_grows_the_same_members_lineage() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    let before = app.graph_runtimes.graph().node_count();

    let effects = app.update(Action::ContentNavigationCommitted {
        member,
        url: "https://www.iana.org/help/example-domains".into(),
    });

    assert_eq!(app.graph_runtimes.graph().node_count(), before);
    assert_eq!(
        app.graph_runtimes
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.url()),
        Some("https://www.iana.org/help/example-domains")
    );
    let key = app.graph_runtimes.graph().get_node_by_id(member).unwrap().0;
    assert_eq!(
        app.graph_runtimes
            .graph()
            .node_history_projection(key)
            .entries,
        vec![
            "https://example.com/".to_string(),
            "https://www.iana.org/help/example-domains".to_string(),
        ]
    );
    assert!(effects.contains(&Effect::SaveSession));
}

/// A live node whose engine reports the control, ready to zoom.
fn note_zoomable(app: &mut App, member: uuid::Uuid) {
    app.content.note_live(
        member,
        Some(crate::content::ContentFacts {
            engine: "test.surface".into(),
            lane: crate::content::ContentLane::Surface,
            structure: None,
            lineage: None,
            capabilities: crate::content::DocumentCapabilityFacts {
                page_zoom: crate::content::CapabilityStatus::Partial {
                    detail: "applied, but the effective level is not read back".into(),
                },
                ..Default::default()
            },
        }),
    );
}

fn requested(app: &App, member: uuid::Uuid) -> Option<f32> {
    app.browser.get(member).and_then(|state| state.page_scale)
}

#[test]
fn page_zoom_steps_the_chromium_ladder_and_persists_the_request() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/zoom".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    note_zoomable(&mut app, member);

    let effects = app.update(Action::PageZoomIn { member });
    assert_eq!(requested(&app, member), Some(1.1));
    assert!(effects.contains(&Effect::SaveSession));
    assert!(effects.contains(&Effect::ScaleContent {
        node: member,
        scale: 1.1
    }));

    app.update(Action::PageZoomIn { member });
    assert_eq!(requested(&app, member), Some(1.25));

    app.update(Action::PageZoomOut { member });
    app.update(Action::PageZoomOut { member });
    app.update(Action::PageZoomOut { member });
    assert_eq!(requested(&app, member), Some(0.9), "1.25 → 1.1 → 1.0 → 0.9");

    // Reset CLEARS rather than storing 1.0, so the save path's empty-entry
    // pruning can drop a node that carries nothing else.
    let effects = app.update(Action::PageZoomReset { member });
    assert_eq!(requested(&app, member), None);
    assert!(effects.contains(&Effect::ScaleContent {
        node: member,
        scale: 1.0
    }));
    assert!(
        app.browser.is_empty(),
        "a reset node has nothing to persist"
    );
}

#[test]
fn the_ladder_ends_report_rather_than_stepping_past_themselves() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/zoom".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    note_zoomable(&mut app, member);
    app.browser.entry(member).page_scale = Some(5.0);

    let effects = app.update(Action::PageZoomIn { member });
    assert_eq!(effects, vec![Effect::Redraw]);
    assert_eq!(requested(&app, member), Some(5.0));
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::AffordanceUnavailable { what, target }
            if *what == "page-zoom" && target.contains("largest scale")
    )));

    app.browser.entry(member).page_scale = Some(0.25);
    let effects = app.update(Action::PageZoomOut { member });
    assert_eq!(effects, vec![Effect::Redraw]);
    assert_eq!(requested(&app, member), Some(0.25));
}

#[test]
fn zoom_rows_appear_only_for_a_live_engine_that_reports_the_control() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/zoom".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    let labels = |app: &App| -> Vec<String> {
        app.available_actions()
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    };

    // Content off: no rows.
    assert!(!labels(&app).iter().any(|label| label == "Zoom in"));

    // Live, but the engine says no: still no rows, and a chord aimed at it
    // persists nothing.
    app.content.note_live(
        member,
        Some(crate::content::ContentFacts {
            engine: "test.opaque".into(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: crate::content::DocumentCapabilityFacts {
                page_zoom: crate::content::CapabilityStatus::Unsupported {
                    reason: "opaque surface has no document scale".into(),
                },
                ..Default::default()
            },
        }),
    );
    assert!(!labels(&app).iter().any(|label| label == "Zoom in"));
    let effects = app.update(Action::PageZoomIn { member });
    assert_eq!(effects, vec![Effect::Redraw]);
    assert_eq!(requested(&app, member), None);
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::AffordanceUnavailable { what, target }
            if *what == "page-zoom"
                && target.contains("opaque surface has no document scale")
    )));

    note_zoomable(&mut app, member);
    let labels = labels(&app);
    for row in ["Zoom in", "Zoom out", "Reset zoom"] {
        assert!(labels.iter().any(|label| label == row), "{row} is offered");
    }
}
