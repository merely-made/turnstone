// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// How many suggestion rows to offer: the configured maximum, clamped to what
/// fits between the card and the bottom of the window.
///
/// The configured value stays the ceiling — a deliberate setting is never
/// raised by geometry — while a short window lowers it so the list does not
/// run off the bottom. Sizes are measured from the same card and sheet
/// as the retained renderer, including its live `ui_zoom`.
pub fn visible_row_limit(
    configured: usize,
    placement: &crate::panes::ChromePlacement,
    viewport_h: f32,
    ui_zoom: f32,
) -> usize {
    visible_row_limit_with_extra_height(configured, placement, viewport_h, ui_zoom, 0.0)
}
