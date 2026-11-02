// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A physical surface size in layout pixels at `scale`.
pub(crate) fn logical_size((w, h): (u32, u32), scale: f32) -> (u32, u32) {
    (
        (w as f32 / scale).round() as u32,
        (h as f32 / scale).round() as u32,
    )
}
