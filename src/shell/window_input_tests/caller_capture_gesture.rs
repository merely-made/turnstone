// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn caller_capture_gesture(window: u64) {
    let mut f = CallerCaptureFixture::new(window, None);
    assert!(f.press(0, inker::MouseButton::Left).1.handled);
    f.assert_a_custody();
    let plan = f.plan();
    // Actual movement over B stays captured by A and leaves host focus alone.
    let position = f.position(1);
    assert!(
        native_mouse_input(
            &mut f.contents.pool,
            &mut f.buttons,
            window,
            Some(f.surfaces[0].id),
            position,
            PointerPhase::Move,
            None,
            Default::default()
        )
        .unwrap()
        .1
        .handled
    );
    assert!(f.press(1, inker::MouseButton::Middle).1.handled);
    f.assert_a_custody();
    assert_eq!(
        f.plan(),
        plan,
        "focus mutation must not be mistaken for changed appearance geometry"
    );
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    assert_eq!(f.buttons[&window], both);
    let second = f.pointers[0].lock().unwrap().clone();
    assert_eq!(second.len(), 3);
    assert!(second.iter().all(|e| e.phase != PointerPhase::Cancel));
    assert_eq!(
        (second[2].phase, second[2].button, second[2].buttons),
        (PointerPhase::Down, Some(inker::MouseButton::Middle), both)
    );
    assert_eq!(
        (second[2].position.x, second[2].position.y),
        (
            position.x - f.surfaces[0].rect.x,
            position.y - f.surfaces[0].rect.y
        )
    );
    assert!(
        f.pointers[1].lock().unwrap().is_empty(),
        "fresh hit B receives no captured gesture"
    );
    assert!(
        !f.pointer(PointerPhase::Up, Some(inker::MouseButton::Right))
            .1
            .handled
    );
    f.assert_a_custody();
    assert!(
        f.pointer(PointerPhase::Up, Some(inker::MouseButton::Left))
            .1
            .handled
    );
    assert_eq!(f.buttons[&window], PointerButtons::AUXILIARY);
    f.assert_a_custody();
    assert!(
        f.pointer(PointerPhase::Up, Some(inker::MouseButton::Middle))
            .1
            .handled
    );
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert_eq!(f.buttons[&window], PointerButtons::NONE);
    assert_eq!(f.pointers[0].lock().unwrap().len(), 5);
    assert!(
        !f.pointer(PointerPhase::Up, Some(inker::MouseButton::Middle))
            .1
            .handled
    );
    assert_eq!(f.pointers[0].lock().unwrap().len(), 5);
    assert!(f.pointers[1].lock().unwrap().is_empty());
}
