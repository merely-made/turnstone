// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn opening_saved_source_uses_the_focused_panes_graph_not_the_compatibility_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let source_graph = app.graph_runtimes.active_graph();
    let source_node = visit(&mut app, "https://saved-source.test/pane-source");
    let annotation = eidetic::Hash::of(b"pane-capture").to_string();
    attach(&mut app, source_graph, source_node, &annotation);
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, _) = target(key, "A 🐦 pane-owned passage.");
    let saved_url = crate::saved_sources::address(&target).unwrap();

    // Pane 0 remains focused on the admitted source graph. Make a second
    // graph active only through the legacy runtime cursor and give it the
    // same local node-key shape, so a compatibility lookup selects the wrong
    // node if the open path forgets its pane owner.
    let cursor_graph = crate::panes::GraphId::new();
    let mut cursor_canvas = Canvas::new();
    let cursor_key = cursor_canvas.visit("https://cursor-only.test/decoy");
    let cursor_node = cursor_canvas.graph().get_node(cursor_key).unwrap().id;
    app.graph_runtimes
        .activate_or_insert(cursor_graph, None, cursor_canvas);
    assert!(app.graph_runtimes.activate(cursor_graph));
    assert_eq!(app.focused_graph_pane(), Some(crate::panes::PaneId(0)));
    assert_eq!(
        app.graph_for_pane(crate::panes::PaneId(0)),
        Some(source_graph)
    );
    assert_eq!(app.graph_runtimes.active_graph(), cursor_graph);

    let effects = app.update(Action::OpenSavedSource(target.clone()));
    let reader_node = app
        .graph_runtimes
        .canvas(source_graph)
        .and_then(|canvas| canvas.graph().get_node_by_url(&saved_url))
        .map(|(_, node)| node.id)
        .expect("the saved reader was visited in the focused pane's graph");
    assert_ne!(reader_node, cursor_node);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node, url } if *node == reader_node && url == &saved_url
    )));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node, .. } if *node == cursor_node
    )));
    assert!(
        app.browser
            .get(reader_node)
            .is_some_and(|state| state.content_on)
    );
    assert!(
        !app.browser
            .get(cursor_node)
            .is_some_and(|state| state.content_on)
    );

    // A pane actually owned by another session cannot host the private reader,
    // even while the compatibility cursor remains a different graph.
    let foreign_graph = crate::panes::GraphId::new();
    app.graph_runtimes.activate_or_insert(
        foreign_graph,
        Some(crate::panes::SessionId::new()),
        Canvas::new(),
    );
    assert!(app.graph_runtimes.activate(cursor_graph));
    app.frisket.root = crate::panes::PaneNode::Leaf {
        pane_id: crate::panes::PaneId(0),
        content: crate::panes::PaneContent::Orrery,
        graph_id: foreign_graph,
    };
    app.index_pane_spaces();
    assert_eq!(
        app.graph_for_pane(crate::panes::PaneId(0)),
        Some(foreign_graph)
    );
    assert_eq!(app.graph_runtimes.active_graph(), cursor_graph);
    let refused = app.update(Action::OpenSavedSource(target));
    assert!(!refused.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { .. }
            | Effect::ReadSavedSource { .. }
            | Effect::FetchPage { .. }
            | Effect::FetchFeed { .. }
            | Effect::FetchFavicon { .. }
    )));
}

#[test]
fn opening_saved_source_from_answers_reveals_and_focuses_the_graph_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let node = visit(&mut app, "https://gone.test/citation");
    attach(
        &mut app,
        graph,
        node,
        &eidetic::ManifestId::of_blob(b"A1").to_string(),
    );
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, _) = target(key, "An exact 🐦 saved passage.");
    let url = crate::saved_sources::address(&target).unwrap();
    app.update(Action::OpenAnswers);
    let answer_pane = app.active_pane.unwrap();
    app.focus = crate::surface::FocusTarget::Pane(answer_pane);
    app.maximized = Some(answer_pane);
    let effects = app.update(Action::OpenSavedSource(target));
    let pane = app.default_graph_pane();
    assert_eq!(app.active_pane, Some(pane));
    assert_eq!(app.focus, crate::surface::FocusTarget::Graph(pane));
    assert_eq!(app.maximized, Some(pane));
    let reader = app
        .graph_pane_focused_member(pane)
        .expect("the reader is focused in its owning graph");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::SpawnContent { node, url: actual } if *node == reader && *actual == url)));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFavicon { .. }
    )));
}

#[test]
fn direct_saved_citation_address_reveals_and_focuses_the_graph_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let node = visit(&mut app, "https://gone.test/citation");
    attach(
        &mut app,
        graph,
        node,
        &eidetic::ManifestId::of_blob(b"A1").to_string(),
    );
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, _) = target(key, "An exact 🐦 saved passage.");
    let url = crate::saved_sources::address(&target).unwrap();
    app.update(Action::OpenAnswers);
    let answer_pane = app.active_pane.unwrap();
    app.focus = crate::surface::FocusTarget::Pane(answer_pane);
    app.maximized = Some(answer_pane);
    let effects = app.update(Action::OpenAddress(url.clone()));
    let pane = app.default_graph_pane();
    assert_eq!(app.active_pane, Some(pane));
    assert_eq!(app.focus, crate::surface::FocusTarget::Graph(pane));
    assert_eq!(app.maximized, Some(pane));
    let reader = app
        .graph_pane_focused_member(pane)
        .expect("the reader is focused in its owning graph");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::SpawnContent { node, url: actual } if *node == reader && *actual == url)));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFavicon { .. }
    )));
}
