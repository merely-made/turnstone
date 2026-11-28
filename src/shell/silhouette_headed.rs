// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cartography::canvas::{Canvas, NodeShape};
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;
use kernel::graph::RelationClaim;
use kernel::graph::apply::{add_node, assert_relation};
use mere::profile::core::ids as core_ids;
use scenograph::presentation::default_graph_representation_registry;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;
use winit::{
    event_loop::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    platform::windows::EventLoopBuilderExtWindows,
};

#[test]
#[ignore = "opens a native window; run with isolated TURNSTONE_ROOT and TURNSTONE_CAPTURE_DIR"]
fn native_canvas_picks_and_attaches_to_polygon_silhouettes() {
    let profile = PathBuf::from(std::env::var_os("TURNSTONE_ROOT").expect("isolated profile"));
    let output =
        PathBuf::from(std::env::var_os("TURNSTONE_CAPTURE_DIR").expect("capture directory"));
    assert!(
        profile.is_absolute() && !profile.exists(),
        "use a fresh profile"
    );
    assert!(output.is_absolute());
    assert!(
        !output.join("diamond-and-hexagon.png").exists(),
        "use a fresh capture"
    );

    let mut event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut shell = Shell::new(event_loop.create_proxy(), None);
    if let Some((setup, _)) = shell.custody_setup.take() {
        setup.request_stop();
    }
    let members = [
        (
            Uuid::from_u128(0x101),
            "event",
            NodeShape::Diamond,
            (-170.0, -100.0),
        ),
        (
            Uuid::from_u128(0x102),
            "software",
            NodeShape::Hexagon,
            (170.0, 100.0),
        ),
    ];
    let mut graph = Graph::new();
    let registry = default_graph_representation_registry();
    let mut shapes = std::collections::HashMap::new();
    let keys = members.map(|(member, class, expected, (x, y))| {
        let profile = registry.resolve(class);
        let shape = profile.primitive.body;
        assert_eq!(shape, expected);
        shapes.insert(member, shape);
        add_node(
            &mut graph,
            Some(member),
            format!("mere://{class}"),
            PortablePoint::new(x, y),
        )
    });
    assert_relation(
        &mut graph,
        keys[0],
        keys[1],
        RelationClaim::of(core_ids::REL_HYPERLINK),
    )
    .expect("fixture link");
    let mut canvas = Canvas::with_graph(graph);
    for (member, ..) in members {
        canvas.set_node_size(member, 130.0);
    }
    canvas.set_physics_paused(true);
    let positions = [0, 1].map(|index| {
        let (x, y) = members[index].3;
        (keys[index], PortablePoint::new(x, y))
    });
    canvas.land_paused_positions(&positions);
    canvas.set_node_shapes(shapes);
    shell.app.graph_runtimes.replace_active_canvas(canvas);
    pump_until(&mut event_loop, &mut shell, |shell| {
        shell.main_receipt.presented > 0
    });
    taproot::Driveable::app_step(&mut shell, "key escape").unwrap();

    let pane = shell.app.graph_command_pane().expect("graph pane");
    shell
        .app
        .with_graph_pane(pane, |canvas| canvas.fit_to_content())
        .unwrap();
    let frames = shell.main_receipt.presented;
    pump_until(&mut event_loop, &mut shell, |shell| {
        shell.main_receipt.presented > frames
    });
    let surface = shell
        .surface_plan()
        .into_iter()
        .find(|surface| surface.kind == crate::surface::SurfaceKind::Graph(pane))
        .expect("visible graph pane")
        .rect;
    let scale = shell.app.pane_layout_scale(pane);

    for (member, class, shape, _) in members {
        let (center, clipped_corner, distance) = shell
            .app
            .with_graph_pane(pane, |canvas| {
                canvas.clear_selection();
                let key = canvas.graph().get_node_key_by_id(member).unwrap();
                let body = canvas.node_billboard(key).unwrap();
                assert_eq!(body.silhouette, shape);
                let side = body.local_side * body.zoom;
                let path = canvas
                    .visible_link_paths()
                    .into_iter()
                    .next()
                    .expect("painted link");
                let [a, b] = path.points.as_slice() else {
                    panic!("expected a straight link")
                };
                let tip = if path.displayed_from == member {
                    *a
                } else {
                    *b
                };
                let (x, y) = canvas.screen_point_of(tip);
                let distance = body.signed_distance_screen((x, y).into()).unwrap();
                assert!(
                    distance.abs() < 0.05,
                    "{class} link tip is {distance}px from the body"
                );
                let center = (body.center.x, body.center.y);
                (
                    center,
                    (center.0 + 0.44 * side, center.1 + 0.44 * side),
                    distance,
                )
            })
            .unwrap();

        for (point, expected) in [(clipped_corner, None), (center, Some(member))] {
            let x = surface.x + point.0 * scale;
            let y = surface.y + point.1 * scale;
            assert!(surface.contains(x, y));
            shell.deliver_move(x, y);
            shell.deliver_press(x, y, winit::event::MouseButton::Left);
            shell.deliver_release(x, y, winit::event::MouseButton::Left);
            let selected = shell
                .app
                .with_graph_pane(pane, |canvas| canvas.selected_members())
                .unwrap();
            assert!(
                selected.iter().copied().eq(expected),
                "{class} pointer at ({x}, {y}): {selected:?}"
            );
        }
        println!(
            "{class} {shape:?}: corner missed, center selected, link tip {distance:.4}px from outline"
        );
    }

    shell
        .app
        .with_graph_pane(pane, |canvas| canvas.clear_selection())
        .unwrap();
    assert!(taproot::Driveable::capture(
        &mut shell,
        "diamond-and-hexagon"
    ));
    pump_until(&mut event_loop, &mut shell, |shell| {
        !shell.main_receipt.captures.is_empty()
    });
    let [(capture, _, false)] = shell.main_receipt.captures.as_slice() else {
        panic!("expected one nonblank capture");
    };
    assert_eq!(capture, &output.join("diamond-and-hexagon.png"));
}

fn pump_until(
    event_loop: &mut winit::event_loop::EventLoop,
    shell: &mut Shell,
    done: impl Fn(&Shell) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(shell) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for native presentation"
        );
        assert!(
            shell.main_receipt.errors.is_empty(),
            "{:?}",
            shell.main_receipt.errors
        );
        shell.request_redraw();
        assert!(matches!(
            event_loop.pump_app_events(Some(Duration::from_millis(25)), &mut *shell),
            PumpStatus::Continue
        ));
    }
}
