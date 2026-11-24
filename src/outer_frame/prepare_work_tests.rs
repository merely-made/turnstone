// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn app() -> crate::app::App {
    let mut app = crate::app::App::test_stub();
    app.frisket.root = crate::panes::PaneNode::Leaf {
        pane_id: PaneId(90),
        content: crate::panes::PaneContent::Roster,
        graph_id: crate::panes::GraphId::nil(),
    };
    app.active_pane = Some(PaneId(90));
    app
}

#[test]
fn outer_frame_prepare_keeps_resized_scaled_and_current_geometry_without_paint() {
    let app = app();
    let mut frame = OuterFrame::default();
    let (observed, paints) = crate::ui::count_paint_calls(|| {
        [
            ((900, 600), 1.0, Some(PaneId(90))),
            ((900, 600), 1.0, Some(PaneId(90))),
            ((1200, 800), 1.0, Some(PaneId(90))),
            ((1200, 800), 2.0, Some(PaneId(90))),
            ((1200, 800), 2.0, None),
        ]
        .into_iter()
        .map(|(size, scale, current)| {
            frame.set_layout_scale(scale);
            let bodies = frame
                .prepare(
                    &app.frisket,
                    current,
                    None,
                    size,
                    &app.shell_chrome_config().appearance,
                    app.session_id.as_uuid().as_u128(),
                )
                .expect("actual frame preparation");
            assert_eq!(bodies.len(), 1);
            assert_eq!(bodies[0].0, PaneId(90));
            let dom = frame.controls.dom.borrow();
            let layout = frame.layout.borrow();
            assert!(layout.has_current_snapshot(&dom));
            assert_eq!(
                layout.painted_rect(&dom, frame.window_root),
                Some((0.0, 0.0, size.0 as f32, size.1 as f32)),
                "window geometry reflects the current physical size and scale"
            );
            let body = bodies[0].1;
            assert!(body.w > 0.0 && body.h > 0.0);
            (body, frame.header_height())
        })
        .collect::<Vec<_>>()
    });
    assert_eq!(observed[0], observed[1], "unchanged preparation is stable");
    assert!(observed[2].0.w > observed[1].0.w);
    assert!(observed[2].0.h > observed[1].0.h);
    assert!(
        observed[3].1 > observed[2].1,
        "the same physical window has a scaled header after reflow"
    );
    assert_eq!(
        paints, 0,
        "geometry preparation must not emit and discard a complete frame scene"
    );
}

#[test]
fn prepared_geometry_follows_workspace_area_and_session() {
    let app = app();
    let mut frame = OuterFrame::default();
    let prepare = |frame: &mut OuterFrame, session| {
        frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (900, 600),
                &app.shell_chrome_config().appearance,
                session,
            )
            .unwrap()
    };
    let original = prepare(&mut frame, 1);
    assert_eq!(prepare(&mut frame, 1), original);
    assert_eq!(prepare(&mut frame, 1), original);
    assert_eq!(frame.prepared_geometry.as_ref().unwrap().bodies, original);
    let previous = frame.stamp();

    frame.set_workspace_area(Rect::new(36.0, 0.0, 864.0, 600.0));
    let moved = prepare(&mut frame, 1);
    assert!(moved[0].1.x > original[0].1.x);
    assert_eq!(prepare(&mut frame, 1), moved);

    assert_eq!(prepare(&mut frame, 2), moved);
    assert_ne!(previous.admission, frame.stamp().admission);
}

#[test]
fn outer_frame_explicit_scene_resources_emits_exactly_one_paint() {
    let app = app();
    let mut frame = OuterFrame::default();
    frame
        .prepare(
            &app.frisket,
            app.active_pane,
            None,
            (900, 600),
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap();
    let (translated, paints) = crate::ui::count_paint_calls(|| frame.scene_resources());
    let translated = translated.expect("explicit scene and resource production");
    assert_eq!(paints, 1);
    let scene: serde_json::Value = serde_json::from_str(&translated.scene.snapshot_json()).unwrap();
    assert!(!scene["ops"].as_array().unwrap().is_empty());
}

#[test]
fn settled_pane_reuses_native_projection_until_geometry_focus_or_routes_change() {
    let app = app();
    let mut frame = OuterFrame::default();
    let prepare = |frame: &mut OuterFrame, size| {
        frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                size,
                &app.shell_chrome_config().appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .unwrap();
    };
    prepare(&mut frame, (900, 600));
    let first = frame.accessibility(false).unwrap();
    let source = frame
        .accessibility
        .borrow()
        .as_ref()
        .unwrap()
        .source
        .clone();
    let (repeat, probes) = crate::ui::count_semantic_probes(|| frame.accessibility(false).unwrap());
    assert_eq!(
        probes, 0,
        "a settled pane must skip DOM action-map lowering"
    );
    assert_eq!(repeat.nodes, first.nodes);
    assert_eq!(repeat.routes, first.routes);

    prepare(&mut frame, (1200, 800));
    let resized = frame.accessibility(false).unwrap();
    assert_ne!(resized.nodes, first.nodes);
    assert!(!Rc::ptr_eq(
        &source,
        &frame.accessibility.borrow().as_ref().unwrap().source
    ));
    let focused = frame.accessibility(true).unwrap();
    assert!(focused.focus.is_some());
    assert!(
        frame
            .accessibility
            .borrow()
            .as_ref()
            .unwrap()
            .header_focused
    );

    frame.controls.invalidate();
    let new_routes = frame.accessibility(true).unwrap().routes;
    assert!(!focused.routes.is_empty());
    assert!(
        focused
            .routes
            .values()
            .all(|route| !frame.controls.admits(route))
    );
    assert!(
        new_routes
            .values()
            .all(|route| frame.controls.admits(route))
    );
}
