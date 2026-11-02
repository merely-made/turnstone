// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::surface::{Surface, SurfaceId, SurfaceKind};
use layout_dom_api::LayoutDomMut;

#[test]
fn frame_selector_rejects_body_slots_and_uses_header_hit() {
    let mut app = crate::app::App::test_stub();
    app.frisket.root = crate::panes::PaneNode::Leaf {
        pane_id: PaneId(90),
        content: crate::panes::PaneContent::Roster,
        graph_id: crate::panes::GraphId::nil(),
    };
    app.active_pane = Some(PaneId(90));
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
        .expect("retained frame");
    assert!(
        frame
            .selector_point(&taproot::Selector::class("frisket-tab"))
            .expect("painted header")
            .is_some()
    );
    assert_eq!(
        frame
            .selector_point(&taproot::Selector::class("frisket-content"))
            .expect("body slot is not frame furniture"),
        None
    );
}

#[test]
fn read_map_stack_has_no_redundant_header_but_lens_keeps_its_controls() {
    let mut app = crate::app::App::test_stub();
    app.update(crate::action::Action::OpenAddress(
        "https://example.test/a".into(),
    ));
    app.update(crate::action::Action::SetWorkspaceMap(false));
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
    assert_eq!(frame.header_height(), 0.0);
    assert!(
        frame
            .selector_point(&taproot::Selector::class("frisket-tab"))
            .unwrap()
            .is_none()
    );
    frame
        .prepare(
            &app.frisket,
            app.active_pane,
            app.active_pane,
            (900, 600),
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap();
    assert_eq!(
        frame.header_height(),
        0.0,
        "maximizing Read keeps the shared toolbar as its header"
    );
    app.update(crate::action::Action::SetWorkspaceMap(true));
    let bodies = frame
        .prepare(
            &app.frisket,
            app.active_pane,
            None,
            (900, 600),
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap();
    assert_eq!(bodies.len(), 1, "retained reader has no Map aperture");
    assert_eq!(bodies[0].0, app.default_graph_pane());
    frame.set_main_view_switch(false);
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
    assert!(
        frame.header_height() > 0.0,
        "a lens without Read/Map retains frame tabs"
    );
}
#[test]
fn compact_headers_and_hairline_dividers_keep_generous_pointer_targets() {
    for side in [
        crate::panes::InsertSide::Right,
        crate::panes::InsertSide::Below,
    ] {
        let mut app = crate::app::App::test_stub();
        app.frisket.summon_leaf(
            &[],
            side,
            crate::panes::PaneNode::Leaf {
                pane_id: PaneId(90),
                content: crate::panes::PaneContent::Roster,
                graph_id: crate::panes::GraphId::nil(),
            },
        );
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
        assert!(
            frame.header_height() <= 36.0,
            "compact default header: {}",
            frame.header_height()
        );
        let control = frame
            .controls
            .order()
            .into_iter()
            .find(|c| matches!(c, Control::Divider { .. }))
            .unwrap();
        let node = frame.controls.node(&control).unwrap();
        let (x, y, w, h) = frame
            .layout
            .borrow()
            .painted_rect(&frame.controls.dom.borrow(), node)
            .unwrap();
        let vertical = matches!(side, crate::panes::InsertSide::Right);
        assert_eq!(if vertical { w } else { h }, 1.0);
        for offset in [-5.0, 0.0, 5.0] {
            let (px, py) = if vertical {
                (x + w / 2.0 + offset, y + h / 2.0)
            } else {
                (x + w / 2.0, y + h / 2.0 + offset)
            };
            assert!(
                matches!(frame.hit(px, py), Some(HeaderHit::Divider { .. })),
                "grab at {offset} on {side:?}"
            );
        }
    }
}
#[test]
fn the_final_floating_canvas_keeps_one_actual_native_aperture() {
    let mut app = crate::app::App::test_stub();
    let pane = PaneId(0);
    app.active_pane = Some(pane);
    app.update(crate::action::Action::FloatActivePane);
    let mut frame = OuterFrame::default();
    let docked = frame
        .prepare(
            &app.frisket,
            app.active_pane,
            None,
            (1180, 800),
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap();
    assert!(docked.is_empty(), "the empty stack has no source placement");
    let floats = frame.floats();
    assert_eq!(floats.len(), 1);
    assert_eq!(floats[0].0, pane);
    let surface = Surface {
        id: SurfaceId::graph(pane),
        kind: SurfaceKind::Graph(pane),
        rect: floats[0].2,
    };
    let slots = body_slots(&frame.controls.dom.borrow(), frame.controls.root());
    assert_eq!(
        slots.len(),
        1,
        "only the real floating owner has a marked body"
    );
    assert_eq!(
        frame.native_source_node(&app, &surface, 0),
        Some(slots[0].2)
    );
    frame.sheet.push_str(
        "\n.workspace-float{opacity:0.55;box-shadow:10px 12px 18px rgba(255,64,0,0.75);}",
    );
    let translated = frame.scene_resources().unwrap();
    let root_rect = frame
        .layout
        .borrow()
        .painted_rect(&frame.controls.dom.borrow(), frame.window_root)
        .unwrap();
    assert_eq!(
        root_rect,
        (0.0, 0.0, 1180.0, 800.0),
        "all-floating stations keep the actual window root viewport"
    );
    assert_eq!(translated.box_shadow_masks.len(), 1);
    let mask = &translated.box_shadow_masks[0];
    assert!(mask.dim > 0 && mask.blur_radius_px > 0.0);
    let scene: serde_json::Value = serde_json::from_str(&translated.scene.snapshot_json()).unwrap();
    let ops = scene["ops"].as_array().unwrap();
    assert!(
        ops.iter().any(|op| {
            let rect = &op["Rect"];
            rect["x0"].as_f64() == Some(0.0)
                && rect["y0"].as_f64() == Some(0.0)
                && rect["x1"].as_f64() == Some(1180.0)
                && rect["y1"].as_f64() == Some(800.0)
                && rect["color"][3].as_f64() == Some(1.0)
        }),
        "the authored window backdrop is emitted outside floated apertures"
    );
    assert!(
        ops.iter()
            .any(|op| op["Image"]["key"].as_u64() == Some(mask.key)),
        "the actual mask resource stays paired with its scene image"
    );
    assert!(
        ops.iter().any(|op| op["PushLayer"]["alpha"]
            .as_f64()
            .is_some_and(|alpha| (alpha - 0.55).abs() < 0.00001)),
        "authored float opacity stays in the scene layer stack"
    );
}
#[test]
fn identical_source_rectangles_keep_exact_appearance_owners() {
    let app = crate::app::App::test_stub();
    let frame = OuterFrame::default();
    let node = uuid::Uuid::from_u128(71);
    let slot = 2;
    let roles = [0x2000_0000_0000_0011, 0x2000_0000_0000_0022];
    let mut owners = Vec::new();
    {
        let mut dom = frame.controls.dom.borrow_mut();
        for role in roles {
            let owner = dom.create_element(cambium::html_qual("div"));
            dom.set_attribute(
                owner,
                cambium::attr_qual("data-native-node"),
                &node.to_string(),
            );
            dom.set_attribute(
                owner,
                cambium::attr_qual("data-native-role"),
                &role.to_string(),
            );
            dom.append_child(frame.controls.root(), owner);
            owners.push(owner);
        }
    }
    for (index, role) in roles.into_iter().enumerate() {
        let surface = Surface {
            id: SurfaceId::content_appearance(
                node,
                role ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            ),
            kind: SurfaceKind::Content(node),
            rect: Rect::new(40.0, 60.0, 240.0, 180.0),
        };
        assert_eq!(
            frame.native_source_node(&app, &surface, slot),
            Some(owners[index])
        );
        assert_eq!(frame.native_source_node(&app, &surface, slot + 1), None);
    }
    let duplicate = frame
        .controls
        .dom
        .borrow_mut()
        .create_element(cambium::html_qual("div"));
    {
        let mut dom = frame.controls.dom.borrow_mut();
        dom.set_attribute(
            duplicate,
            cambium::attr_qual("data-native-node"),
            &node.to_string(),
        );
        dom.set_attribute(
            duplicate,
            cambium::attr_qual("data-native-role"),
            &roles[0].to_string(),
        );
        dom.append_child(frame.controls.root(), duplicate);
    }
    let ambiguous = Surface {
        id: SurfaceId::content_appearance(
            node,
            roles[0] ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        ),
        kind: SurfaceKind::Content(node),
        rect: Rect::new(40.0, 60.0, 240.0, 180.0),
    };
    assert_eq!(frame.native_source_node(&app, &ambiguous, slot), None);
}
