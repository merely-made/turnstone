// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Reuse one full-window frame texture for each measured float crop. This
/// preserves the DOM's paint/clip coordinates without a second layout owner.
pub(super) fn frame_placement(
    surface: &crate::surface::Surface,
    w: u32,
    h: u32,
) -> ExternalTexturePlacement {
    let r = surface.rect;
    let placement = ExternalTexturePlacement::new(r.dest());
    if matches!(surface.kind, SurfaceKind::FloatFrame(_)) {
        placement.with_uv([
            r.x / w.max(1) as f32,
            r.y / h.max(1) as f32,
            (r.x + r.w) / w.max(1) as f32,
            (r.y + r.h) / h.max(1) as f32,
        ])
    } else {
        placement
    }
}
