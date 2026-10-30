use super::*;
#[test]
fn nested_contexts_survive_reading_visits_and_up_follows_the_chosen_route() {
    let (mut app, a, b, _) = fixture();
    let mut visit = app.reading.scope.navigation.current().unwrap().clone();
    let graph = visit.place.graph;
    visit.place.root = Some(b);
    visit.place.contexts = Some(vec![
        ScopeContext {
            level: ScopeLevel::Graph,
            graph,
            root: None,
        },
        ScopeContext {
            level: ScopeLevel::Neighborhood,
            graph,
            root: Some(a),
        },
        ScopeContext {
            level: ScopeLevel::Neighborhood,
            graph,
            root: Some(b),
        },
    ]);
    app.reading.scope.navigation.capture(visit);
    command(&mut app, ScopeDirection::Up);
    assert_eq!(app.scope_place().root, Some(b));
    command(&mut app, ScopeDirection::Up);
    assert_eq!(app.scope_place().root, Some(a));
    assert_eq!(app.scope_place().focus, Some(a));
    command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().focus, Some(a));
}
#[test]
fn replacing_the_live_canvas_cannot_complete_an_earlier_click_on_the_same_document() {
    let (mut app, a, _, _) = fixture();
    let graph = app.scope_place().graph;
    let before = app.graph_runtimes.get(graph).unwrap().instance;
    let mut activation = cambium::scope_navigation::NodeActivation::default();
    activation.press(Some((before, a)), (0., 0.), false);
    assert_eq!(activation.release(Some((before, a)), (0., 0.), 10.), None);
    let canvas = mere::canvas::Canvas::new();
    app.graph_runtimes.replace_active_canvas(canvas);
    let after = app.graph_runtimes.get(graph).unwrap().instance;
    assert_ne!(before, after);
    activation.press(Some((after, a)), (0., 0.), false);
    assert_eq!(activation.release(Some((after, a)), (0., 0.), 100.), None);
}
#[test]
fn close_prefers_the_remaining_tab_in_its_own_stack_over_another_cell() {
    let (mut app, a, b, c) = fixture();
    let reader = app.active_pane.unwrap();
    app.update(Action::ReadGraphMember(b));
    app.workbench_for_pane_mut(reader).unwrap().open_tile(c);
    app.update(Action::WorkbenchActivate(b));
    app.update(Action::Reading(crate::reading_view::ReadAction::Close(
        ReadTarget {
            pane: reader,
            member: b,
        },
    )));
    assert_eq!(
        app.browser_command_target().map(|(member, _)| member),
        Some(a)
    );
    assert!(app.workbench_for_pane(reader).unwrap().has_tile(c));
}
#[test]
fn location_search_from_a_map_opens_a_reading_and_offers_structural_scopes() {
    let (mut app, _, b, _) = fixture();
    command(&mut app, ScopeDirection::Up);
    command(&mut app, ScopeDirection::Up);
    app.update(Action::Scope(ScopeCommand::EditLocation));
    app.update(Action::OmnibarInsert("https://scope.test/b".into()));
    app.update(Action::OmnibarCommit);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(b));
    assert_eq!(app.graph_runtimes.graph().node_count(), 3);
    app.update(Action::Scope(ScopeCommand::EditLocation));
    app.update(Action::OmnibarInsert("Meres".into()));
    app.update(Action::OmnibarCommit);
    assert_eq!(app.scope_place().level, ScopeLevel::Overmap);
    command(&mut app, ScopeDirection::Back);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(b));
}
#[test]
fn reading_another_member_records_the_origin_before_revealing_the_destination() {
    let (mut app, a, b, _) = fixture();
    app.update(Action::ReadGraphMember(b));
    assert_eq!(app.scope_place().focus, Some(b));
    command(&mut app, ScopeDirection::Back);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(a));
    command(&mut app, ScopeDirection::Forward);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(b));
}
#[test]
fn accepting_a_location_from_reading_preserves_back_and_forward_between_documents() {
    let (mut app, a, b, _) = fixture();
    app.update(Action::Scope(ScopeCommand::EditLocation));
    app.update(Action::OmnibarInsert("https://scope.test/b".into()));
    app.update(Action::OmnibarCommit);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(b));
    command(&mut app, ScopeDirection::Back);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(a));
    command(&mut app, ScopeDirection::Forward);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(b));
}
#[test]
fn editing_location_cancels_without_changing_scope_history() {
    let (mut app, _, _, _) = fixture();
    let history = app.reading.scope.navigation.clone();
    let place = app.scope_place();
    app.update(Action::Scope(ScopeCommand::EditLocation));
    assert!(app.omnibar.open);
    app.update(Action::OmnibarInsert("uncommitted address".into()));
    app.update(Action::OmnibarClose);
    assert_eq!(app.scope_place(), place);
    assert_eq!(app.reading.scope.navigation, history);
}
#[test]
fn close_controls_preserve_documents_and_return_the_last_reading_to_its_map() {
    let (mut app, a, b, _) = fixture();
    let reader = app.active_pane.unwrap();
    app.update(Action::ReadGraphMember(b));
    app.update(Action::Reading(crate::reading_view::ReadAction::Close(
        ReadTarget {
            pane: reader,
            member: a,
        },
    )));
    assert_eq!(app.scope_place().focus, Some(b));
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert!(!app.workbench_for_pane(reader).unwrap().has_tile(a));
    assert!(app.workbench_for_pane(reader).unwrap().has_tile(b));
    app.update(Action::Reading(crate::reading_view::ReadAction::Close(
        ReadTarget {
            pane: reader,
            member: b,
        },
    )));
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    assert_eq!(app.scope_place().focus, Some(b));
    assert_eq!(app.workbench_for_pane(reader).unwrap().tile_count(), 0);
    assert_eq!(app.graph_runtimes.graph().node_count(), 3);
    command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(b));
}
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

#[test]
fn scope_crumb_resolves_its_real_route_after_generic_navigation_adoption() {
    let (mut app, member, _, _) = fixture();
    let graph = app.scope_crumbs().into_iter().find(|crumb| {
        matches!(&crumb.target, ScopeDestination::Place(place) if place.level == ScopeLevel::Graph)
    }).unwrap();
    app.update(Action::Scope(ScopeCommand::To(graph.target)));
    let place = app.scope_place();
    assert_eq!(place.level, ScopeLevel::Graph);
    assert_eq!(place.focus, Some(member));
    assert_eq!(place.up().unwrap().level, ScopeLevel::Overmap);
    command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    command(&mut app, ScopeDirection::Down);
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(app.scope_place().focus, Some(member));
}
