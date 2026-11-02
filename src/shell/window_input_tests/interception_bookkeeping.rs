// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn interception_bookkeeping(
    f: &mut PrimaryPaneCaptureFixture,
    button: winit::event::MouseButton,
) {
    use super::super::super::input::PrimaryPointerHost;
    f.pointer_window_mouse(
        None,
        (-100., -100.),
        PointerPhase::Down,
        super::super::super::input::surface_mouse_button(button),
    );
}
