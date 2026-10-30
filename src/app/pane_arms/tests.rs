// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn summoning_a_tool_reveals_it_from_a_maximized_reader_without_growing_on_reentry() {
    let mut app = App::test_stub();
    let graph = app.default_graph_pane();
    app.maximized = Some(graph);
    app.update(Action::SummonPane(PaneKindId::new(kind::INSPECTOR)));
    let inspector = app.active_pane.unwrap();
    assert_ne!(inspector, graph);
    assert_eq!(app.maximized, Some(inspector));
    let count = app.frisket.iter_leaves().count();
    app.reveal_pane(graph);
    app.update(Action::SummonPane(PaneKindId::new(kind::INSPECTOR)));
    assert_eq!(app.active_pane, Some(inspector));
    assert_eq!(app.maximized, Some(inspector));
    assert_eq!(app.frisket.iter_leaves().count(), count);
}

#[test]
fn floating_pane_moves_through_live_primary_and_lens_projections() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(PaneKindId::new(kind::ROSTER)));
    let roster = app.active_pane.expect("summoned roster is active");

    app.update(Action::FloatActivePane);
    assert!(app.frisket.outer_workspace().ok().is_some_and(|frame| {
        frame
            .workspace
            .floating()
            .iter()
            .any(|f| f.tile.id.0 == roster.0)
    }));

    app.update(Action::DockActivePane);
    assert!(app.frisket.outer_workspace().is_ok_and(|frame| {
        frame.workspace.floating().is_empty()
            && frame
                .workspace
                .tiled()
                .find(workbench::TileId(roster.0))
                .is_some()
    }));

    app.update(Action::FloatActivePane);
    app.update(Action::TearOutActivePane);
    assert_eq!(app.space_of(roster), Some(SpaceRef::Lens(0)));
    assert!(
        app.space(SpaceRef::Lens(0))
            .and_then(|s| s.outer_workspace().ok())
            .is_some_and(|frame| frame
                .workspace
                .floating()
                .iter()
                .any(|f| f.tile.id.0 == roster.0))
    );

    app.update(Action::ReturnActivePaneToPrimary);
    assert_eq!(app.space_of(roster), Some(SpaceRef::Primary));
    assert!(app.frisket.outer_workspace().ok().is_some_and(|frame| {
        frame
            .workspace
            .floating()
            .iter()
            .any(|f| f.tile.id.0 == roster.0)
    }));
    let events: Vec<_> = app
        .take_events()
        .into_iter()
        .map(|event| event.describe())
        .collect();
    assert!(events.iter().any(|event| event == "pane-floated roster"));
    assert!(events.iter().any(|event| event == "pane-docked roster"));
    assert!(events.iter().any(|event| event == "pane-torn-out roster"));
    assert!(events.iter().any(|event| event == "pane-returned roster"));
}
