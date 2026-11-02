// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Deliver the native terminal event before returning non-content custody to
/// Shell's renderer-specific release handling.
pub(in super::super) fn dispatch_primary_release(
    host: &mut impl PrimaryPointerHost,
    x: f32,
    y: f32,
    button: MouseButton,
) -> std::ops::ControlFlow<(), Option<crate::surface::SurfaceKind>> {
    let appearance = if surface_mouse_button(button).is_some()
        && !host.pointer_has_contributed_capture()
        && matches!(
            *host.pointer_state().2,
            Some(crate::surface::SurfaceKind::Content(_))
        ) {
        host.sync_pointer_placements();
        super::super::window_input::captured_mouse(host.pointer_state().1)
    } else {
        None
    };
    host.pointer_window_mouse(
        appearance,
        (x, y),
        inker::PointerPhase::Up,
        surface_mouse_button(button),
    );
    if host.pointer_has_contributed_capture() {
        if button == MouseButton::Left {
            host.pointer_release_contributed(x, y);
        }
        return std::ops::ControlFlow::Break(());
    }
    let captured = host.pointer_state().2.take();
    if let Some(crate::surface::SurfaceKind::Content(_)) = captured {
        if super::super::window_input::captured_mouse(host.pointer_state().1).is_some() {
            *host.pointer_state().2 = captured;
        }
        host.pointer_redraw();
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(captured)
}
