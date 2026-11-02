// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Compose the frame's already-rasterized layers into an owned `COPY_SRC`
/// target, read the pixels back, and encode a PNG at `path`. Composes the same
/// layer list, each at its own placement, that the presented frame did, so the
/// receipt matches what was shown (occlusion and all).
pub(in super::super) fn capture_composed(
    host: &SurfaceHost,
    layers: &[CompositeLayer],
    w: u32,
    h: u32,
    path: &Path,
) -> bool {
    let Ok(frame) = capture_target(host, layers, w, h) else {
        return false;
    };
    let rgba = read_texture_rgba(host, &frame, w, h);
    if rgba.is_empty() {
        return false;
    }
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    image::codecs::png::PngEncoder::new(file)
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .is_ok()
}
