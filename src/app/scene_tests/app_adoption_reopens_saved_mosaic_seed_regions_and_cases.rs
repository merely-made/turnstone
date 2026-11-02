// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    source.with_graph_pane(pane, |canvas| canvas.set_physics_paused(true));
    source.refresh_facets();
    let session_dir = source.session_dir();
    std::fs::create_dir_all(&session_dir).unwrap();
    crate::session_graph_file::save_profiled(
        &session_dir.join(pandect::session_graph_store::GRAPH_FILE),
        source.graph_runtimes.graph(),
        Some(pandect::graph_placement::PlacementProfile::RecordedStrataV1),
    )
    .unwrap();
    crate::session::save_node_facets(&session_dir, source.graph_runtimes.facets());
    let mut reopened = App::test_stub_at(dir.path().to_path_buf());
    reopened
        .sessions
        .insert(pandect::GraphSessionManifest::new(id, graph));
    reopened.adopt_session(id);
    assert!(!reopened.session_load_refused());
    let reopened_pane = reopened.default_graph_pane();
    assert!(
        reopened
            .graph_runtimes
            .get(graph)
            .unwrap()
            .scene_restore_pending
    );
    for _ in 0..3 {
        // The actual host loop receives a new viewport on reopen. It must
        // prime its recompute gate rather than replace saved paused geometry.
        reopened.graph_pane_frame(reopened_pane, 960, 720).unwrap();
        reopened
            .with_graph_pane(reopened_pane, |canvas| {
                assert!(canvas.graph_grammar());
                assert!(canvas.physics_paused());
                assert_eq!(canvas.layout_strategy(), Some("mosaic.default"));
                let actual = canvas.cartography_geometry();
                assert_eq!(
                    actual.iter().collect::<BTreeMap<_, _>>(),
                    expected.iter().collect::<BTreeMap<_, _>>()
                );
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
    assert!(
        !reopened
            .graph_runtimes
            .get(graph)
            .unwrap()
            .scene_restore_pending
    );
    // A subsequent resize resumes the ordinary projection contract. Mosaic
    // is an infinite workspace: its configured aspect and world-area budget
    // can legitimately produce the same geometry at a different viewport.
    let (projected_positions, projected_regions) = reopened
        .with_graph_pane(reopened_pane, |canvas| {
            let focus = canvas.focused_key();
            assert!(!canvas.needs_strategy_recompute("mosaic.default", 960, 720, focus));
            assert!(canvas.needs_strategy_recompute("mosaic.default", 1200, 900, focus));
            let extents = canvas.strategy_extents();
            let projection = canvas.project_arrangement_for_view(
                "mosaic.default",
                1200,
                900,
                Some(&extents),
                canvas.size_by_recency(),
                canvas.camera().zoom,
                None,
            );
            let positions = projection
                .positions
                .iter()
                .map(|(key, p)| (canvas.graph().get_node(*key).unwrap().id, (p.x, p.y)))
                .collect::<BTreeMap<_, _>>();
            (positions, projection.regions)
        })
        .unwrap();
    reopened.graph_pane_frame(reopened_pane, 1200, 900).unwrap();
    reopened
        .with_graph_pane(reopened_pane, |canvas| {
            assert!(!canvas.needs_strategy_recompute(
                "mosaic.default",
                1200,
                900,
                canvas.focused_key()
            ));
            assert_eq!(
                canvas
                    .cartography_geometry()
                    .iter()
                    .collect::<BTreeMap<_, _>>(),
                projected_positions
            );
            assert_eq!(canvas.strategy_regions(), projected_regions);
        })
        .unwrap();

    // A person's new arrangement command before the first host frame must
    // win over the saved Mosaic placement, while keeping their saved seed.
    // A restart releases the previous writer before adopting its session.
    drop(reopened);
    let mut changed = App::test_stub_at(dir.path().to_path_buf());
    changed
        .sessions
        .insert(pandect::GraphSessionManifest::new(id, graph));
    changed.adopt_session(id);
    assert!(!changed.session_load_refused());
    let changed_pane = changed.default_graph_pane();
    changed.active_pane = Some(changed_pane);
    changed.focus = FocusTarget::Graph(changed_pane);
    changed.update(Action::SetLayoutStrategy(Some("grid.default")));
    changed.graph_pane_frame(changed_pane, 960, 720).unwrap();
    changed
        .with_graph_pane(changed_pane, |canvas| {
            assert_eq!(canvas.layout_strategy(), Some("grid.default"));
            assert!(canvas.physics_paused());
            let actual = canvas.cartography_geometry();
            assert!(actual.regions().is_empty());
            assert_ne!(
                actual.iter().collect::<BTreeMap<_, _>>(),
                expected.iter().collect::<BTreeMap<_, _>>()
            );
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
        crate::session_persistence::SessionPersistence::writable(owner_dir.clone(), None).unwrap();
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
    let scene = crate::scene_state::read_scene(&facets, *owner.as_uuid())
        .expect("the scene is saved on the container")
        .expect("and reads");
    let seed = scene
        .cartography
        .seed()
        .unwrap()
        .iter()
        .copied()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(seed, expected_seed.into_iter().collect());
    assert!(!scene.cartography.regions().is_empty());
    assert_eq!(
        mere::canvas::saved_scene::scene_arrangement(&scene).unwrap(),
        "mosaic.default"
    );
    assert!(scene.physics_paused);
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

#[test]
fn summoned_arrange_pane_binds_its_graph_and_routes_actual_seed_buttons_and_physics() {
    use crate::arrange_pane::{ArrangeIntent, ArrangePane};
    let (mut app, graph_pane, _) = fixture();
    let graph = app.graph_for_pane(graph_pane).unwrap();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ARRANGE,
    )));
    let arrange = app.active_pane.expect("ordinary summon activates Arrange");
    assert_eq!(
        app.graph_for_pane(arrange),
        Some(graph),
        "a registry-declared graph source must be bound on summon"
    );
    // The physical native press focuses the clicked pane before invoking
    // its retained handler. Reproduce that real focus transition here.
    app.focus = FocusTarget::Pane(arrange);
    assert_eq!(
        app.scene_command_pane(),
        Some(graph_pane),
        "active={:?}, focus={:?}, primary={:?}, lenses={:?}, arrange={arrange:?}",
        app.active_pane,
        app.focus,
        app.frisket.iter_leaves().collect::<Vec<_>>(),
        app.lenses
            .iter()
            .flatten()
            .map(|s| s.iter_leaves().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    let mut renderer = ArrangePane::new();
    for (label, saved) in [("Set seed", true), ("Restore recipe seed", false)] {
        renderer.sync_for_pane(&app, arrange, 600.0, 800.0);
        renderer.scene(600, 800, 1.0);
        let selector = taproot::Selector::role("button").containing(label);
        let (x, y) = renderer
            .inline_selector(&selector)
            .unwrap()
            .expect("visible seed button");
        let intents = renderer.click(x, y, 600, 800);
        assert_eq!(intents.len(), 1, "physical button produces one intent");
        for intent in intents {
            app.update(intent.into_action().unwrap());
        }
        assert_eq!(
            app.graph_runtimes.canvas(graph).unwrap().has_saved_seed(),
            saved
        );
    }
    let physics = ArrangeIntent::SetLaw("orbit.gravity".into())
        .into_action()
        .unwrap();
    app.update(physics);
    assert_eq!(
        mere::canvas::PhysicsChoice::live(app.graph_runtimes.canvas(graph).unwrap())
            .law
            .id(),
        "orbit.gravity"
    );
}

#[test]
fn registered_graph_sources_retag_with_the_session_while_settings_stays_independent() {
    let (mut app, graph_pane, _) = fixture();
    let outgoing = app.graph_for_pane(graph_pane).unwrap();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ARRANGE,
    )));
    let arrange = app.active_pane.unwrap();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::SETTINGS,
    )));
    let settings = app.active_pane.unwrap();
    assert_eq!(app.graph_for_pane(arrange), Some(outgoing));
    assert_eq!(app.graph_for_pane(settings), None);
    let incoming = GraphId::from_uuid(uuid::Uuid::new_v4());
    app.frisket.retag_graph_bound_from(outgoing, incoming);
    assert_eq!(app.graph_for_pane(graph_pane), Some(incoming));
    assert_eq!(app.graph_for_pane(arrange), Some(incoming));
    assert_eq!(app.graph_for_pane(settings), None);
    for kind in [
        crate::panes::kind::SETTINGS,
        crate::panes::kind::PUBLISHING,
        crate::panes::kind::SHARED_KNOT,
    ] {
        assert!(
            !PaneContent::Registered(crate::panes::PaneKindId::new(kind)).follows_active_graph(),
            "non-graph provider {kind} must stay independent"
        );
    }
}
