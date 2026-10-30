// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn review_overlap_native_reader_appearances_wheel_mutates_only_top_view() {
    use super::super::super::content_pool::{HostPlacement, NodeDocuments};
    let engines = Arc::new(super::super::super::standard_content_engines());
    let surface_engines = Arc::new(inker::SurfaceEngineRegistry::new());
    let epoch = std::time::Instant::now();
    let body = format!("<html><body><main><h1>Article</h1>{}</main></body></html>",
        "<p>This substantial article has enough words for independent retained Reader views and scrolling.</p>".repeat(150));
    let request = inker::SessionSpawnRequest::new("https://fixture.test/story")
        .with_body(body)
        .with_content_type("text/html")
        .with_viewport(640, 480);
    let decision = super::super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("overlap-reader-test"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: request.content_type.clone(),
        pinned_engine: Some(inker::routing::ENGINE_GENET_READER.into()),
    });
    let controller = pelt_core::PeltController::new_shared(
        engines.clone(),
        surface_engines.clone(),
        pelt_core::PeltControllerConfig::from_request(decision.engine_id.clone(), request)
            .with_host_history(),
        super::super::super::ShellClock(epoch),
    )
    .unwrap();
    let node = uuid::Uuid::from_u128(7);
    let mut contents = NodeDocuments::default();
    contents.insert(
        node,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    let plan = vec![
        Surface {
            id: SurfaceId(10),
            kind: SurfaceKind::Content(node),
            rect: Rect::new(0., 0., 320., 200.),
        },
        Surface {
            id: SurfaceId(11),
            kind: SurfaceKind::Content(node),
            rect: Rect::new(20., 0., 320., 200.),
        },
    ];
    contents.sync_placements(
        plan.iter()
            .map(|surface| HostPlacement {
                surface: *surface,
                window: 0,
                focused: false,
                scale_factor: 1.,
                reading_preferences: Default::default(),
                reading_anchor: None,
                color_mode: mere_document_lanes::ModeProfile::LIGHT,
            })
            .collect(),
        engines,
        surface_engines,
        epoch,
    );
    let keys = plan
        .iter()
        .map(|surface| contents.pool.placement(&surface.id).unwrap().content)
        .collect::<Vec<_>>();
    assert_ne!(keys[0], keys[1]);
    for surface in &plan {
        assert!(contents.pool.frame(&surface.id).is_some());
    }
    let reader = |key: &ContentKey| {
        contents
            .pool
            .content(key)
            .unwrap()
            .document()
            .unwrap()
            .session()
            .as_any_ref()
            .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
            .unwrap()
    };
    assert!(Arc::ptr_eq(
        &reader(&keys[0]).source_document(),
        &reader(&keys[1]).source_document()
    ));
    assert_eq!(
        crate::surface::hit_test(&plan, FocusTarget::Graph(crate::panes::PaneId(99)), 27., 9.)
            .unwrap()
            .id,
        SurfaceId(11)
    );
    assert!(
        contents
            .pool
            .input_at(
                0,
                PeltInput::Wheel {
                    position: PhysicalPosition { x: 27., y: 9. },
                    delta: (0., 120.),
                    modifiers: Default::default()
                }
            )
            .handled
    );
    let reader = |key: &ContentKey| {
        contents
            .pool
            .content(key)
            .unwrap()
            .document()
            .unwrap()
            .session()
            .as_any_ref()
            .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
            .unwrap()
    };
    assert_eq!(
        reader(&keys[0]).scroll_y(),
        0.,
        "covered Reader appearance retains its own state"
    );
    assert!(
        reader(&keys[1]).scroll_y() > 0.,
        "top Reader appearance receives wheel"
    );
}

#[test]
fn a_scaled_window_delivers_producer_pointers_in_physical_local_pixels() {
    // The pool places content in layout pixels and the host divides physical
    // pointers once; the surface producer, which owns physical pixels,
    // receives the same local point it did before windows composed at a scale.
    let mut f = CallerCaptureFixture::new(0, None);
    f.scale = 2.;
    f.sync(true);
    let rect = f.surfaces[0].rect;
    let placement = f
        .contents
        .pool
        .placement(&f.surfaces[0].id)
        .unwrap()
        .clone();
    assert_eq!(placement.scale_factor, 2.);
    assert_eq!(
        (placement.rect.x, placement.rect.width),
        (rect.x / 2., rect.w / 2.),
        "the pool's rect is the physical plan over the layout scale"
    );
    assert!(f.press(0, inker::MouseButton::Left).1.handled);
    let delivered = f.pointers[0].lock().unwrap().last().unwrap().position;
    assert!((delivered.x - rect.w / 2.).abs() < 0.01, "{delivered:?}");
    assert!((delivered.y - rect.h / 2.).abs() < 0.01, "{delivered:?}");
}

#[test]
fn review_caller_capture_primary_second_down_preserves_custody() {
    caller_capture_gesture(0);
}

#[test]
fn review_caller_capture_lens_second_down_preserves_custody() {
    caller_capture_gesture(1);
}

#[test]
fn review_caller_capture_primary_navigation_names_first_receiver() {
    caller_capture_effect(0, false);
}

#[test]
fn review_caller_capture_lens_navigation_names_first_receiver() {
    caller_capture_effect(1, false);
}

#[test]
fn review_caller_capture_primary_submission_names_first_receiver() {
    caller_capture_effect(0, true);
}

#[test]
fn review_caller_capture_lens_submission_names_first_receiver() {
    caller_capture_effect(1, true);
}

#[test]
fn review_caller_capture_primary_actual_retirement_allows_new_receiver() {
    caller_capture_retirement(0);
}

#[test]
fn review_caller_capture_lens_actual_retirement_allows_new_receiver() {
    caller_capture_retirement(1);
}

#[test]
fn review_primary_pane_chord_left_up_then_middle_up_retires_actual_receiver() {
    primary_pane_chord(winit::event::MouseButton::Left);
}

#[test]
fn review_primary_pane_chord_middle_up_then_left_up_retires_actual_receiver() {
    primary_pane_chord(winit::event::MouseButton::Middle);
}

#[test]
fn review_primary_pane_chord_genuine_focus_loss_cancels_and_allows_new_receiver() {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    let pane = f.pane_position();
    f.press(pane, Middle);
    f.focus_lost();
    f.release(pane, Left);
    f.release(pane, Middle);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    let events = f.content.pointers[0].lock().unwrap().clone();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.phase == PointerPhase::Cancel)
            .count(),
        1
    );
    assert_eq!(events.last().unwrap().phase, PointerPhase::Cancel);
    assert!(!events.iter().any(|e| e.phase == PointerPhase::Up));
    assert!(f.content.pointers[1].lock().unwrap().is_empty());

    f.content.sync(true);
    let b = f.content.position(1);
    f.press(b, Left);
    f.release(b, Left);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.content.pointers[0].lock().unwrap().len(), events.len());
    assert_eq!(
        f.content.pointers[1]
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.phase, e.button))
            .collect::<Vec<_>>(),
        vec![
            (PointerPhase::Down, Some(inker::MouseButton::Left)),
            (PointerPhase::Up, Some(inker::MouseButton::Left)),
        ]
    );
    assert_eq!(
        &f.delivered[f.delivered.len() - 2..],
        &[(ContentKey::Node(f.content.nodes[1]), true); 2]
    );
}

#[test]
fn review_primary_no_hit_chord_left_up_then_middle_up_retires_actual_receiver() {
    primary_no_hit_chord(winit::event::MouseButton::Left);
}

#[test]
fn review_primary_no_hit_chord_middle_up_then_left_up_retires_actual_receiver() {
    primary_no_hit_chord(winit::event::MouseButton::Middle);
}

#[test]
fn review_primary_modal_rejection_cancels_before_shell_clear_without_later_up() {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    let admitted = f.capture;
    assert_eq!(admitted, Some(SurfaceKind::Content(f.content.nodes[0])));
    let request = f.open_primary_decision();
    f.content.assert_a_custody();
    f.press_primary_outside_decision_card();
    assert_eq!(
        f.cancellation_capture,
        vec![admitted],
        "the actual gate must cancel A while its shell capture still exists"
    );
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    assert_eq!(
        f.content.buttons[&0], both,
        "interception is not physical Up"
    );
    let phases = || {
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>()
    };
    assert_eq!(phases(), vec![PointerPhase::Down, PointerPhase::Cancel]);
    // No redraw/sync between rejection and actual terminal input.
    f.release(a, Left);
    assert_eq!(f.content.buttons[&0], PointerButtons::AUXILIARY);
    f.release(a, Middle);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Cancel]
    );
    assert!(f.content.pointers[1].lock().unwrap().is_empty());
    assert!(f.content.app.user_agent_decision.remove(request));
    let b = f.content.position(1);
    f.press(b, Left);
    f.release(b, Left);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    assert_eq!(f.content.pointers[0].lock().unwrap().len(), 2);
    assert_eq!(
        f.content.pointers[1]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Up]
    );
}

#[test]
fn review_primary_modal_rejection_preserves_real_lens_custody() {
    let mut f = PrimaryPaneCaptureFixture {
        content: CallerCaptureFixture::new(1, None),
        capture: None,
        delivered: Vec::new(),
        cancellation_capture: Vec::new(),
    };
    assert!(f.content.press(0, inker::MouseButton::Left).1.handled);
    f.capture = Some(SurfaceKind::Content(f.content.nodes[0]));
    let admitted = f.capture;
    let appearance = f.content.surfaces[0].id;
    assert_eq!(
        f.content
            .contents
            .pool
            .placement(&appearance)
            .unwrap()
            .window,
        1
    );
    f.open_primary_decision();
    f.press_primary_outside_decision_card();
    assert_eq!(captured_mouse(&f.content.contents.pool), Some(appearance));
    assert_eq!(
        f.capture, admitted,
        "a primary modal must preserve the lens shell mirror"
    );
    assert_eq!(f.content.buttons[&1], PointerButtons::PRIMARY);
    assert_eq!(f.content.buttons[&0], PointerButtons::AUXILIARY);
    assert_eq!(
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down]
    );
    // Release the rejected primary button without entering the lens receiver.
    assert!(
        native_mouse_input(
            &mut f.content.contents.pool,
            &mut f.content.buttons,
            0,
            None,
            PhysicalPosition {
                x: -100.0,
                y: -100.0
            },
            PointerPhase::Up,
            Some(inker::MouseButton::Middle),
            Default::default(),
        )
        .is_none()
    );
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(captured_mouse(&f.content.contents.pool), Some(appearance));
    let (source, effect) = f
        .content
        .pointer(PointerPhase::Up, Some(inker::MouseButton::Left));
    assert_eq!(source, ContentKey::Node(f.content.nodes[0]));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.content.buttons[&1], PointerButtons::NONE);
    assert_eq!(
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Up]
    );
    assert!(f.content.pointers[1].lock().unwrap().is_empty());
}

#[test]
fn review_primary_modal_rejection_closed_gate_preserves_content_custody() {
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, winit::event::MouseButton::Left);
    let admitted = f.capture;
    assert!(!super::super::super::input::reject_primary_modal_press(
        &mut f
    ));
    assert_eq!(f.capture, admitted);
    assert!(f.cancellation_capture.is_empty());
    f.content.assert_a_custody();
    f.release(a, winit::event::MouseButton::Left);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Up]
    );
}

#[test]
fn review_native_interception_chrome_retires_before_action_and_drop() {
    let (mut f, lifetime) = interception_fixture();
    interception_bookkeeping(&mut f, winit::event::MouseButton::Middle);
    let node = f.content.nodes[0];
    assert!(super::super::super::input::dispatch_primary_chrome_press(
        &mut f,
        Some(crate::chrome_view::ChromeIntent::KeepNode(node))
    ));
    // Actual KeepNode preserves the content focus; it must stay logically
    // focused while the pressed gesture alone is retired.
    assert_eq!(
        f.content.app.focus,
        FocusTarget::Content {
            node,
            appearance: f.content.surfaces[0].id
        }
    );
    assert_interception_retired(&mut f, &lifetime, winit::event::MouseButton::Middle);
    drop(f.content.contents.remove(&node).unwrap());
    assert_eq!(
        *lifetime.lock().unwrap(),
        vec![
            "Cancel".to_string(),
            "focus false".to_string(),
            "drop".to_string()
        ],
        "receiver Cancel precedes actual removal focus loss and producer Drop"
    );
    // The same primary Chrome admission must not clear a Lens-owned mirror.
    let mut foreign = PrimaryPaneCaptureFixture {
        content: CallerCaptureFixture::new(1, None),
        capture: None,
        delivered: Vec::new(),
        cancellation_capture: Vec::new(),
    };
    assert!(
        foreign
            .content
            .press(0, inker::MouseButton::Right)
            .1
            .handled
    );
    foreign.capture = Some(SurfaceKind::Content(foreign.content.nodes[0]));
    let mirror = foreign.capture;
    let node = foreign.content.nodes[0];
    interception_bookkeeping(&mut foreign, winit::event::MouseButton::Middle);
    assert!(super::super::super::input::dispatch_primary_chrome_press(
        &mut foreign,
        Some(crate::chrome_view::ChromeIntent::KeepNode(node))
    ));
    assert_eq!(foreign.capture, mirror);
    assert_eq!(
        captured_mouse(&foreign.content.contents.pool),
        Some(foreign.content.surfaces[0].id)
    );
    assert!(
        foreign
            .content
            .pointer(PointerPhase::Up, Some(inker::MouseButton::Right))
            .1
            .handled
    );
}

#[test]
fn review_native_interception_omnibar_swallow_retires_before_action() {
    let (mut f, lifetime) = interception_fixture();
    f.content
        .app
        .update(crate::action::Action::OmnibarOpen { command: false });
    assert!(f.content.app.omnibar.open);
    interception_bookkeeping(&mut f, winit::event::MouseButton::Middle);
    assert!(super::super::super::input::reject_primary_omnibar_press(
        &mut f
    ));
    assert!(!f.content.app.omnibar.open);
    assert_interception_retired(&mut f, &lifetime, winit::event::MouseButton::Middle);
}

#[test]
fn review_native_interception_context_swallow_retires_before_action() {
    let (mut f, lifetime) = interception_fixture();
    let pos = f.content.position(1);
    let hit = crate::surface::hit_test(&f.content.plan(), f.content.app.focus, pos.x, pos.y);
    assert!(matches!(hit.unwrap().kind, SurfaceKind::Content(_)));
    interception_bookkeeping(&mut f, winit::event::MouseButton::Right);
    assert!(super::super::super::input::dispatch_primary_context_press(
        &mut f,
        winit::event::MouseButton::Right,
        hit
    ));
    assert!(f.content.app.omnibar.open);
    assert_interception_retired(&mut f, &lifetime, winit::event::MouseButton::Right);
}
