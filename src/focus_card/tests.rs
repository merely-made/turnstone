// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{
    action::{Action, Update},
    reading_view::ReadTarget,
};
fn fixture() -> (App, GraphId, Uuid, Uuid) {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://card.test/source".into()));
    let pane = app.default_graph_pane();
    let source = app.graph_pane_focused_member(pane).unwrap();
    let member = app
        .with_graph_pane(pane, |c| {
            c.open_member_as_new_node(Some(source), "https://card.test/reference")
        })
        .unwrap();
    let graph = app.graph_for_pane(pane).unwrap();
    app.update(Action::ReadGraphMember(source));
    let reader = app.active_pane.unwrap();
    app.reading.anchors.insert(
        ReadTarget {
            pane: reader,
            member: source,
        },
        tabard::ReadingAnchor {
            source: "https://card.test/source".into(),
            revision: 1,
            text: 2,
            offset: 3,
            exact: "keep this passage".into(),
            prefix: String::new(),
            suffix: String::new(),
            line_offset: 0.,
            fallback_y: 480.,
        },
    );
    (app, graph, source, member)
}
#[test]
fn field_entry_uses_the_existing_forme_and_palette_commands_preserve_readings() {
    let (mut app, _, source, member) = fixture();
    let reader = app.active_pane.unwrap();
    app.workbench_for_pane_mut(reader)
        .unwrap()
        .open_split(&[source, member]);
    let layout = app.workbench_for_pane(reader).unwrap().to_arrangement().1;
    let anchors = app.reading.anchors.clone();
    app.update(Action::SetWorkspaceMap(true));
    let map = app.active_pane.unwrap();
    app.graph_pane_frame(map, 1024, 600).unwrap();
    let target = app.forme_field_target(map).unwrap();
    let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
    let commands = app.available_actions();
    assert!(
        commands
            .iter()
            .any(|(label, action)| label == "Preview reading layout"
                && matches!(action, Action::FocusCard(Command::Peek(_))))
    );
    assert!(
        commands
            .iter()
            .any(|(label, action)| label == "Enter reading layout"
                && matches!(action, Action::FocusCard(Command::EnterLayout(_))))
    );
    app.update(Action::FocusCard(Command::Peek(target.clone())));
    assert_eq!(
        serde_json::to_value(&app.reading.scope.navigation).unwrap(),
        history
    );
    app.update(Action::FocusCard(Command::EnterLayout(target)));
    assert!(matches!(
        app.scope_place().level,
        crate::scope_navigation::ScopeLevel::Forme(_)
    ));
    assert_eq!(app.reading.scope.reader, Some(reader));
    assert_eq!(app.reading.anchors, anchors);
    assert_eq!(
        app.workbench_for_pane(reader).unwrap().to_arrangement().1,
        layout
    );
    app.update(Action::Scope(
        crate::scope_navigation::ScopeCommand::Navigate(
            cambium::scope_navigation::ScopeDirection::Down,
        ),
    ));
    assert_eq!(app.active_pane, Some(reader));
    assert_eq!(
        app.scope_place().level,
        crate::scope_navigation::ScopeLevel::Document
    );
    assert_eq!(app.reading.anchors, anchors);
}
#[test]
fn forme_preview_prefers_the_active_then_retained_reader_over_tree_order() {
    let (mut app, graph, _, _) = fixture();
    let reader = app.active_pane.unwrap();
    let other = crate::panes::PaneId(900);
    app.frisket.root = crate::panes::PaneNode::Split {
        axis: crate::panes::SplitAxis::Horizontal,
        ratio: 0.5,
        first: Box::new(crate::panes::PaneNode::Leaf {
            pane_id: other,
            content: crate::panes::PaneContent::Workbench,
            graph_id: graph,
        }),
        second: Box::new(app.frisket.root.clone()),
    };
    app.index_pane_spaces();
    assert_eq!(app.forme_card_target().unwrap().reader, Some(reader));
    app.update(Action::SetWorkspaceMap(true));
    assert_eq!(app.forme_card_target().unwrap().reader, Some(reader));
}
#[test]
fn forme_peek_enters_existing_access_and_round_trips_without_rearrangement() {
    use crate::scope_navigation::{ScopeCommand, ScopeLevel};
    use cambium::scope_navigation::ScopeDirection;
    let (mut app, graph, source, member) = fixture();
    let reader = app.active_pane.unwrap();
    app.workbench_for_pane_mut(reader)
        .unwrap()
        .open_split(&[source, member]);
    let layout = app.workbench_for_pane(reader).unwrap().to_arrangement().1;
    let anchors = app.reading.anchors.clone();
    app.update(Action::SetWorkspaceMap(true));
    let map = app.active_pane.unwrap();
    let camera = app.with_graph_pane(map, |c| c.camera()).unwrap();
    let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
    let target = app.forme_card_target().unwrap();
    app.update(Action::FocusCard(Command::Peek(target.clone())));
    app.update(Action::FocusCard(Command::Keep(target.clone())));
    assert_eq!(
        serde_json::to_value(&app.reading.scope.navigation).unwrap(),
        history
    );
    assert_eq!(app.reading.anchors, anchors);
    assert_eq!(
        app.workbench_for_pane(reader).unwrap().to_arrangement().1,
        layout
    );
    let card = app.focus_card_views().pop().unwrap();
    assert_eq!(card.forme.as_ref().unwrap().cells.len(), 2);
    let child = card
        .related
        .iter()
        .find(|(t, _, _)| t.member == Some(source))
        .unwrap()
        .0
        .clone();
    app.update(Action::FocusCard(Command::Read(child)));
    assert_eq!(app.active_pane, Some(reader));
    assert_eq!(app.scope_place().focus, Some(source));
    assert!(matches!(
        app.scope_place().up().unwrap().level,
        ScopeLevel::Forme(_)
    ));
    assert_eq!(app.reading.anchors, anchors);
    app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Up)));
    assert!(matches!(app.scope_place().level, ScopeLevel::Forme(_)));
    app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Up)));
    assert_eq!(app.scope_place().level, ScopeLevel::Graph);
    assert_eq!(app.scope_place().graph, graph);
    assert_eq!(app.with_graph_pane(map, |c| c.camera()).unwrap(), camera);
    app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Back)));
    assert!(matches!(app.scope_place().level, ScopeLevel::Forme(_)));
    app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Back)));
    assert_eq!(app.scope_place().focus, Some(source));
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert_eq!(
        app.workbench_for_pane(reader).unwrap().to_arrangement().1,
        layout
    );
    assert_eq!(app.reading.anchors, anchors);
}
#[test]
fn kept_forme_card_reads_live_layout_and_rejects_a_removed_tile_target() {
    let (mut app, _, source, member) = fixture();
    let reader = app.active_pane.unwrap();
    app.workbench_for_pane_mut(reader)
        .unwrap()
        .open_split(&[source, member]);
    let target = app.forme_card_target().unwrap();
    app.update(Action::FocusCard(Command::Peek(target.clone())));
    app.update(Action::FocusCard(Command::Keep(target)));
    let stale = app.focus_card_views()[0]
        .related
        .iter()
        .find(|(t, _, _)| t.member == Some(member))
        .unwrap()
        .0
        .clone();
    app.workbench_for_pane_mut(reader)
        .unwrap()
        .close_tile(member);
    let before = app.scope_place();
    app.update(Action::FocusCard(Command::Read(stale)));
    assert_eq!(app.scope_place(), before);
    assert!(!app.workbench_for_pane(reader).unwrap().has_tile(member));
    assert_eq!(
        app.focus_card_views()[0]
            .forme
            .as_ref()
            .unwrap()
            .cells
            .len(),
        1
    );
}

#[test]
fn lower_scope_peek_uses_downs_destination_without_visiting_it() {
    use crate::scope_navigation::{ScopeCommand, ScopeLevel};
    use cambium::scope_navigation::ScopeDirection;
    let (mut app, _, _, _) = fixture();
    app.update(Action::SetWorkspaceMap(true));
    let before = app.scope_place();
    let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
    let anchors = app.reading.anchors.clone();
    let target = app.lower_scope_card().unwrap();
    assert_eq!(target.origin, Origin::Scope(before.down().unwrap()));
    app.update(Action::FocusCard(Command::Peek(target.clone())));
    app.update(Action::FocusCard(Command::Keep(target.clone())));
    let views = app.focus_card_views();
    let swatch = views[0]
        .swatch
        .as_ref()
        .expect("a graph-bearing destination uses a swatch");
    assert_eq!(swatch.graph.nodes.len(), 2);
    for (child, _, _) in &views[0].related {
        let Origin::Scope(place) = &child.origin else {
            panic!("swatch children retain their contextual route")
        };
        assert_eq!(place.level, ScopeLevel::Document);
        assert_eq!(place.root, before.focus);
    }
    assert_eq!(app.scope_place(), before);
    assert_eq!(
        serde_json::to_value(&app.reading.scope.navigation).unwrap(),
        history
    );
    assert_eq!(app.reading.anchors, anchors);
    let pane = app.graph_pane_for_navigation(before.graph).unwrap();
    app.with_graph_pane(pane, |canvas| {
        canvas.open_member_as_new_node(before.focus, "https://card.test/new-neighbor");
    });
    app.graph_pane_select_member(pane, before.focus.unwrap());
    assert_eq!(
        app.focus_card_views()[0]
            .swatch
            .as_ref()
            .unwrap()
            .graph
            .nodes
            .len(),
        3,
        "keeping a scope holds its binding while membership stays live"
    );
    app.update(Action::FocusCard(Command::Read(target)));
    assert_eq!(app.scope_place(), before.down().unwrap());
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    let reading = app.lower_scope_card().unwrap();
    app.update(Action::FocusCard(Command::Peek(reading.clone())));
    assert!(app.focus_card_views().last().unwrap().swatch.is_none());
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    app.update(Action::FocusCard(Command::Read(reading)));
    assert_eq!(app.scope_place().level, ScopeLevel::Document);
    assert!(app.lower_scope_card().is_none());
    // History is still the ordinary compass history.
    app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Back)));
    assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
}

#[test]
fn node_relationship_and_inline_peeks_do_not_navigate_or_rearrange_the_reading() {
    let (mut app, graph, source, member) = fixture();
    let origin = app.scope_place();
    let focus = app.focus;
    let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
    let anchors = app.reading.anchors.clone();
    let reader = app.active_pane.unwrap();
    let members = app
        .workbench_for_pane(reader)
        .unwrap()
        .slot_views()
        .flat_map(|s| s.members.to_vec())
        .collect::<Vec<_>>();
    let relation = crate::relationship_navigation::relationships(
        graph,
        app.graph_runtimes.canvas(graph).unwrap().graph(),
        source,
    )
    .into_iter()
    .find(|r| r.target.member == member)
    .unwrap()
    .target;
    for origin in [
        Origin::Node,
        Origin::Relationship(relation),
        Origin::Inline(source),
    ] {
        let target = app.focus_card_target(graph, member, origin).unwrap();
        app.update(Action::FocusCard(Command::Peek(target.clone())));
        app.update(Action::FocusCard(Command::Keep(target.clone())));
        assert_eq!(app.focus, focus);
        assert_eq!(app.reading.anchors, anchors);
        assert_eq!(
            serde_json::to_value(&app.reading.scope.navigation).unwrap(),
            history
        );
        assert_eq!(
            app.workbench_for_pane(reader)
                .unwrap()
                .slot_views()
                .flat_map(|s| s.members.to_vec())
                .collect::<Vec<_>>(),
            members
        );
    }
    assert_eq!(app.scope_place(), origin);
    assert_eq!(app.focus_card_views().len(), 3);
    assert_eq!(
        app.graph_runtimes
            .canvas(graph)
            .unwrap()
            .graph()
            .node_count(),
        2
    );
    let target = app.reading.cards.kept[0].clone();
    app.update(Action::FocusCard(Command::Read(target)));
    assert_eq!(app.scope_place().focus, Some(member));
    assert_eq!(app.reading.anchors, anchors);
}
#[test]
fn a_saved_passage_from_another_address_is_never_presented_as_this_preview() {
    let (mut app, graph, source, _) = fixture();
    let target = app.focus_card_target(graph, source, Origin::Node).unwrap();
    app.update(Action::FocusCard(Command::Peek(target)));
    assert!(app.focus_card_views()[0].info.excerpt.is_some());
    for anchor in app.reading.anchors.values_mut() {
        anchor.source = "https://card.test/another-revision-source".into();
    }
    assert!(app.focus_card_views()[0].info.excerpt.is_none());
}

#[test]
fn kept_cards_survive_other_peeks_but_cannot_act_after_runtime_replacement() {
    let (mut app, graph, source, member) = fixture();
    let kept = app.focus_card_target(graph, member, Origin::Node).unwrap();
    app.update(Action::FocusCard(Command::Peek(kept.clone())));
    app.update(Action::FocusCard(Command::Keep(kept.clone())));
    let peek = app.focus_card_target(graph, source, Origin::Node).unwrap();
    app.update(Action::FocusCard(Command::Peek(peek.clone())));
    app.update(Action::FocusCard(Command::Close(peek)));
    assert_eq!(app.focus_card_views().len(), 1);
    let clone = app.graph_runtimes.canvas(graph).unwrap().graph().clone();
    app.graph_runtimes.set_graph(clone);
    assert!(!app.valid_focus_card(&kept));
    assert!(app.focus_card_views().is_empty());
    let before = app.scope_place();
    app.update(Action::FocusCard(Command::Read(kept)));
    assert_eq!(app.scope_place(), before);
}
