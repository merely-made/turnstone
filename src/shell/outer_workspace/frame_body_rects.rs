// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Compatibility projection for callers that need only docked pane bodies.
pub(in super::super) fn frame_body_rects(
    app: &crate::app::App,
    space: SpaceRef,
    size: (u32, u32),
) -> Option<Vec<(PaneId, Rect)>> {
    frame_body_geometry(app, space, size).map(|(docked, _)| docked)
}
