// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{outer_frame::OuterFrame, panes::PaneId, surface::Surface};

fn fixture(lens: bool, floating: bool) -> (OuterFrame, Vec<Surface>) {
    let mut app = crate::app::App::test_stub();
    app.active_pane = Some(PaneId(0));
    if floating {
        app.update(crate::action::Action::FloatActivePane);
    }
    if lens {
        app.lenses.push(Some(app.frisket.clone()));
    }
    let layout = if lens {
        app.lenses[0].as_ref().unwrap()
    } else {
        &app.frisket
    };
    let mut frame = OuterFrame::default();
    frame.set_main_view_switch(!lens);
    frame
        .prepare(
            layout,
            app.active_pane,
            None,
            (640, 480),
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap();
    let contents = crate::shell::NodeDocuments::default();
    let plan = if lens {
        crate::shell::lens::lens_content_plan(&app, &contents, 0, 640, 480)
    } else {
        crate::shell::primary_content_plan(&app, &contents, 640, 480)
    };
    assert_eq!(
        plan.iter().filter(|s| s.kind == SurfaceKind::Frame).count(),
        1,
        "the actual shared window plan retains one frame owner"
    );
    assert_eq!(
        plan.iter()
            .filter(|s| matches!(s.kind, SurfaceKind::FloatFrame(_)))
            .count(),
        usize::from(floating),
        "actual floating frame crops remain in the source plan"
    );
    (frame, plan)
}

#[test]
fn shared_primary_and_lens_plans_do_not_paint_discarded_frame_scenes() {
    let mut windows = [(false, false), (false, true), (true, false), (true, true)]
        .map(|(lens, floating)| fixture(lens, floating));
    let (planned, paints) = crate::ui::count_paint_calls(|| {
        let mut planned = Vec::new();
        for (frame, plan) in &mut windows {
            let shared = plan.iter().any(|s| s.kind == SurfaceKind::Frame);
            for surface in plan {
                if matches!(
                    surface.kind,
                    SurfaceKind::Frame | SurfaceKind::FloatFrame(_)
                ) {
                    // Both real render loops use this lazy scene-construction
                    // seam; the callback emits the actual retained frame paint.
                    planned.push(Shell::frame_scene_for_plan(shared, || frame.scene()));
                }
            }
        }
        planned
    });
    assert_eq!(
        paints, 0,
        "shared native composition must not build scenes it discards before rasterization"
    );
    assert_eq!(planned.len(), 6, "docked owners plus actual float crops");
    assert!(planned.iter().all(Option::is_none));
}

#[test]
fn standalone_frame_scene_still_paints_once_without_shared_composition() {
    let (mut frame, _) = fixture(false, false);
    let (scene, paints) =
        crate::ui::count_paint_calls(|| Shell::frame_scene_for_plan(false, || frame.scene()));
    let scene = scene.expect("standalone frame keeps its existing paint path");
    assert_eq!(paints, 1);
    assert!(!scene.ops.is_empty());
}
