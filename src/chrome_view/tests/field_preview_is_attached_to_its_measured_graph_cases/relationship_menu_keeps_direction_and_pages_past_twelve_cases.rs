// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn relationship_menu_keeps_direction_and_pages_past_twelve_neighbors() {
    use mere::kernel::graph::{EdgeAssertion, SemanticSubKind, apply::assert_relation};
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/root".into()));
    let pane = app.default_graph_pane();
    let source = app.graph_pane_focused_member(pane).unwrap();
    let mut members = Vec::new();
    for i in 0..15 {
        let member = app
            .with_graph_pane(pane, |canvas| {
                canvas.open_member_as_new_node(None, &format!("https://example.test/{i:02}"))
            })
            .unwrap();
        app.with_graph_pane(pane, |canvas| {
            canvas.ingest_graph(|graph| {
                let a = graph.get_node_by_id(source).unwrap().0;
                let b = graph.get_node_by_id(member).unwrap().0;
                assert_relation(
                    graph,
                    a,
                    b,
                    EdgeAssertion::Semantic {
                        sub_kind: SemanticSubKind::Cites,
                        label: None,
                    },
                )
                .unwrap();
                true
            });
        });
        members.push(member);
    }
    app.graph_pane_select_member(pane, source);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 700.0)]);
    let (x, y) = toolbar_point(&chrome, "Document connections", 1024, 700);
    chrome.click(0, x, y, 1024, 700);
    for _ in 0..2 {
        let (x, y) = toolbar_point(&chrome, "More relationships", 1024, 700);
        assert_eq!(
            chrome.click(0, x, y, 1024, 700),
            vec![ChromeIntent::MenuChanged]
        );
    }
    assert_eq!(chrome.runner.state().relationships_start, 12);
    let row = chrome.runner.state().connections.last().unwrap().clone();
    let (x, y) = toolbar_point(
        &chrome,
        &format!("{} · {}: {}", row.relationship, row.address, row.title),
        1024,
        700,
    );
    let intents = chrome.click(0, x, y, 1024, 700);
    let [ChromeIntent::FocusCard(crate::focus_card::Command::Peek(target))] = intents.as_slice()
    else {
        panic!("expected focus card: {intents:?}")
    };
    let target = target.clone();
    app.update(Action::FocusCard(crate::focus_card::Command::Peek(
        target.clone(),
    )));
    chrome.sync(&app, &[(0, 1024.0, 700.0)]);
    assert_eq!(
        app.graph_pane_focused_member(pane),
        Some(source),
        "peek leaves the source selected"
    );
    let (x, y) = toolbar_point(&chrome, &format!("Read: {}", row.title), 1024, 700);
    assert_eq!(
        chrome.click(0, x, y, 1024, 700),
        vec![ChromeIntent::FocusCard(crate::focus_card::Command::Read(
            target
        ))]
    );
    assert_eq!(row.target.member, *members.last().unwrap());
    assert_eq!(row.relationship, "Cites");
    assert_eq!(app.graph_runtimes.graph().nodes().count(), 16);
}

#[test]
fn toolbar_connections_come_from_graph_edges_and_keep_member_identity() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/a".into()));
    let a = app.graph_runtimes.focused_member().unwrap();
    let pane = app.default_graph_pane();
    let b = app
        .with_graph_pane(pane, |canvas| {
            canvas.open_member_as_new_node(Some(a), "https://example.test/b")
        })
        .unwrap();
    assert_eq!(app.graph_pane_focused_member(pane), Some(b));
    assert!(
        app.graph_runtimes
            .graph()
            .projected_relations()
            .next()
            .is_some(),
        "fixture has a held relationship"
    );
    app.update(Action::SetWorkspaceMap(false));
    assert_eq!(app.browser_command_target().map(|(id, _)| id), Some(b));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(
        chrome
            .runner
            .state()
            .connections
            .iter()
            .any(|row| row.target.member == a)
    );
    app.update(Action::ReadGraphMember(a));
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(
        chrome
            .runner
            .state()
            .connections
            .iter()
            .any(|row| row.target.member == b)
    );
    assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
}

#[test]
fn graph_navigation_lists_unarranged_members_and_opens_their_existing_identity() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/reading".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::ReadGraphMember(first));
    let map = app.default_graph_pane();
    let other = app
        .with_graph_pane(map, |canvas| {
            canvas.open_member_as_new_node(Some(first), "https://example.test/unarranged")
        })
        .unwrap();
    app.update(Action::WorkbenchActivate(first));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(
        chrome
            .runner
            .state()
            .documents
            .iter()
            .any(|(id, _, arranged)| *id == other && !arranged)
    );
    let (x, y) = toolbar_point(&chrome, "Documents in this map", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    let label = chrome
        .runner
        .state()
        .documents
        .iter()
        .find(|(id, _, _)| *id == other)
        .unwrap()
        .1
        .clone();
    let (x, y) = toolbar_point(&chrome, &label, 1024, 600);
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::ReadMember(other)]
    );
    app.update(Action::ReadGraphMember(other));
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(
        chrome
            .runner
            .state()
            .documents
            .iter()
            .any(|(id, _, arranged)| *id == other && *arranged)
    );
    assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
}

#[test]
fn graph_navigation_can_reach_members_beyond_the_first_menu_page() {
    let mut app = App::test_stub();
    for index in 0..19 {
        app.update(Action::OpenAddress(format!(
            "https://example.test/document-{index:02}"
        )));
    }
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let last = chrome.runner.state().documents.last().unwrap().clone();
    let (x, y) = toolbar_point(&chrome, "Documents in this map", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    for _ in 0..2 {
        let (x, y) = toolbar_point(&chrome, "More documents", 1024, 600);
        chrome.click(0, x, y, 1024, 600);
    }
    let (x, y) = toolbar_point(&chrome, &last.1, 1024, 600);
    assert!(
        x < 1024.0 && y < 600.0,
        "later members remain within the menu viewport"
    );
    let (x, y) = toolbar_point(&chrome, "Previous documents", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    let earlier = chrome.runner.state().documents[8].clone();
    let (x, y) = toolbar_point(&chrome, &earlier.1, 1024, 600);
    assert!(y < 600.0 && x < 1024.0);
    let (x, y) = toolbar_point(&chrome, "More documents", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    let (x, y) = toolbar_point(&chrome, &last.1, 1024, 600);
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::ReadMember(last.0)]
    );
}
