// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn arrange_buttons_follow_the_unique_torn_out_graph_without_using_the_active_cursor() {
    use crate::arrange_pane::ArrangePane;
    let (mut app, arrange, graph_pane, owner) = arrange_after_ordinary_graph_tear_out();
    let unrelated = GraphId::from_uuid(uuid::Uuid::new_v4());
    let mut unrelated_canvas = Canvas::new();
    unrelated_canvas.visit("mere://unrelated-compatibility-cursor");
    app.graph_runtimes
        .activate_or_insert(unrelated, None, unrelated_canvas);
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
        let (x, y) = renderer.inline_selector(&selector).unwrap().unwrap();
        let intents = renderer.click(x, y, 600, 800);
        assert_eq!(intents.len(), 1);
        for intent in intents {
            app.update(intent.into_action().unwrap());
        }
        assert_eq!(
            app.graph_runtimes.canvas(owner).unwrap().has_saved_seed(),
            saved
        );
        assert!(
            !app.graph_runtimes
                .canvas(unrelated)
                .unwrap()
                .has_saved_seed()
        );
    }
    app.update(Action::SetPhysicsLaw("orbit.gravity"));
    assert_eq!(
        mere::canvas::PhysicsChoice::live(app.graph_runtimes.canvas(owner).unwrap())
            .law
            .id(),
        "orbit.gravity"
    );
    assert_eq!(
        mere::canvas::PhysicsChoice::live(app.graph_runtimes.canvas(unrelated).unwrap()).law,
        mere::canvas::PhysicsLaw::Springs
    );
    assert_eq!(app.graph_runtimes.active_graph(), unrelated);
}

#[test]
fn arrange_refuses_ambiguous_foreign_graph_panes_instead_of_guessing_their_selection() {
    let (mut app, _, _, owner) = arrange_after_ordinary_graph_tear_out();
    app.lenses.push(Some(FrisketLayout {
        root: PaneNode::Leaf {
            pane_id: PaneId(9000),
            content: PaneContent::Orrery,
            graph_id: owner,
        },
        ..Default::default()
    }));
    app.index_pane_spaces();
    let unrelated = GraphId::from_uuid(uuid::Uuid::new_v4());
    app.graph_runtimes
        .activate_or_insert(unrelated, None, Canvas::new());
    assert_eq!(app.scene_command_pane(), None);
    assert!(app.update(Action::SetSeed).is_empty());
    assert!(
        app.update(Action::SetPhysicsLaw("orbit.gravity"))
            .is_empty()
    );
    for graph in [owner, unrelated] {
        let canvas = app.graph_runtimes.canvas(graph).unwrap();
        assert!(!canvas.has_saved_seed());
        assert_eq!(
            mere::canvas::PhysicsChoice::live(canvas).law,
            mere::canvas::PhysicsLaw::Springs
        );
    }
}

#[test]
fn role_and_composition_choices_reopen_from_the_saved_scene_into_the_form() {
    use mere::canvas::physics_controls::{StageDraft, TwoLawGrouping, TwoLawMix, TwoStageSchedule};
    use mere::canvas::physics_form::{FieldId, PhysicsForm, Readback, Slot};
    use mere::canvas::{PhysicsKindSource, PhysicsLaw, PhysicsOverlay, Role, StageStop};

    let mix = TwoLawMix {
        first: PhysicsLaw::Charge,
        second: PhysicsLaw::Springs,
        first_share: 0.3,
        second_share: 2.0,
    };
    let grouping = TwoLawGrouping {
        partition: PhysicsKindSource::Component,
        outer: PhysicsLaw::Charge,
        inner: PhysicsLaw::Springs,
        outer_share: 4.5,
    };
    let schedule = TwoStageSchedule {
        first: StageDraft {
            law: PhysicsLaw::Springs,
            stop: StageStop::Frames(12000),
            capture: None,
        },
        second: StageDraft {
            law: PhysicsLaw::Charge,
            stop: StageStop::LawDone,
            capture: Some(Role::Pinned),
        },
        overlays: vec![PhysicsOverlay::GravityLocus],
    };
    for composition in [
        Action::ApplyPhysicsMix(mix),
        Action::ApplyPhysicsGrouping(grouping),
        Action::ApplyPhysicsSchedule(schedule.clone()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (mut source, pane, _) = fixture();
        source.data_root = dir.path().to_path_buf();
        let id = source.session_id;
        let graph = source.graph_for_pane(pane).unwrap();
        source
            .sessions
            .insert(pandect::GraphSessionManifest::new(id, graph));
        source.update(Action::SetPhysicsRole(Role::Pinned.id()));
        source.update(Action::SetPhysicsRoleGroups(
            PhysicsKindSource::Component.id(),
        ));
        source.update(composition);
        let expected = source
            .with_graph_pane(pane, |canvas| Readback::of(canvas))
            .unwrap();
        assert!(
            source.graph_runtimes.get(graph).unwrap().scene_save_pending,
            "a composing edit marks the scene for saving"
        );
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
        reopened.graph_pane_frame(reopened_pane, 960, 720).unwrap();
        let actual = reopened
            .with_graph_pane(reopened_pane, |canvas| Readback::of(canvas))
            .unwrap();
        // The speed is the canvas's pace, not part of the scene.
        assert_eq!(
            Readback {
                speed: expected.speed,
                ..actual.clone()
            },
            expected
        );
        assert_eq!(actual.role, Role::Pinned);
        assert_eq!(actual.role_groups, PhysicsKindSource::Component);
        assert!(
            matches!(actual.mix, Ok(Some(_)))
                || matches!(actual.grouping, Ok(Some(_)))
                || matches!(actual.schedule, Ok(Some(_)))
        );

        // The form the pane draws opens on what was saved.
        let mut form = PhysicsForm::default();
        form.follow_roles(&actual);
        form.follow_composition(&actual);
        assert_eq!(form.value(FieldId::Role), "pinned");
        if let Ok(Some(saved)) = &actual.schedule {
            assert_eq!(form.schedule().as_ref(), Ok(saved));
            assert_eq!(form.value(FieldId::StageFrames(Slot::First)), "12000");
        }
        if let Ok(Some(saved)) = &actual.mix {
            assert_eq!(form.mix().as_ref(), Ok(saved));
        }
        if let Ok(Some(saved)) = &actual.grouping {
            assert_eq!(form.grouping().as_ref(), Ok(saved));
        }
    }
}
