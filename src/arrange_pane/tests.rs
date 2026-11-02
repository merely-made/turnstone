// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::canvas::physics_controls::{read_two_law_mix, speed_preset};
use mere::canvas::{PhysicsLaw, Role};

#[test]
fn refused_overlays_are_visible_and_keep_the_applied_law() {
    let mut app = App::test_stub();
    app.update(Action::SetPhysicsOverlay(
        PhysicsOverlay::GridSnap.id(),
        true,
    ));
    app.update(Action::SetPhysicsLaw(PhysicsLaw::Density.id()));
    let refusal = app
        .physics_refusal()
        .expect("Density refused the existing overlay");
    assert_eq!(refusal.law, PhysicsLaw::Density);
    assert_eq!(refusal.refused, vec![PhysicsOverlay::GridSnap]);
    assert_eq!(app.graph_runtimes.physics_law(), PhysicsLaw::Density);
    assert!(app.graph_runtimes.physics_overlays().is_empty());
    assert!(
        arrange_rows(&app)
            .iter()
            .any(|row| row.starts_with("Physics refused:"))
    );
    assert!(app.take_events().iter().any(|event| matches!(event,
        crate::observe::AppEvent::PhysicsRefused(refusal)
            if refusal.law == PhysicsLaw::Density
    )));

    app.update(Action::SetPhysicsOverlay(
        PhysicsOverlay::Skeleton.id(),
        true,
    ));
    assert!(app.graph_runtimes.physics_overlays().is_empty());
    assert_eq!(
        app.physics_refusal().unwrap().refused,
        vec![PhysicsOverlay::Skeleton]
    );
    app.update(Action::ApplyPhysicsProfile("crystal"));
    assert!(app.physics_refusal().is_none());
    assert!(
        !arrange_rows(&app)
            .iter()
            .any(|row| row.starts_with("Physics refused:"))
    );
}

#[test]
fn current_refusal_attaches_to_the_platform_tree_and_clears_without_dom_sync() {
    use crate::panes::{PaneContent, PaneKindId, kind};
    use std::collections::HashMap;

    let mut app = App::test_stub();
    app.update(Action::SummonPane(PaneKindId::new(kind::ARRANGE)));
    let pane = app.frisket.iter_leaves().find_map(|(pane, content, _)| {
            matches!(content, PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::ARRANGE)
                .then_some(pane)
        }).expect("Arrange pane");
    app.update(Action::SetPhysicsOverlay(
        PhysicsOverlay::GridSnap.id(),
        true,
    ));
    app.update(Action::SetPhysicsLaw(PhysicsLaw::Density.id()));
    let bounds = crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0);
    let contribution = physics_refusal_tree(&app, pane, bounds).unwrap();
    let status_id = contribution.root;
    assert_ne!(
        status_id,
        physics_refusal_tree(&app, crate::panes::PaneId(pane.0 + 1), bounds)
            .unwrap()
            .root,
        "each pane owns a distinct stable identity"
    );
    let (tree, routes) = crate::a11y::project_app_with_routes_and_contributions(
        &app,
        HashMap::from([(pane, contribution)]),
    );
    let status = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == status_id)
        .unwrap()
        .1;
    assert_eq!(status.role(), accesskit::Role::Status);
    assert_eq!(status.live(), Some(accesskit::Live::Polite));
    assert_eq!(
        status.bounds(),
        Some(accesskit::Rect::new(100.0, 40.0, 580.0, 440.0))
    );
    let label = format!("Physics refused: {}", app.physics_refusal().unwrap());
    assert_eq!(status.label(), Some(label.as_str()));
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.children().contains(&status_id))
    );
    assert!(
        !routes.contains_key(&status_id),
        "read-only status has no action route"
    );

    app.update(Action::SetPhysicsOverlay(
        PhysicsOverlay::Skeleton.id(),
        true,
    ));
    assert_eq!(
        physics_refusal_tree(&app, pane, bounds).unwrap().root,
        status_id
    );
    app.update(Action::ApplyPhysicsProfile("crystal"));
    let contributions = physics_refusal_tree(&app, pane, bounds)
        .map(|tree| HashMap::from([(pane, tree)]))
        .unwrap_or_default();
    let (cleared, routes) =
        crate::a11y::project_app_with_routes_and_contributions(&app, contributions);
    assert!(!cleared.nodes.iter().any(|(id, _)| *id == status_id));
    assert!(!routes.contains_key(&status_id));
}

#[test]
fn intents_lower_to_the_palette_actions_by_id() {
    assert_eq!(
        ArrangeIntent::SetLayout("free".into()).into_action(),
        Some(Action::SetLayoutStrategy(None))
    );
    assert_eq!(
        ArrangeIntent::SetLayout("grid.default".into()).into_action(),
        Some(Action::SetLayoutStrategy(Some("grid.default")))
    );
    assert_eq!(
        ArrangeIntent::SetLaw("orbit.gravity".into()).into_action(),
        Some(Action::SetPhysicsLaw("orbit.gravity"))
    );
    assert_eq!(
        ArrangeIntent::SetOverlay {
            id: "skeleton".into(),
            on: true
        }
        .into_action(),
        Some(Action::SetPhysicsOverlay("skeleton", true))
    );
    assert_eq!(
        ArrangeIntent::ApplyProfile("crystal".into()).into_action(),
        Some(Action::ApplyPhysicsProfile("crystal"))
    );
    assert_eq!(ArrangeIntent::SetLaw("plasma".into()).into_action(), None);
    assert_eq!(
        ArrangeIntent::ApplyProfile("custom".into()).into_action(),
        None
    );
}

#[test]
fn composing_intents_lower_and_reach_the_canvas() {
    let mut app = App::test_stub();
    let mix = TwoLawMix {
        first: PhysicsLaw::Charge,
        second: PhysicsLaw::Springs,
        first_share: 0.5,
        second_share: 2.0,
    };
    for intent in [
        ArrangeIntent::SetRole("pinned".into()),
        ArrangeIntent::SetSpeed("5".into()),
        ArrangeIntent::ApplyMix(mix),
    ] {
        app.update(intent.into_action().expect("a known id lowers"));
    }
    let canvas = &app.graph_runtimes;
    assert_eq!(canvas.arrangement_roles().default, Role::Pinned);
    assert_eq!(SPEED_PRESETS[speed_preset(canvas)].0, "5");
    assert_eq!(read_two_law_mix(canvas), Ok(Some(mix)));
    let rows = arrange_rows(&app);
    for expected in ["Role: Pinned", "Speed: 5x", "Mix: Charge 0.5 + Springs 2"] {
        assert!(
            rows.iter().any(|row| row.starts_with(expected)),
            "{expected}: {rows:?}"
        );
    }
    assert!(
        rows.iter()
            .any(|row| row.starts_with("Physics status: Physics mix set"))
    );
    app.update(ArrangeIntent::UseSingleLaw.into_action().unwrap());
    assert_eq!(read_two_law_mix(&app.graph_runtimes), Ok(None));
    assert_eq!(ArrangeIntent::SetRole("boss".into()).into_action(), None);
    assert_eq!(ArrangeIntent::SetSpeed("3".into()).into_action(), None);
}

#[test]
fn flat_and_composed_profiles_follow_shared_root_aware_readback() {
    use mere::canvas::StageStop;
    use mere::canvas::physics_controls::{StageDraft, TwoLawGrouping, TwoLawMix, TwoStageSchedule};

    let mut flat = App::test_stub();
    for overlay in PhysicsOverlay::ALL {
        flat.update(Action::SetPhysicsOverlay(overlay.id(), false));
    }
    flat.update(Action::SetPhysicsLaw(PhysicsLaw::Charge.id()));
    assert!(arrange_rows(&flat).contains(&"Profile: Gas".to_string()));

    let mix = TwoLawMix {
        first: PhysicsLaw::Charge,
        second: PhysicsLaw::Springs,
        first_share: 1.0,
        second_share: 1.0,
    };
    let grouping = TwoLawGrouping {
        partition: mere::canvas::PhysicsKindSource::Component,
        outer: PhysicsLaw::Charge,
        inner: PhysicsLaw::Springs,
        outer_share: 1.0,
    };
    let schedule = TwoStageSchedule {
        first: StageDraft {
            law: PhysicsLaw::Charge,
            stop: StageStop::Frames(60),
            capture: None,
        },
        second: StageDraft {
            law: PhysicsLaw::Springs,
            stop: StageStop::LawDone,
            capture: None,
        },
        overlays: Vec::new(),
    };
    for composition in [
        Action::ApplyPhysicsMix(mix),
        Action::ApplyPhysicsGrouping(grouping),
        Action::ApplyPhysicsSchedule(schedule),
    ] {
        let mut app = App::test_stub();
        app.update(composition);
        let rows = arrange_rows(&app);
        assert!(rows.contains(&"Physics: Charge".to_string()), "{rows:?}");
        assert!(rows.contains(&"Profile: custom".to_string()), "{rows:?}");
    }
}

#[test]
fn canvas_verbs_share_one_label_with_the_catalogue_and_the_palette() {
    for verb in VERBS {
        let action = Action::from_canvas_verb(verb).expect("the pane offers what the shell has");
        assert_eq!(action.canvas_verb(), Some(verb));
        let id = crate::action::shared_command_id(&action).expect("a shared id");
        assert_eq!(cambium::catalogue::label(id), Some(verb.label()), "{id}");
        assert_eq!(ArrangeIntent::Verb(verb).into_action(), Some(action));
    }
}

#[test]
fn the_rows_read_the_canvas_back() {
    let mut app = App::test_stub();
    app.update(Action::SetPhysicsLaw("orbit.gravity"));
    app.update(Action::SetPhysicsOverlay("skeleton", true));
    let rows = arrange_rows(&app);
    assert!(rows.contains(&"Physics: Orbit".to_string()), "{rows:?}");
    assert!(rows.contains(&"Overlays: Skeleton".to_string()), "{rows:?}");
    assert!(rows.contains(&"Profile: custom".to_string()), "{rows:?}");
    app.update(Action::ApplyPhysicsProfile("crystal"));
    let rows = arrange_rows(&app);
    assert!(rows.contains(&"Physics: Stress".to_string()), "{rows:?}");
    assert!(rows.contains(&"Overlays: Grid".to_string()), "{rows:?}");
    assert!(rows.contains(&"Profile: Crystal".to_string()), "{rows:?}");
}
