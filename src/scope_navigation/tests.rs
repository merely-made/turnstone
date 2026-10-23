use super::*;
fn command(app: &mut App, direction: ScopeDirection) -> Vec<Effect> {
    app.update(Action::Scope(ScopeCommand::Navigate(direction)))
}
fn fixture() -> (App, Uuid, Uuid, Uuid) {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://scope.test/a".into()));
    let pane = app.default_graph_pane();
    let a = app.graph_pane_focused_member(pane).unwrap();
    let b = app
        .with_graph_pane(pane, |c| {
            c.open_member_as_new_node(None, "https://scope.test/b")
        })
        .unwrap();
    let c = app
        .with_graph_pane(pane, |c| {
            c.open_member_as_new_node(None, "https://scope.test/outside")
        })
        .unwrap();
    app.with_graph_pane(pane, |canvas| {
        canvas.ingest_graph(|graph| {
            let ak = graph.get_node_by_id(a).unwrap().0;
            let bk = graph.get_node_by_id(b).unwrap().0;
            mere::kernel::graph::apply::assert_relation(
                graph,
                ak,
                bk,
                mere::kernel::graph::EdgeAssertion::Semantic {
                    sub_kind: mere::kernel::graph::SemanticSubKind::Cites,
                    label: None,
                },
            )
            .unwrap();
            true
        });
    });
    app.update(Action::ReadGraphMember(a));
    (app, a, b, c)
}
#[test]
fn up_down_keeps_reading_anchor_and_neighborhood_does_not_edit_graph() {
    let (mut app, a, _, outside) = fixture();
    let reader = app.active_pane.unwrap();
    let anchor = tabard::ReadingAnchor {
        source: "https://scope.test/a".into(),
        revision: 42,
        text: 3,
        offset: 0,
        exact: "A retained passage".into(),
        prefix: String::new(),
        suffix: String::new(),
        line_offset: 0.0,
        fallback_y: 600.0,
    };
    app.reading.anchors.insert(
        ReadTarget {
            pane: reader,
            member: a,
        },
        anchor.clone(),
    );
    command(&mut app, ScopeDirection::Up);
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    let graph = app.scope_place().graph;
    let pane = app.graph_pane_for_navigation(graph).unwrap();
    app.with_graph_pane(pane, |canvas| {
        assert_eq!(canvas.graph().nodes().count(), 3);
        assert!(!canvas.node_in_scope(canvas.graph().get_node_by_id(outside).unwrap().0));
    });
    let effects = command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(a));
    assert!(effects.contains(&Effect::RestoreReadingAnchor {
        target: ReadTarget {
            pane: reader,
            member: a
        },
        anchor
    }));
}
#[test]
fn selected_child_and_temporal_previous_have_different_destinations() {
    let (mut app, _, b, _) = fixture();
    command(&mut app, ScopeDirection::Up);
    command(&mut app, ScopeDirection::Up);
    let pane = app.active_pane.unwrap();
    app.graph_pane_select_member(pane, b);
    command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().root, Some(b));
    assert_eq!(
        app.scope_place().down().unwrap().level,
        ScopeLevel::Document
    );
    command(&mut app, ScopeDirection::Back);
    assert_eq!(app.scope_place().level, ScopeLevel::Graph);
    command(&mut app, ScopeDirection::Forward);
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().focus, Some(b));
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
}
#[test]
fn camera_is_restored_per_scope_and_routes_survive_cold_load() {
    let (mut app, _, _, _) = fixture();
    command(&mut app, ScopeDirection::Up);
    let pane = app.active_pane.unwrap();
    let camera = mere::canvas::CameraView {
        offset: (120.0, 70.0),
        zoom: 2.0,
    };
    app.with_graph_pane(pane, |c| c.set_camera(camera));
    command(&mut app, ScopeDirection::Up);
    command(&mut app, ScopeDirection::Down);
    let restored = app.with_graph_pane(pane, |c| c.camera()).unwrap();
    assert_eq!(restored, camera);
    let root = tempfile::tempdir().unwrap();
    app.reading.save(root.path()).unwrap();
    let cold = crate::reading_view::ReadingState::load(root.path());
    assert!(cold.load_error.is_none());
    assert_eq!(cold.scope.navigation, app.reading.scope.navigation);
}
#[test]
fn stale_destination_does_not_advance_history() {
    let (mut app, _, _, _) = fixture();
    command(&mut app, ScopeDirection::Up);
    let mut nav = app.reading.scope.navigation.clone();
    let mut stale = nav.current().unwrap().clone();
    stale.place.graph = GraphId::nil();
    nav.visit(stale);
    nav.step(false);
    app.reading.scope.navigation = nav;
    let before = app
        .reading
        .scope
        .navigation
        .current()
        .unwrap()
        .place
        .clone();
    command(&mut app, ScopeDirection::Forward);
    assert_eq!(
        app.reading.scope.navigation.current().unwrap().place,
        before
    );
}

#[test]
fn neighborhood_is_live_and_does_not_leak_to_a_second_pane() {
    let (mut app, a, _, outside) = fixture();
    let graph = app.scope_place().graph;
    let canvas = app.graph_runtimes.canvas_mut(graph).unwrap();
    let first = PaneId(100);
    let second = PaneId(101);
    let mut views = crate::app::GraphPaneViews::default();
    views.set_neighborhood(first, a);
    views.install(first, canvas);
    views.stash(first, canvas);
    assert_eq!(canvas.scope_members().unwrap().len(), 2);
    views.install(second, canvas);
    assert!(canvas.scope_members().is_none());
    views.stash(second, canvas);
    canvas.ingest_graph(|graph| {
        let ak = graph.get_node_by_id(a).unwrap().0;
        let ck = graph.get_node_by_id(outside).unwrap().0;
        mere::kernel::graph::apply::assert_relation(
            graph,
            ak,
            ck,
            mere::kernel::graph::EdgeAssertion::Semantic {
                sub_kind: mere::kernel::graph::SemanticSubKind::Cites,
                label: None,
            },
        )
        .unwrap();
        true
    });
    views.install(first, canvas);
    assert_eq!(canvas.scope_members().unwrap().len(), 3);
    views.install(second, canvas);
    assert!(canvas.scope_members().is_none());
}

#[test]
fn selecting_a_mere_and_entering_it_preserves_cross_session_history() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().into());
    let old = app.session_id;
    let old_graph = app.graph_runtimes.active_graph();
    app.sessions
        .insert(pandect::GraphSessionManifest::new(old, old_graph));
    let new = crate::panes::SessionId::new();
    let new_graph = GraphId::from_uuid(*new.as_uuid());
    app.sessions
        .insert(pandect::GraphSessionManifest::new(new, new_graph));
    command(&mut app, ScopeDirection::Up);
    assert_eq!(app.scope_place().level, ScopeLevel::Overmap);
    app.update(Action::SelectScopeMere(new));
    assert_eq!(app.session_id, old);
    assert_eq!(app.scope_place().down().unwrap().graph, new_graph);
    let effects = command(&mut app, ScopeDirection::Down);
    assert!(effects.contains(&Effect::SwitchSession { id: new }));
    assert_eq!(
        app.reading.scope.navigation.current().unwrap().place.level,
        ScopeLevel::Overmap
    );
    app.adopt_session(new);
    assert_eq!(app.scope_place().level, ScopeLevel::Graph);
    command(&mut app, ScopeDirection::Back);
    assert_eq!(app.scope_place().level, ScopeLevel::Overmap);
    let effects = command(&mut app, ScopeDirection::Back);
    assert!(effects.contains(&Effect::SwitchSession { id: old }));
    app.adopt_session(old);
    assert_eq!(app.scope_place().graph, old_graph);
    assert_eq!(app.scope_place().level, ScopeLevel::Graph);
    assert!(app.reading.scope.navigation.destination(true).is_some());
}

#[test]
fn a_cold_current_visit_uses_the_latest_saved_reading_position() {
    let (mut app, a, _, _) = fixture();
    let pane = app.active_pane.unwrap();
    let target = ReadTarget { pane, member: a };
    let mut anchor = tabard::ReadingAnchor {
        source: "https://scope.test/a".into(),
        revision: 42,
        text: 3,
        offset: 0,
        exact: "An earlier passage".into(),
        prefix: String::new(),
        suffix: String::new(),
        line_offset: 0.0,
        fallback_y: 300.0,
    };
    app.reading.anchors.insert(target, anchor.clone());
    app.retain_scope_anchor(target, &anchor);
    anchor.text = 9;
    anchor.exact = "The latest saved passage".into();
    anchor.fallback_y = 900.0;
    app.reading.anchors.insert(target, anchor.clone());
    let root = tempfile::tempdir().unwrap();
    app.reading.save(root.path()).unwrap();
    app.reading = crate::reading_view::ReadingState::load(root.path());
    let effects = app.restore_scope_after_session();
    assert!(effects.contains(&Effect::RestoreReadingAnchor {
        target,
        anchor: anchor.clone()
    }));
    assert_eq!(
        app.reading
            .scope
            .navigation
            .current()
            .unwrap()
            .anchor
            .as_ref()
            .unwrap()
            .reading,
        Some(anchor)
    );
}

#[test]
fn legacy_scope_anchors_load_and_malformed_bookmarks_are_not_silently_dropped() {
    let anchor = tabard::ReadingAnchor {
        source: "https://scope.test/a".into(),
        revision: 42,
        text: 3,
        offset: 0,
        exact: "A retained passage".into(),
        prefix: String::new(),
        suffix: String::new(),
        line_offset: 0.0,
        fallback_y: 300.0,
    };
    let legacy = serde_json::to_value(&anchor).unwrap();
    let bookmark: ScopeBookmark = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(bookmark.reading, Some(anchor));
    let mut invalid = legacy;
    invalid["text"] = serde_json::json!({"invalid":"number"});
    assert!(serde_json::from_value::<ScopeBookmark>(invalid).is_err());
}
