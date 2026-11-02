// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::{Action, Update};
use crate::content::{CapabilityStatus, ContentFacts, DocumentCapabilityFacts, StructureFacts};

/// Weld's page-zoom limit as `Shell::weld` reports it. Copied rather than
/// imported: that lane is behind the `weld` feature and these tests run
/// without it, so the copy is the thing to re-check when the lane changes.
const WELD_PAGE_ZOOM_DETAIL: &str = "the requested scale is applied as a CEF zoom level, but Windows runs CEF's UI thread separately so the effective level cannot be read back";

#[test]
fn only_a_person_with_the_ui_id_renders_as_you() {
    use mere::kernel::graph::Author;
    let app = App::test_stub();
    assert_eq!(journal_author_label(&app, &Author::user()), "you");
    for author in [
        Author::script("user", "body-1").via("turnstone"),
        Author::engine("user", "host-1").via("turnstone"),
        Author::rule("user", "rule-1").via("turnstone"),
    ] {
        assert_eq!(journal_author_label(&app, &author), author.to_string());
        assert_ne!(journal_author_label(&app, &author), "you");
    }
}

#[test]
fn no_focus_reports_none_honestly() {
    let app = App::test_stub();
    let sections = inspector_sections(&app);
    assert_eq!(
        sections[0].rows,
        vec![("Focused node".to_string(), "none".to_string())]
    );
    assert_eq!(
        sections[1].rows,
        vec![("Content state".to_string(), "none".to_string())]
    );
}

#[test]
fn content_tags_are_sorted_and_follow_tag_removal() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/tagged".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    assert!(app.graph_runtimes.tag_node(member, "zebra"));
    assert!(app.graph_runtimes.tag_node(member, "apple"));
    assert!(
        inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: apple, zebra")
    );
    assert!(app.graph_runtimes.untag_node(member, "apple"));
    assert!(
        inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: zebra")
    );
    assert!(app.graph_runtimes.untag_node(member, "zebra"));
    assert!(
        inspector_lines(&app)
            .iter()
            .any(|line| line == "Tags: none")
    );
}

/// The full read: node facts off graph truth, content facts off the
/// spawn-time mirror — the same rows the pane draws and a scenario asserts.
#[test]
fn focused_node_with_live_content_reports_both_sections() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/page".to_string()));
    let node = app
        .graph_runtimes
        .focused_member()
        .expect("opened node focused");
    app.apply_update(Update::ContentSpawned {
        node,
        facts: Some(ContentFacts {
            engine: "genet.web".to_string(),
            lane: crate::content::ContentLane::Document,
            lineage: None,
            capabilities: Default::default(),
            structure: Some(StructureFacts {
                title: Some("The Page".to_string()),
                headings: 2,
                links: 5,
                outline: vec![
                    crate::content::OutlineFact {
                        depth: 0,
                        role: "group",
                        name: String::new(),
                    };
                    9
                ],
            }),
        }),
    });
    let lines = inspector_lines(&app);
    let has = |s: &str| lines.iter().any(|l| l.contains(s));
    assert!(has("URL: https://example.com/page"), "{lines:?}");
    assert!(has(&format!("Node id: {node}")));
    assert!(has("Map position pinned: no"));
    assert!(has("Content state: live"));
    assert!(has("Engine: genet.web"));
    assert!(has("Document title: The Page"));
    assert!(has("Document structure: headings=2 outline=9"));
    assert!(has("Outgoing links: 5"));
}

/// A lane without a structural read says so; nothing is synthesized.
#[test]
fn a_lane_without_introspection_is_reported_not_synthesized() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/x".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.apply_update(Update::ContentSpawned {
        node,
        facts: Some(ContentFacts {
            engine: "some.lane".to_string(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: Default::default(),
        }),
    });
    let lines = inspector_lines(&app);
    assert!(
        lines
            .iter()
            .any(|l| l == "Structural read: none for this lane")
    );
    assert!(!lines.iter().any(|l| l.starts_with("Document structure")));
}

#[test]
fn livery_and_weld_capability_limits_are_presented_honestly() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress(
        "https://example.com/capabilities".to_string(),
    ));
    let node = app.graph_runtimes.focused_member().unwrap();
    // Livery's real limits today: it finds and zooms for itself, cannot
    // hand back a rendered capture, and only shares navigation with the
    // host that owns lineage and refetch.
    let livery = DocumentCapabilityFacts {
        find_in_page: CapabilityStatus::Supported,
        page_zoom: CapabilityStatus::Supported,
        page_capture: CapabilityStatus::Unsupported {
            reason: "Livery sessions do not capture rendered pages".into(),
        },
        navigation: CapabilityStatus::Partial {
            detail: "the host owns document lineage, policy, and refetch".into(),
        },
    };
    app.content.note_live(
        node,
        Some(ContentFacts {
            engine: "genet.livery".into(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: livery,
        }),
    );
    let lines = inspector_lines(&app);
    assert!(lines.iter().any(|line| line == "Find in page: supported"));
    assert!(lines.iter().any(|line| line == "Page zoom: supported"));
    assert!(lines.iter().any(|line| {
        line == "Page capture: unavailable: Livery sessions do not capture rendered pages"
    }));
    assert!(lines.iter().any(|line| {
        line == "Navigation controls: partial: the host owns document lineage, policy, and refetch"
    }));
    // No engine has answered a zoom request on this node, so the row is
    // the request alone — the applied half is never invented.
    assert!(lines.iter().any(|line| line == "Requested page zoom: 100%"));
    app.content.note_page_zoom(
        node,
        crate::content::PageZoomFacts {
            requested: 1.25,
            applied: 1.25,
            min: 0.25,
            max: 5.0,
        },
    );
    assert!(
        inspector_lines(&app)
            .iter()
            .any(|line| line == "Requested page zoom: 100% (applied 125%)")
    );
    // Closing the content takes the effective level with it: the read-back
    // belongs to the session that computed it, not to the node. The Weld
    // half below re-opens on the same node, and its request-only row is
    // what proves the transient entry went with the session.
    app.content.note_closed(node);

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
                // read-back is missing (`Shell::weld` reports this string).
                page_zoom: CapabilityStatus::Partial {
                    detail: WELD_PAGE_ZOOM_DETAIL.into(),
                },
                page_capture: CapabilityStatus::Unsupported {
                    reason: "Turnstone has not projected Weld snapshots yet".into(),
                },
                navigation: CapabilityStatus::Supported,
            },
        }),
    );
    let lines = inspector_lines(&app);
    assert!(
        lines
            .iter()
            .any(|line| line == "Navigation controls: supported")
    );
    assert!(
        lines
            .iter()
            .any(|line| *line == format!("Page zoom: partial: {WELD_PAGE_ZOOM_DETAIL}"))
    );
    assert!(lines.iter().any(|line| {
        line == "Page capture: unavailable: Turnstone has not projected Weld snapshots yet"
    }));

    // The requested scale is disclosed as requested, never as applied.
    assert!(lines.iter().any(|line| line == "Requested page zoom: 100%"));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::PageZoomIn { member: node });
    assert!(
        inspector_lines(&app)
            .iter()
            .any(|line| line == "Requested page zoom: 110%")
    );
}

/// Closing the content drops the facts with it (no stale mirror).
#[test]
fn closing_content_drops_the_facts() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/x".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.apply_update(Update::ContentSpawned {
        node,
        facts: Some(ContentFacts {
            engine: "genet.web".to_string(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: Default::default(),
        }),
    });
    assert!(app.content.facts(node).is_some());
    app.update(Action::ToggleNodeContent); // live -> close
    assert!(app.content.facts(node).is_none());
    let lines = inspector_lines(&app);
    assert!(lines.iter().any(|l| l == "Content state: none"));
    assert!(!lines.iter().any(|l| l.starts_with("Engine")));
}
