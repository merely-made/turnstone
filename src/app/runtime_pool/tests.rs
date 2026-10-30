// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn graph(n: u128) -> GraphId {
    GraphId::from_uuid(uuid::Uuid::from_u128(n))
}

#[test]
fn display_budget_survives_graph_replacement_and_unknown_rate() {
    let a = graph(901);
    let b = graph(902);
    let assert_budget = |pool: &GraphRuntimePool, id, rate| {
        let actual = pool.canvas(id).unwrap().physics_step_budget().unwrap();
        let expected = mere::canvas::StepBudget::for_display(rate);
        assert_eq!(actual.per_frame, expected.per_frame);
        assert_eq!(actual.margin, expected.margin);
    };
    let mut pool = GraphRuntimePool::new(a, None, Canvas::new());
    pool.set_physics_display_rate(Some(144_000));
    pool.activate_or_insert(b, None, Canvas::new());
    assert_budget(&pool, a, Some(144_000));
    assert_budget(&pool, b, Some(144_000));
    pool.activate_or_insert(a, None, Canvas::new());
    assert_budget(&pool, a, Some(144_000));
    pool.set_physics_display_rate(None);
    for id in [a, b] {
        assert_budget(&pool, id, None);
    }
}

#[test]
fn graph_pool_keeps_two_graph_authorities_independent() {
    let a = graph(1);
    let b = graph(2);
    let mut pool = GraphRuntimePool::new(a, None, Canvas::new());
    pool.active_canvas_mut().visit("https://a.example/");
    pool.activate_or_insert(b, None, Canvas::new())
        .canvas
        .visit("https://b.example/");

    assert_eq!(pool.len(), 2);
    assert!(
        pool.canvas(a)
            .expect("graph A")
            .graph()
            .get_node_by_url("https://a.example/")
            .is_some()
    );
    assert!(
        pool.canvas(a)
            .expect("graph A")
            .graph()
            .get_node_by_url("https://b.example/")
            .is_none()
    );
    assert!(
        pool.canvas(b)
            .expect("graph B")
            .graph()
            .get_node_by_url("https://b.example/")
            .is_some()
    );
}

#[test]
fn pane_viewports_do_not_alias_on_one_graph() {
    let a = graph(1);
    let first = PaneId(10);
    let second = PaneId(11);
    let mut pool = GraphRuntimePool::new(a, None, Canvas::with_sample_graph());
    let mut views = GraphPaneViews::default();
    let canvas = pool.canvas_mut(a).expect("graph runtime");

    views.install(first, canvas);
    views.stash(first, canvas);
    views.install(second, canvas);
    views.stash(second, canvas);
    views.install(first, canvas);
    canvas.wheel(0.0, 240.0);
    views.stash(first, canvas);
    let first_view = views.viewport(first).expect("first view");

    views.install(second, canvas);
    let second_view = views.viewport(second).expect("second view");
    assert_ne!(first_view, second_view, "pane-local views do not alias");

    views.install(first, canvas);
    assert_eq!(canvas.viewport(), first_view);
}

#[test]
fn pane_selections_do_not_alias_on_one_graph() {
    let a = graph(1);
    let first = PaneId(10);
    let second = PaneId(11);
    let mut pool = GraphRuntimePool::new(a, None, Canvas::new());
    let one = pool.active_canvas_mut().visit("https://one.example/");
    let one = pool.active_canvas().graph().get_node(one).expect("one").id;
    let two = pool.active_canvas_mut().visit("https://two.example/");
    let two = pool.active_canvas().graph().get_node(two).expect("two").id;
    let mut views = GraphPaneViews::default();
    let canvas = pool.canvas_mut(a).expect("graph runtime");

    views.install(first, canvas);
    canvas.select_member(one);
    views.stash(first, canvas);
    views.install(second, canvas);
    canvas.select_member(two);
    views.stash(second, canvas);

    assert_eq!(views.focused_member(first), Some(one));
    assert_eq!(views.focused_member(second), Some(two));
    views.install(first, canvas);
    assert_eq!(canvas.focused_member(), Some(one));
}

#[test]
fn forme_runtime_key_keeps_graph_and_forme_separate() {
    let graph_a = graph(1);
    let graph_b = graph(2);
    let forme = FormeRef::Stored(mere::forme::FormeId::from_uuid(uuid::Uuid::from_u128(7)));
    let mut pool = FormeRuntimePool::default();

    pool.get_or_create(graph_a, forme);
    pool.get_or_create(graph_b, forme);

    assert_eq!(pool.len(), 2);
    assert!(pool.get(graph_a, forme).is_some());
    assert!(pool.get(graph_b, forme).is_some());
}

#[test]
fn workbenches_are_owned_by_their_forme_runtime() {
    let graph_a = graph(1);
    let graph_b = graph(2);
    let forme = FormeRef::Stored(mere::forme::FormeId::from_uuid(uuid::Uuid::from_u128(7)));
    let member = uuid::Uuid::from_u128(42);
    let mut pool = FormeRuntimePool::default();

    pool.get_or_create(graph_a, forme)
        .workbench
        .open_tile(member);
    pool.get_or_create(graph_b, forme);

    assert!(pool.get(graph_a, forme).unwrap().workbench.has_tile(member));
    assert!(!pool.get(graph_b, forme).unwrap().workbench.has_tile(member));
}
