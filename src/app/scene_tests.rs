// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::canvas::Canvas;
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::{EdgeAssertion, ProvenanceSubKind};
use scenograph::attention::Attention;

const URLS: [&str; 4] = [
    "https://woodshed.example/progression",
    "https://woodshed.example/take",
    "https://knot.example/lyrics",
    "https://sources.example/chords",
];

fn fixture() -> (App, PaneId, Vec<uuid::Uuid>) {
    let mut app = App::test_stub();
    let pane = app.default_graph_pane();
    let members = app
        .with_graph_pane(pane, |canvas| {
            for url in URLS {
                canvas.visit(url);
            }
            let members: Vec<_> = URLS
                .iter()
                .map(|url| canvas.graph().get_node_by_url(url).unwrap().1.id)
                .collect();
            assert!(canvas.tag_node(members[0], "song"));
            assert!(canvas.tag_node(members[0], "practice"));
            assert!(canvas.tag_node(members[2], "song"));
            assert!(canvas.tag_node(members[2], "publishing"));
            canvas.ingest_graph(|graph| {
                let keys: Vec<_> = members
                    .iter()
                    .map(|id| graph.get_node_key_by_id(*id).unwrap())
                    .collect();
                mere::kernel::graph::apply::assert_relation(
                    graph,
                    keys[0],
                    keys[3],
                    EdgeAssertion::Provenance {
                        sub_kind: ProvenanceSubKind::ExtractedFrom,
                    },
                )
                .unwrap();
                mere::kernel::graph::apply::assert_relation(
                    graph,
                    keys[2],
                    keys[0],
                    EdgeAssertion::Provenance {
                        sub_kind: ProvenanceSubKind::RewrittenFrom,
                    },
                )
                .unwrap();
                true
            });
            let keys: Vec<_> = members.iter().map(|id| canvas.graph().get_node_key_by_id(*id).unwrap()).collect();
            assert!(canvas.graph().node_content_tags(keys[0]).unwrap().contains("song"), "public tag operation must retain canonical source data");
            assert!(canvas.graph().node_content_tags(keys[2]).unwrap().contains("publishing"));
            let relations: Vec<_> = canvas.graph().projected_relations().map(|(_, row)| row).collect();
            for (from, to) in [(keys[0], keys[3]), (keys[2], keys[0])] {
                assert!(relations.iter().any(|r| r.from == from && r.to == to && matches!(r.kind, mere::kernel::graph::RelationKind::Provenance(_))), "fixture source provenance must survive public writes and reconciliation: {relations:?}");
            }
            canvas.resize(800, 600);
            canvas.set_physics_paused(true);
            canvas.clear_selection();
            members
        })
        .unwrap();
    app.focus = FocusTarget::Graph(pane);
    (app, pane, members)
}

fn place(app: &mut App, pane: PaneId, id: &'static str) {
    app.update(Action::SetLayoutStrategy(Some(id)));
    app.with_graph_pane(pane, |canvas| {
        App::drive_layout_strategy_for(canvas, 800, 600);
        canvas.frame(800, 600);
    })
    .unwrap();
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

#[test]
fn app_adoption_reopens_saved_mosaic_seed_regions_and_pause_before_the_next_layout_pass() {
    use mere::canvas::mosaic::MosaicTarget;
    use std::collections::BTreeMap;

    let dir = tempfile::tempdir().unwrap();
    let (mut source, pane, members) = fixture();
    source.data_root = dir.path().to_path_buf();
    let id = source.session_id;
    let graph = source.graph_for_pane(pane).unwrap();
    source
        .sessions
        .insert(pandect::GraphSessionManifest::new(id, graph));
    place(&mut source, pane, "mosaic.default");
    source.with_graph_pane(pane, |canvas| {
        assert!(
            canvas.mosaic_move_member(members[2], MosaicTarget::Group("woodshed.example".into()))
        );
        canvas.select_member(members[2]);
    });
    source.update(Action::GrowTiles);
    source.update(Action::SetSeed);
    let expected = source
        .with_graph_pane(pane, |c| c.cartography_geometry())
        .unwrap();
    assert!(!expected.regions().is_empty());
    assert!(expected.mosaic().is_some());
    source.refresh_facets();
    let session_dir = source.session_dir();
    std::fs::create_dir_all(&session_dir).unwrap();
    pandect::session_graph_store::save_profiled(
        &session_dir.join(pandect::session_graph_store::GRAPH_FILE),
        source.graph_runtimes.graph(),
        Some(pandect::graph_placement::PlacementProfile::RecordedStrataV1),
    )
    .unwrap();
    crate::session::save_node_facets(&session_dir, source.graph_runtimes.facets());
    crate::session::save_view_intent(
        &session_dir,
        &crate::session::ViewIntentV1 {
            layout_strategy: Some("mosaic.default".into()),
            physics_paused: Some(true),
            ..Default::default()
        },
    );
    let mut reopened = App::test_stub_at(dir.path().to_path_buf());
    reopened
        .sessions
        .insert(pandect::GraphSessionManifest::new(id, graph));
    reopened.adopt_session(id);
    assert!(!reopened.session_load_refused());
    let reopened_pane = reopened.default_graph_pane();
    reopened
        .with_graph_pane(reopened_pane, |canvas| {
            assert!(canvas.graph_grammar());
            assert!(canvas.physics_paused());
            assert_eq!(canvas.layout_strategy(), Some("mosaic.default"));
            // A normal first recompute must respect the restored Mosaic state.
            App::drive_layout_strategy_for(canvas, 800, 600);
            let actual = canvas.cartography_geometry();
            assert_eq!(
                actual
                    .seed()
                    .unwrap()
                    .iter()
                    .copied()
                    .collect::<BTreeMap<_, _>>(),
                expected
                    .seed()
                    .unwrap()
                    .iter()
                    .copied()
                    .collect::<BTreeMap<_, _>>()
            );
            assert_eq!(actual.regions(), expected.regions());
            let a = actual.mosaic().unwrap();
            let e = expected.mosaic().unwrap();
            assert_eq!(
                a.group_of.iter().cloned().collect::<BTreeMap<_, _>>(),
                e.group_of.iter().cloned().collect::<BTreeMap<_, _>>()
            );
            assert_eq!(
                a.scale.iter().copied().collect::<BTreeMap<_, _>>(),
                e.scale.iter().copied().collect::<BTreeMap<_, _>>()
            );
            assert_eq!(a.member_order, e.member_order);
            assert_eq!(a.group_order, e.group_order);
            assert!(canvas.physics_paused());
        })
        .unwrap();
}

#[test]
fn graph_scene_persistence_writes_the_explicit_owner_and_preserves_the_active_session() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let active = app.graph_runtimes.active_graph();
    let active_dir = app.session_dir();
    std::fs::create_dir_all(&active_dir).unwrap();
    std::fs::write(
        active_dir.join("facets.json"),
        b"active facets remain byte identical",
    )
    .unwrap();
    std::fs::write(
        active_dir.join("view-intent.json"),
        b"active intent remains byte identical",
    )
    .unwrap();
    let owner_session = crate::panes::SessionId::new();
    let owner = GraphId::from_uuid(*owner_session.as_uuid());
    let owner_dir = crate::session::session_dir(root.path(), owner_session);
    std::fs::create_dir_all(&owner_dir).unwrap();
    std::fs::write(
        owner_dir.join("graph.json"),
        b"graph truth remains byte identical",
    )
    .unwrap();
    let mut canvas = Canvas::new();
    canvas.visit("mere://owned-scene/a");
    canvas.visit("mere://other-scene/b");
    canvas.resize(800, 600);
    canvas.set_layout_strategy(Some("mosaic.default".into()));
    App::drive_layout_strategy_for(&mut canvas, 800, 600);
    canvas.set_seed();
    let expected_seed = canvas.cartography_geometry().seed().unwrap().to_vec();
    app.graph_runtimes
        .activate_or_insert(owner, Some(owner_session), canvas);
    app.graph_runtimes.get_mut(owner).unwrap().persistence =
        crate::session_persistence::SessionPersistence::writable(owner_dir.clone(), None);
    app.graph_runtimes.activate(active);
    app.persist_graph_scene(owner).unwrap();
    assert_eq!(app.graph_runtimes.active_graph(), active);
    assert_eq!(
        std::fs::read(active_dir.join("facets.json")).unwrap(),
        b"active facets remain byte identical"
    );
    assert_eq!(
        std::fs::read(active_dir.join("view-intent.json")).unwrap(),
        b"active intent remains byte identical"
    );
    assert_eq!(
        std::fs::read(owner_dir.join("graph.json")).unwrap(),
        b"graph truth remains byte identical"
    );
    let facets = pandect::load_node_facets(&owner_dir).unwrap().unwrap();
    let seed = pandect::read_arrangement_seed(&facets)
        .unwrap()
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(seed, expected_seed.into_iter().collect());
    assert!(!pandect::read_arrangement_regions(&facets, *owner.as_uuid()).is_empty());
    let intent = crate::session::load_view_intent(&owner_dir).unwrap();
    assert_eq!(intent.layout_strategy.as_deref(), Some("mosaic.default"));
    assert_eq!(intent.physics_paused, Some(true));
}

#[test]
fn refused_graph_scene_persistence_preserves_both_disk_and_live_facets() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let graph = app.graph_runtimes.active_graph();
    let directory = app.session_dir();
    std::fs::create_dir_all(&directory).unwrap();
    for file in ["facets.json", "view-intent.json", "graph.json"] {
        std::fs::write(directory.join(file), format!("preserve {file}")).unwrap();
    }
    let runtime = app.graph_runtimes.get_mut(graph).unwrap();
    runtime.canvas.visit("mere://refused-scene");
    runtime.canvas.set_seed();
    let before = serde_json::to_value(runtime.canvas.facets()).unwrap();
    runtime.persistence = crate::session_persistence::SessionPersistence::refused(
        directory.clone(),
        "fixture refusal".into(),
    );
    assert!(app.persist_graph_scene(graph).is_err());
    for file in ["facets.json", "view-intent.json", "graph.json"] {
        assert_eq!(
            std::fs::read(directory.join(file)).unwrap(),
            format!("preserve {file}").as_bytes()
        );
    }
    assert_eq!(
        before,
        serde_json::to_value(app.graph_runtimes.canvas(graph).unwrap().facets()).unwrap()
    );
}

#[test]
fn resizing_mosaic_tiles_cancels_both_pending_and_running_reset_returns() {
    for running in [false, true] {
        let (mut app, pane, members) = fixture();
        place(&mut app, pane, "mosaic.default");
        app.update(Action::SetSeed);
        app.with_graph_pane(pane, |canvas| {
            canvas.seed_cartography(members.iter().map(|id| (*id, (1200.0, 800.0))));
        });
        app.update(Action::ResetToSeed);
        if running {
            let graph = app.graph_for_pane(pane).unwrap();
            let mut transition = None;
            app.with_graph_pane(pane, |canvas| {
                mere::canvas::physics_controls::advance_arrangement(
                    canvas,
                    &mut transition,
                    "mosaic.default",
                    0.0,
                );
            });
            assert!(transition.is_some());
            app.graph_runtimes.get_mut(graph).unwrap().scene_transition = transition;
        } else {
            app.with_graph_pane(pane, |canvas| assert!(canvas.has_stop_return()));
        }
        app.with_graph_pane(pane, |canvas| {
            canvas.select_member(members[0]);
        });
        app.update(Action::GrowTiles);
        let graph = app.graph_for_pane(pane).unwrap();
        let runtime = app.graph_runtimes.get(graph).unwrap();
        assert!(runtime.scene_transition.is_none());
        assert!(!runtime.canvas.has_stop_return());
        let geometry = runtime.canvas.cartography_geometry();
        assert_eq!(
            geometry
                .mosaic()
                .unwrap()
                .scale
                .iter()
                .find(|(id, _)| *id == members[0])
                .unwrap()
                .1,
            1.25
        );
        assert!(runtime.scene_save_pending);
    }
}
