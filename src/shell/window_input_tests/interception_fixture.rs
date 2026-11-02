// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn interception_fixture() -> (PrimaryPaneCaptureFixture, Calls) {
    let mut f = PrimaryPaneCaptureFixture::new();
    let lifetime = interception_page(&mut f.content, 0);
    f.press(f.content.position(0), winit::event::MouseButton::Left);
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        Some(f.content.surfaces[0].id)
    );
    lifetime.lock().unwrap().clear();
    (f, lifetime)
}
