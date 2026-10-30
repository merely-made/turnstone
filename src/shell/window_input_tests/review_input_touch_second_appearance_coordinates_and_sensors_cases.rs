// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn review_input_touch_second_appearance_coordinates_and_sensors() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 207.);
    let events = events.lock().unwrap();
    let event = events.last().unwrap();
    assert_eq!((event.position.x, event.position.y), (7., 9.));
    assert_eq!(
        (event.pressure, event.altitude_angle),
        (Some(0.6), Some(0.7))
    );
    assert_eq!(
        pool.captured_placement(event.pointer_id),
        Some(&SurfaceId(11))
    );
}

#[test]
fn review_input_touch_distinct_ids_and_independent_terminals() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Down, 8.);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Move, 9.);
    let e = events.lock().unwrap();
    assert_ne!(e[0].pointer_id, e[1].pointer_id);
    assert_eq!(e[1].pointer_id, e[3].pointer_id);
    assert_eq!(
        pool.captured_placement(e[3].pointer_id),
        Some(&SurfaceId(10))
    );
}

#[test]
fn review_input_touch_hide_reopen_retires_old_contact() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    pool.set_placements([]);
    pool.set_placements([(SurfaceId(10), placed(key(7), 0, 0., false))]);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Move, 8.);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 8.);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        [PointerPhase::Down, PointerPhase::Cancel]
    );
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    let e = events.lock().unwrap();
    assert_ne!(e[0].pointer_id, e.last().unwrap().pointer_id);
}

#[test]
fn review_input_touch_provider_replacement_does_not_receive_old_up() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    let replacement = Calls::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    pool.insert(key(7), hosted(&replacement));
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Move, 8.);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 8.);
    assert!(
        !replacement
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("pointer"))
    );
    assert_eq!(
        events.lock().unwrap().last().unwrap().phase,
        PointerPhase::Cancel
    );
}

#[test]
fn review_input_touch_focus_loss_cancels_every_contact() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Down, 8.);
    input.focus_lost(&mut pool, 0);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Up, 8.);
    let e = events.lock().unwrap();
    assert_eq!(
        e.iter().filter(|e| e.phase == PointerPhase::Cancel).count(),
        2
    );
    assert!(!e.iter().any(|e| e.phase == PointerPhase::Up));
}

#[test]
fn review_input_source_window_text() {
    window_keyboard_case(keyboard(0, Some("text")));
}

#[test]
fn review_input_source_window_enter() {
    window_keyboard_case(keyboard(13, None));
}

#[test]
fn review_input_source_window_ime() {
    window_keyboard_case(PeltInput::Session(inker::SessionInput::Ime(
        inker::SessionIme::Commit("composed".into()),
    )));
}

#[test]
fn review_input_lens_auxiliary_and_mouse_capture_id() {
    for button in [inker::MouseButton::Middle, inker::MouseButton::Right] {
        assert!(
            lens_button(button),
            "lens content accepts auxiliary buttons"
        );
    }
}

#[test]
fn review_input_mouse_capture_uses_actual_pointer_id() {
    let (mut pool, _, _) = touch_pool();
    let event = super::super::super::input::mouse_pointer_event(
        7.,
        9.,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
        PointerButtons::AUXILIARY,
        Default::default(),
    );
    pool.input_at(0, PeltInput::Pointer(event));
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
}

#[test]
fn review_input_window_button_masks_and_unrelated_release() {
    let middle = Some(inker::MouseButton::Middle);
    for window in [0, 1] {
        let down = mouse_buttons(window, PointerButtons::NONE, PointerPhase::Down, middle);
        assert_eq!(down, PointerButtons::AUXILIARY);
        let unrelated = mouse_buttons(
            window,
            down,
            PointerPhase::Up,
            Some(inker::MouseButton::Left),
        );
        assert_eq!(unrelated, down);
        assert_eq!(
            mouse_buttons(window, unrelated, PointerPhase::Up, middle),
            PointerButtons::NONE
        );
    }
}

#[test]
fn review_input_lens_control_wheel_matches_primary_zoom_policy() {
    assert!(wheel_zoom(0, true, 1., true));
    assert!(wheel_zoom(1, true, 1., true));
    assert!(!wheel_zoom(1, true, 1., false));
}

#[test]
fn review_input_plain_wheel_does_not_zoom() {
    for window in [0, 1] {
        assert!(!wheel_zoom(window, false, 1., true));
        assert!(!wheel_zoom(window, true, 0., true));
    }
}

#[test]
fn review_input_click_slop_parser_is_configurable_and_finite() {
    assert_eq!(click_slop(None), 4.);
    assert_eq!(click_slop(Some("8.5")), 8.5);
    assert_eq!(click_slop(Some("0")), 0.);
    for value in ["NaN", "inf", "-1", "bad"] {
        assert_eq!(click_slop(Some(value)), 4.);
    }
}

#[test]
fn review_mouse_boundary_primary_hide_then_physical_release() {
    cancelled_native_mouse_case(0, false);
}

#[test]
fn review_mouse_boundary_lens_hide_then_physical_release() {
    cancelled_native_mouse_case(1, false);
}

#[test]
fn review_mouse_boundary_primary_replacement_then_physical_release() {
    cancelled_native_mouse_case(0, true);
}

#[test]
fn review_mouse_boundary_lens_replacement_then_physical_release() {
    cancelled_native_mouse_case(1, true);
}

#[test]
fn review_mouse_boundary_unrelated_release_keeps_middle_custody() {
    for window in [0, 1] {
        let (mut pool, events, _) = touch_pool();
        pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
        let mut buttons = HashMap::new();
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            Some(SurfaceId(10)),
            PointerPhase::Down,
            Some(inker::MouseButton::Middle),
        );
        let appearance = captured_mouse(&pool);
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            appearance,
            PointerPhase::Up,
            Some(inker::MouseButton::Left),
        );
        assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
        assert_eq!(buttons[&window], PointerButtons::AUXILIARY);
        let appearance = captured_mouse(&pool);
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            appearance,
            PointerPhase::Move,
            None,
        );
        let appearance = captured_mouse(&pool);
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            appearance,
            PointerPhase::Up,
            Some(inker::MouseButton::Middle),
        );
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .map(|e| (e.phase, e.buttons))
                .collect::<Vec<_>>(),
            [
                (PointerPhase::Down, PointerButtons::AUXILIARY),
                (PointerPhase::Move, PointerButtons::AUXILIARY),
                (PointerPhase::Up, PointerButtons::NONE),
            ]
        );
    }
}

#[test]
fn review_bookkeeping_primary_second_button_is_delivered_once() {
    bookkeeping_with_existing_mouse_capture(0);
}

#[test]
fn review_bookkeeping_lens_second_button_is_delivered_once() {
    bookkeeping_with_existing_mouse_capture(1);
}

#[test]
fn review_overlap_native_mouse_matches_host_topmost_and_preserves_capture() {
    let (mut pool, events, plan) = overlap_surface_pool();
    let hit = crate::surface::hit_test(
        &plan,
        FocusTarget::Content {
            node: uuid::Uuid::from_u128(8),
            appearance: SurfaceId(11),
        },
        27.,
        9.,
    )
    .unwrap();
    assert_eq!(hit.id, SurfaceId(11));
    let mut buttons = HashMap::new();
    let (source, effect) = native_mouse_input(
        &mut pool,
        &mut buttons,
        0,
        Some(hit.id),
        PhysicalPosition { x: 27., y: 9. },
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(8));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(11)));
    let (source, unrelated) = native_mouse_input(
        &mut pool,
        &mut buttons,
        0,
        Some(SurfaceId(11)),
        PhysicalPosition { x: 207., y: 209. },
        PointerPhase::Up,
        Some(inker::MouseButton::Right),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(8));
    assert!(!unrelated.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(11)));
    for (phase, button) in [
        (PointerPhase::Move, None),
        (PointerPhase::Up, Some(inker::MouseButton::Left)),
    ] {
        let appearance = captured_mouse(&pool);
        let (source, effect) = native_mouse_input(
            &mut pool,
            &mut buttons,
            0,
            appearance,
            PhysicalPosition { x: 207., y: 209. },
            phase,
            button,
            Default::default(),
        )
        .unwrap();
        assert_eq!(source, key(8));
        assert!(effect.handled);
    }
    assert_eq!(captured_mouse(&pool), None);
    assert!(
        events[0].lock().unwrap().is_empty(),
        "covered producer must receive no gesture"
    );
    let upper = events[1].lock().unwrap();
    assert_eq!(
        upper
            .iter()
            .map(|e| (e.phase, e.position.x, e.position.y))
            .collect::<Vec<_>>(),
        vec![
            (PointerPhase::Down, 7., 9.),
            (PointerPhase::Move, 187., 209.),
            (PointerPhase::Up, 187., 209.)
        ]
    );
}

#[test]
fn review_overlap_native_touch_matches_host_topmost_and_preserves_capture() {
    let (mut pool, events, plan) = overlap_surface_pool();
    let mut adapter = TouchInput::default();
    for (phase, x) in [
        (PointerPhase::Down, 27.),
        (PointerPhase::Move, 207.),
        (PointerPhase::Up, 207.),
    ] {
        assert!(adapter.deliver(
            &mut pool,
            &plan,
            FocusTarget::Graph(crate::panes::PaneId(99)),
            0,
            80,
            phase,
            PhysicalPosition { x, y: 9. },
            Some(0.6),
            Some(0.7),
            Default::default()
        ));
        if phase == PointerPhase::Down {
            let id = events[1].lock().unwrap().last().map(|e| e.pointer_id);
            assert!(
                id.is_some(),
                "host-selected upper producer receives touch Down"
            );
            assert_eq!(pool.captured_placement(id.unwrap()), Some(&SurfaceId(11)));
        }
    }
    assert!(events[0].lock().unwrap().is_empty());
    let upper = events[1].lock().unwrap();
    assert_eq!(
        upper
            .iter()
            .map(|e| (e.phase, e.position.x, e.position.y))
            .collect::<Vec<_>>(),
        vec![
            (PointerPhase::Down, 7., 9.),
            (PointerPhase::Move, 187., 9.),
            (PointerPhase::Up, 187., 9.)
        ]
    );
    assert!(upper.iter().all(|e| e.pointer_type == PointerType::Touch));
    assert_eq!(pool.captured_placement(upper[0].pointer_id), None);
}

#[test]
fn review_overlap_native_navigation_effect_names_actual_receiver() {
    overlap_native_document_effect(false);
}

#[test]
fn review_overlap_native_submission_effect_names_actual_receiver() {
    overlap_native_document_effect(true);
}
