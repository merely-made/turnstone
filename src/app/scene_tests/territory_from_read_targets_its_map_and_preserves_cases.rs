// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn territory_from_read_targets_its_map_and_preserves_source_and_positions() {
    let (mut app, pane, members) = fixture();
    let owner = app.graph_for_pane(pane).unwrap();
    app.update(Action::ReadGraphMember(members[0]));
    let other = GraphId::from_uuid(uuid::Uuid::new_v4());
    app.graph_runtimes
        .activate_or_insert(other, None, Canvas::new());
    let before = serde_json::to_value(
        app.graph_runtimes
            .canvas(owner)
            .unwrap()
            .graph()
            .to_snapshot_at(0),
    )
    .unwrap();
    let positions = app
        .graph_runtimes
        .canvas(owner)
        .unwrap()
        .cartography_geometry();
    let effects = app.update(Action::ToggleTerritory);
    assert!(app.graph_runtimes.canvas(owner).unwrap().territory_field());
    assert!(!app.graph_runtimes.canvas(other).unwrap().territory_field());
    assert!(effects.iter().all(|e| matches!(e, Effect::Redraw)));
    assert_eq!(
        before,
        serde_json::to_value(
            app.graph_runtimes
                .canvas(owner)
                .unwrap()
                .graph()
                .to_snapshot_at(0)
        )
        .unwrap()
    );
    assert_eq!(
        positions,
        app.graph_runtimes
            .canvas(owner)
            .unwrap()
            .cartography_geometry()
    );
    app.update(Action::ToggleTerritory);
    assert!(!app.graph_runtimes.canvas(owner).unwrap().territory_field());
}

#[test]
fn scene_catalog_layouts_place_finite_members_and_their_bounded_regions() {
    let (mut app, pane, members) = fixture();
    for (id, bounded) in [
        ("partition.default", true),
        ("sets.default", true),
        ("provenance.default", false),
        ("mosaic.default", true),
    ] {
        place(&mut app, pane, id);
        app.with_graph_pane(pane, |canvas| {
            assert_eq!(canvas.layout_strategy(), Some(id));
            assert_eq!(!canvas.strategy_regions().is_empty(), bounded, "{id}");
            for member in &members {
                let key = canvas.graph().get_node_key_by_id(*member).unwrap();
                let p = canvas.world_position_of(key).expect("placed source member");
                assert!(p.x.is_finite() && p.y.is_finite(), "{id}: {p:?}");
            }
            assert!(
                canvas
                    .strategy_regions()
                    .iter()
                    .all(|r| r.contour.iter().all(|p| p.x.is_finite() && p.y.is_finite()))
            );
        })
        .unwrap();
    }
}

#[test]
fn leaving_bounded_scenes_clears_regions_including_the_free_path() {
    let (mut app, pane, _) = fixture();
    for target in [Some("grid.default"), None] {
        place(&mut app, pane, "partition.default");
        app.with_graph_pane(pane, |c| assert!(!c.strategy_regions().is_empty()));
        app.update(Action::SetLayoutStrategy(target));
        app.with_graph_pane(pane, |canvas| {
            App::drive_layout_strategy_for(canvas, 800, 600);
            assert!(canvas.strategy_regions().is_empty());
        });
    }
}

#[test]
fn provenance_scene_places_real_derivation_steps_after_their_sources() {
    let (mut app, pane, members) = fixture();
    place(&mut app, pane, "provenance.default");
    app.with_graph_pane(pane, |canvas| {
        let x = |id| {
            canvas
                .world_position_of(canvas.graph().get_node_key_by_id(id).unwrap())
                .unwrap()
                .x
        };
        assert!(
            x(members[3]) < x(members[0]),
            "source precedes extraction: source={}, extraction={}, rewrite={}, axis={:?}",
            x(members[3]),
            x(members[0]),
            x(members[2]),
            mere::canvas::scene_axes::derivation_columns(canvas.graph())
        );
        assert!(x(members[0]) < x(members[2]), "extraction precedes rewrite");
    });
}

#[test]
fn a_scene_pick_preserves_the_persons_playing_or_paused_choice() {
    for paused in [true, false] {
        let (mut app, pane, _) = fixture();
        app.with_graph_pane(pane, |c| c.set_physics_paused(paused));
        place(&mut app, pane, "partition.default");
        app.with_graph_pane(pane, |c| assert_eq!(c.physics_paused(), paused));
    }
}

#[test]
fn scene_and_attention_commands_resolve_the_owner_instead_of_the_compatibility_cursor() {
    let (mut app, pane, members) = fixture();
    let owner = app.graph_for_pane(pane).unwrap();
    app.with_graph_pane(pane, |c| c.select_member(members[0]));
    let other = GraphId::from_uuid(uuid::Uuid::new_v4());
    let mut other_canvas = Canvas::new();
    other_canvas.visit("mere://unrelated-scene");
    other_canvas.set_physics_paused(true);
    app.graph_runtimes
        .activate_or_insert(other, None, other_canvas);
    let other_truth = serde_json::to_value(
        app.graph_runtimes
            .canvas(other)
            .unwrap()
            .graph()
            .to_snapshot_at(0),
    )
    .unwrap();
    place(&mut app, pane, "mosaic.default");
    app.update(Action::SetSeed);
    app.update(Action::ToggleForegroundPin);
    let owned = app.graph_runtimes.canvas(owner).unwrap();
    assert_eq!(owned.layout_strategy(), Some("mosaic.default"));
    assert!(owned.has_saved_seed());
    assert_eq!(owned.member_foreground_pinned(members[0]), Some(true));
    let untouched = app.graph_runtimes.canvas(other).unwrap();
    assert_eq!(untouched.layout_strategy(), None);
    assert!(!untouched.has_saved_seed());
    assert!(
        untouched
            .members_with_attention(Attention::Retained)
            .is_empty()
    );
    assert_eq!(
        other_truth,
        serde_json::to_value(untouched.graph().to_snapshot_at(0)).unwrap()
    );
}

#[test]
fn foreground_attention_survives_selection_without_becoming_a_position_pin_or_truth() {
    let (mut app, pane, members) = fixture();
    let truth = app
        .with_graph_pane(pane, |canvas| {
            canvas.seed_cartography(
                members
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (*id, (i as f32 * 100.0, 40.0))),
            );
            canvas.set_seed();
            canvas.select_member(members[0]);
            serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap()
        })
        .unwrap();
    app.update(Action::ToggleForegroundPin);
    app.with_graph_pane(pane, |canvas| {
        canvas.select_member(members[1]);
        assert_eq!(
            canvas.member_attention(members[0]),
            Some(Attention::Forward)
        );
        // Move all unheld positions through the public restore seam. Reset must
        // still move the foreground member: attention did not position-pin it.
        canvas.seed_cartography(members.iter().map(|id| (*id, (900.0, 700.0))));
        let moves = canvas.reset_to_seed();
        assert!(moves.iter().any(|m| m.key == members[0]));
        let key = canvas.graph().get_node_key_by_id(members[0]).unwrap();
        assert_eq!(
            canvas.world_position_of(key),
            Some(PortablePoint::new(0.0, 40.0))
        );
        canvas.select_member(members[0]);
    });
    app.update(Action::ToggleForegroundPin);
    app.with_graph_pane(pane, |canvas| {
        canvas.select_member(members[1]);
        assert_eq!(
            canvas.member_attention(members[0]),
            Some(Attention::Retained)
        );
    });
    app.update(Action::DismissRetained);
    app.with_graph_pane(pane, |canvas| {
        assert_eq!(
            canvas.member_attention(members[0]),
            Some(Attention::Ambient)
        );
        assert_eq!(
            canvas.member_attention(members[1]),
            Some(Attention::Forward)
        );
        assert_eq!(
            truth,
            serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap()
        );
    });
}

#[test]
fn arrange_controls_and_physics_resolve_their_window_graph_with_a_different_active_cursor() {
    let (mut app, primary, _) = fixture();
    let graph_a = app.graph_for_pane(primary).unwrap();
    let graph_b = GraphId::from_uuid(uuid::Uuid::new_v4());
    let mut canvas = Canvas::new();
    canvas.visit("mere://lens-owned-scene");
    canvas.resize(640, 480);
    canvas.set_physics_paused(true);
    app.graph_runtimes.activate_or_insert(graph_b, None, canvas);
    assert!(
        app.graph_runtimes.canvas(graph_b).unwrap().graph_grammar(),
        "new graph runtimes adopt native grammar"
    );
    assert!(app.graph_runtimes.activate(graph_a));
    let lens_graph = PaneId(4000);
    let arrange = PaneId(4001);
    app.lenses.push(Some(FrisketLayout {
        root: PaneNode::Split {
            axis: crate::panes::SplitAxis::Horizontal,
            ratio: 0.6,
            first: Box::new(PaneNode::Leaf {
                pane_id: lens_graph,
                content: PaneContent::Orrery,
                graph_id: graph_b,
            }),
            second: Box::new(PaneNode::Leaf {
                pane_id: arrange,
                content: PaneContent::Registered(crate::panes::PaneKindId::new(
                    crate::panes::kind::ARRANGE,
                )),
                graph_id: graph_b,
            }),
        },
        ..Default::default()
    }));
    app.index_pane_spaces();
    app.active_pane = Some(arrange);
    app.focus = FocusTarget::Pane(arrange);
    assert_eq!(app.scene_command_pane(), Some(lens_graph));
    app.update(Action::SetPhysicsLaw("orbit.gravity"));
    app.update(Action::SetLayoutStrategy(Some("mosaic.default")));
    app.with_graph_pane(lens_graph, |c| App::drive_layout_strategy_for(c, 640, 480));
    app.update(Action::SetSeed);
    let paused = app.graph_runtimes.canvas(graph_b).unwrap().physics_paused();
    app.update(Action::TogglePhysics);
    let b = app.graph_runtimes.canvas(graph_b).unwrap();
    assert_eq!(
        mere::canvas::PhysicsChoice::live(b).law.id(),
        "orbit.gravity"
    );
    assert_eq!(b.layout_strategy(), Some("mosaic.default"));
    assert!(b.has_saved_seed());
    assert_eq!(b.physics_paused(), !paused);
    let a = app.graph_runtimes.canvas(graph_a).unwrap();
    assert_eq!(
        mere::canvas::PhysicsChoice::live(a).law,
        mere::canvas::PhysicsLaw::Springs
    );
    assert_eq!(a.layout_strategy(), None);
    assert!(!a.has_saved_seed());
    assert!(a.physics_paused());
}

#[test]
fn a_new_arrangement_discards_the_reset_return_that_has_not_reached_a_frame() {
    let (mut app, pane, members) = fixture();
    app.update(Action::SetSeed);
    app.with_graph_pane(pane, |canvas| {
        canvas.seed_cartography(members.iter().map(|id| (*id, (1100.0, 900.0))));
    });
    app.update(Action::ResetToSeed);
    app.with_graph_pane(pane, |canvas| assert!(canvas.has_stop_return()));
    // Deliberately pick before rendering: the old return must not overwrite
    // the new arrangement when the host next advances its transition clock.
    place(&mut app, pane, "partition.default");
    app.with_graph_pane(pane, |canvas| {
        assert!(!canvas.has_stop_return());
        assert_eq!(canvas.layout_strategy(), Some("partition.default"));
        assert!(!canvas.strategy_regions().is_empty());
    });
    let graph = app.graph_for_pane(pane).unwrap();
    assert!(
        app.graph_runtimes
            .get(graph)
            .unwrap()
            .scene_transition
            .is_none()
    );
}
