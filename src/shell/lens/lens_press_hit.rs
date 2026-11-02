// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The actual Lens press resolver before Pane/Content/Divider dispatch.
/// Current same-window custody precedes every fresh hit-kind branch.
pub(in super::super) fn lens_press_hit(
    pool: &super::super::window_input::Pool,
    window: u64,
    plan: &[crate::surface::Surface],
    focus: crate::surface::FocusTarget,
    position: inker::PhysicalPosition,
) -> Option<crate::surface::HitResult> {
    super::super::window_input::captured_content_hit(pool, window, position)
        .or_else(|| crate::surface::hit_test(plan, focus, position.x, position.y))
}
