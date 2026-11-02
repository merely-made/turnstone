// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Dispatch a hit admitted by the primary caller's chrome/modal/context gates.
/// A returned hit continues into Shell's pane/graph/divider renderer handling.
pub(in super::super) fn dispatch_primary_press(
    host: &mut impl PrimaryPointerHost,
    hit: Option<crate::surface::HitResult>,
    x: f32,
    y: f32,
    button: MouseButton,
) -> Option<crate::surface::HitResult> {
    // Pool custody wins over a fresh hit, including a position outside the
    // plan, only after current placement/window admission is revalidated.
    host.sync_pointer_placements();
    let hit = {
        let (focus, pool, _) = host.pointer_state();
        let position = inker::PhysicalPosition { x, y };
        super::super::window_input::captured_content_hit(pool, 0, position)
            .or(hit)
            .map(|hit| {
                super::super::window_input::content_press_target(focus, pool, 0, position, hit)
            })
    };
    *host.pointer_state().2 = hit.map(|h| h.kind);
    let hit = hit?;
    if !matches!(hit.kind, crate::surface::SurfaceKind::Content(_)) {
        return Some(hit);
    }
    host.pointer_content_focus(hit.id);
    if let Some(button) = surface_mouse_button(button) {
        host.pointer_content_mouse(hit.id, hit.local, inker::PointerPhase::Down, Some(button));
    }
    host.pointer_redraw();
    None
}
