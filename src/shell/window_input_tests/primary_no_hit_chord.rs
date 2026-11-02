// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn primary_no_hit_chord(first_release: winit::event::MouseButton) {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        Some(f.content.surfaces[0].id)
    );
    let pane = PhysicalPosition {
        x: -100.0,
        y: -100.0,
    };
    assert!(
        crate::surface::hit_test(&f.content.plan(), f.content.app.focus, pane.x, pane.y).is_none(),
        "this real plan position must have no fresh hit"
    );
    f.press(pane, Middle);
    f.move_pointer(pane);
    let second_release = if first_release == Left { Middle } else { Left };
    f.release(pane, first_release);
    let after_first = captured_mouse(&f.content.contents.pool);
    f.release(pane, second_release);

    // Check the terminal outcome before intermediate routing assertions, so
    // the existing caller fault reports stranded custody rather than a count.
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        None,
        "all physical releases must retire primary pool custody after a no-hit chord"
    );
    assert_eq!(
        after_first,
        Some(f.content.surfaces[0].id),
        "the other admitted button still holds A"
    );
    assert_eq!(f.capture, None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.app.focus,
        FocusTarget::Content {
            node: f.content.nodes[0],
            appearance: f.content.surfaces[0].id
        }
    );
    assert!(
        f.content.pointers[1].lock().unwrap().is_empty(),
        "B must not receive A's gesture"
    );
    assert_eq!(
        f.delivered,
        vec![(ContentKey::Node(f.content.nodes[0]), true); 5]
    );
    let events = f.content.pointers[0].lock().unwrap().clone();
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    let (first, remaining, second) = if first_release == Left {
        (
            inker::MouseButton::Left,
            PointerButtons::AUXILIARY,
            inker::MouseButton::Middle,
        )
    } else {
        (
            inker::MouseButton::Middle,
            PointerButtons::PRIMARY,
            inker::MouseButton::Left,
        )
    };
    assert_eq!(
        events
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        vec![
            (
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                PointerButtons::PRIMARY
            ),
            (PointerPhase::Down, Some(inker::MouseButton::Middle), both),
            (PointerPhase::Move, None, both),
            (PointerPhase::Up, Some(first), remaining),
            (PointerPhase::Up, Some(second), PointerButtons::NONE),
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.pointer_id == 1 && e.pointer_type == inker::PointerType::Mouse)
    );
    for event in &events[1..] {
        assert_eq!(
            (event.position.x, event.position.y),
            (
                pane.x - f.content.surfaces[0].rect.x,
                pane.y - f.content.surfaces[0].rect.y
            )
        );
    }
    f.release(pane, second_release);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().len(),
        5,
        "duplicate terminal input must not re-enter A"
    );
}
