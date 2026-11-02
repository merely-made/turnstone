// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn assert_interception_retired(
    f: &mut PrimaryPaneCaptureFixture,
    lifetime: &Calls,
    button: winit::event::MouseButton,
) {
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    assert_eq!(
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Cancel]
    );
    assert!(
        !lifetime.lock().unwrap().iter().any(|e| e == "focus false"),
        "interception must not invent content or OS focus loss"
    );
    let physical = f.content.buttons[&0];
    assert!(physical.0 & PointerButtons::PRIMARY.0 != 0);
    assert!(
        physical.0
            & match button {
                winit::event::MouseButton::Middle => PointerButtons::AUXILIARY.0,
                winit::event::MouseButton::Right => PointerButtons::SECONDARY.0,
                _ => panic!("expected an intercepted chord button"),
            }
            != 0
    );
    // Immediate releases occur before a redraw/placement sync and cannot reach
    // the retired producer. Physical masks still follow the real OS envelope.
    f.release(f.content.position(0), winit::event::MouseButton::Left);
    f.release(f.content.position(0), button);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(f.content.pointers[0].lock().unwrap().len(), 2);
    assert!(f.content.pointers[1].lock().unwrap().is_empty());
}
