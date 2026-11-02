// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn bookkeeping_with_existing_mouse_capture(window: u64) {
    let (mut pool, events, _) = touch_pool();
    pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
    let mut buttons = HashMap::new();
    let position = PhysicalPosition { x: 7., y: 9. };
    assert!(
        native_mouse_input(
            &mut pool,
            &mut buttons,
            window,
            None,
            position,
            PointerPhase::Down,
            Some(inker::MouseButton::Left),
            Default::default(),
        )
        .is_none()
    );
    let (source, effect) = content_mouse_input(
        &mut pool,
        &buttons,
        window,
        Some(SurfaceId(10)),
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(7));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    events.lock().unwrap().clear();

    // Both native windows record the physical press before deciding whether
    // the host admits content delivery. Existing capture cannot admit it early.
    let bookkeeping = native_mouse_input(
        &mut pool,
        &mut buttons,
        window,
        None,
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
        Default::default(),
    );
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    assert_eq!(buttons[&window], both);
    assert!(
        bookkeeping.is_none(),
        "bookkeeping must emit no host effect"
    );
    assert!(
        events.lock().unwrap().is_empty(),
        "bookkeeping must not reach the producer"
    );
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));

    let (source, effect) = content_mouse_input(
        &mut pool,
        &buttons,
        window,
        Some(SurfaceId(10)),
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(7));
    assert!(effect.handled);
    assert!(effect.navigation.is_none());
    assert!(effect.submission.is_none());
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.phase, e.button, e.buttons, e.position.x, e.position.y))
            .collect::<Vec<_>>(),
        [(
            PointerPhase::Down,
            Some(inker::MouseButton::Middle),
            both,
            7.,
            9.
        )],
        "the admitted second button press must reach the producer exactly once"
    );

    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Up,
        Some(inker::MouseButton::Left),
    );
    assert_eq!(buttons[&window], PointerButtons::AUXILIARY);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Up,
        Some(inker::MouseButton::Middle),
    );
    assert_eq!(buttons[&window], PointerButtons::NONE);
    assert_eq!(captured_mouse(&pool), None);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        [
            (PointerPhase::Down, Some(inker::MouseButton::Middle), both),
            (
                PointerPhase::Up,
                Some(inker::MouseButton::Left),
                PointerButtons::AUXILIARY
            ),
            (
                PointerPhase::Up,
                Some(inker::MouseButton::Middle),
                PointerButtons::NONE
            ),
        ]
    );
    let (_, late) = native_mouse_input(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        position,
        PointerPhase::Up,
        Some(inker::MouseButton::Middle),
        Default::default(),
    )
    .unwrap();
    assert!(!late.handled);
    assert!(late.navigation.is_none());
    assert!(late.submission.is_none());
    assert_eq!(
        events.lock().unwrap().len(),
        3,
        "late release cannot reach retired capture"
    );
}
