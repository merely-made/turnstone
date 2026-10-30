// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Lens captures use the same owned-copy helper with its five-second bound.
pub(in super::super) fn read_texture_rgba(
    host: &SurfaceHost,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    host.shared_core()
        .read_rgba8_texture(texture, width, height)
        .map(|frame| frame.rgba)
        .map_err(|error| tracing::warn!(%error, "capture readback failed"))
        .unwrap_or_default()
}
