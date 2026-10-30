// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn clamp_scroll(
    wanted: (f32, f32),
    content: (f32, f32),
    viewport: (u32, u32),
) -> (f32, f32) {
    (
        wanted
            .0
            .clamp(0.0, (content.0 - viewport.0 as f32).max(0.0)),
        wanted
            .1
            .clamp(0.0, (content.1 - viewport.1 as f32).max(0.0)),
    )
}
