// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn graph_navigation_read_adds_a_tab_and_keeps_existing_arrangements() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::ReadGraphMember(first));
    let reader = app.active_pane.unwrap();
    let map = app.default_graph_pane();
    let second = app
        .with_graph_pane(map, |canvas| {
            canvas.open_member_as_new_node(Some(first), "https://example.test/second")
        })
        .unwrap();
    let graph_before = serde_json::to_value(app.graph_runtimes.graph().to_snapshot()).unwrap();
    app.pane_context = Default::default();
    app.update(Action::ReadGraphMember(second));
    let wb = app.workbench_for_pane(reader).unwrap();
    assert_eq!(wb.tile_count(), 2);
    assert_eq!(
        wb.slot_count(),
        1,
        "following a member uses the reading stack"
    );
    assert_eq!(app.browser_command_target().unwrap().0, second);
    app.update(Action::WorkbenchSplitOut {
        dragged: second,
        axis: crate::action::WbAxis::Row,
        after: true,
    });
    assert_eq!(app.workbench_for_pane(reader).unwrap().slot_count(), 2);
    app.update(Action::ReadGraphMember(first));
    assert_eq!(app.workbench_for_pane(reader).unwrap().slot_count(), 2);
    app.update(Action::RevealGraphMember(second));
    assert_eq!(app.active_pane, Some(map));
    assert_eq!(app.graph_pane_focused_member(map), Some(second));
    assert_eq!(app.workbench_for_pane(reader).unwrap().slot_count(), 2);
    assert_eq!(
        serde_json::to_value(app.graph_runtimes.graph().to_snapshot()).unwrap(),
        graph_before,
        "reading and revealing project existing graph data"
    );
}

#[test]
fn graph_navigation_fit_after_read_uses_the_new_pane_aperture() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    let pane = app.default_graph_pane();
    app.with_graph_pane(pane, |canvas| {
        canvas.open_member_as_new_node(Some(first), "https://example.test/second");
        canvas.set_physics_paused(true);
        canvas.set_layout_strategy(None);
    });
    app.graph_pane_frame(pane, 1024, 521).unwrap();
    app.update(Action::SetWorkspaceMap(false));
    app.update(Action::SetWorkspaceMap(true));
    app.update(Action::FitView);
    app.graph_pane_frame(pane, 638, 521).unwrap();
    app.with_graph_pane(pane, |canvas| {
        assert_eq!(canvas.viewport().view, (638, 521));
        let actual = canvas.viewport();
        canvas.fit_visible();
        let expected = canvas.viewport();
        assert_eq!(actual.offset, expected.offset);
        assert_eq!(actual.zoom, expected.zoom);
    });
    assert!(
        !app.graph_views.take_fit(pane),
        "one explicit fit is consumed by one frame"
    );
}
