// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn caller_capture_effect(window: u64, submit: bool) {
    let mut f = CallerCaptureFixture::new(window, Some(submit));
    f.press(0, inker::MouseButton::Left);
    let (source, effect) = f.press(1, inker::MouseButton::Middle);
    assert!(effect.handled);
    assert_eq!(
        source,
        ContentKey::Node(f.nodes[0]),
        "caller selects actual captured effect source A"
    );
    f.assert_a_custody();
    let (source, unrelated) = f.pointer(PointerPhase::Up, Some(inker::MouseButton::Right));
    assert_eq!(source, ContentKey::Node(f.nodes[0]));
    assert!(!unrelated.handled);
    assert!(unrelated.navigation.is_none() && unrelated.submission.is_none());
    f.assert_a_custody();
    for button in [inker::MouseButton::Left, inker::MouseButton::Middle] {
        let (source, effect) = f.pointer(PointerPhase::Up, Some(button));
        assert_eq!(source, ContentKey::Node(f.nodes[0]));
        if submit {
            assert_eq!(effect.submission.unwrap().action, "https://upper.test/post");
            assert!(effect.navigation.is_none());
        } else {
            assert_eq!(
                effect.navigation.unwrap().request.address,
                "https://upper.test/next"
            );
            assert!(effect.submission.is_none());
        }
        if button == inker::MouseButton::Left {
            f.assert_a_custody();
        }
    }
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert!(
        !f.inputs[0]
            .lock()
            .unwrap()
            .iter()
            .any(|input| matches!(input, inker::SessionInput::Cancel))
    );
    assert!(!f.inputs[1].lock().unwrap().iter().any(|input| matches!(
        input,
        inker::SessionInput::PointerButton { .. } | inker::SessionInput::PointerMoved { .. }
    )));
}
