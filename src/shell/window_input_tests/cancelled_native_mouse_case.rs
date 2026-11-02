// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn cancelled_native_mouse_case(window: u64, replace: bool) {
    let (mut pool, old_events, _) = touch_pool();
    pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
    let mut buttons = HashMap::new();
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        None,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
    );
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
    );
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    assert_eq!(
        old_events.lock().unwrap()[0].buttons,
        PointerButtons::AUXILIARY
    );
    let new_events = if replace {
        let replacement = Arc::new(Mutex::new(Vec::new()));
        pool.insert(
            key(7),
            PeltContent::from_surface(
                Box::new(Page {
                    calls: Default::default(),
                    pointers: replacement.clone(),
                    acquisitions: Default::default(),
                    title: None,
                    fail_offset: None,
                }),
                (100, 100),
                route(),
            ),
        );
        replacement
    } else {
        pool.set_placements([]);
        pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
        old_events.clone()
    };
    assert_eq!(captured_mouse(&pool), None);
    assert_eq!(
        buttons[&window],
        PointerButtons::AUXILIARY,
        "pool Cancel is not a physical Middle release"
    );
    assert_eq!(
        old_events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        [PointerPhase::Down, PointerPhase::Cancel]
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
        old_events.lock().unwrap().len(),
        2,
        "retired gesture cannot receive late Up or activate"
    );
    if replace {
        assert!(
            new_events.lock().unwrap().is_empty(),
            "replacement cannot receive old terminal event"
        );
    }
    let start = new_events.lock().unwrap().len();
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        None,
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
    );
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
    );
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
        Some(inker::MouseButton::Left),
    );
    let events = new_events.lock().unwrap();
    assert_eq!(
        events[start..]
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        [
            (
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                PointerButtons::PRIMARY
            ),
            (PointerPhase::Move, None, PointerButtons::PRIMARY),
            (
                PointerPhase::Up,
                Some(inker::MouseButton::Left),
                PointerButtons::NONE
            ),
        ],
        "native Middle Up without capture must clear the physical mask before a new Left gesture"
    );
    assert_eq!(buttons[&window], PointerButtons::NONE);
}
