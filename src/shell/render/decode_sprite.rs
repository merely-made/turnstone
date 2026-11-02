// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Decode a dropped image file into a face-sized PNG data-URI plus its traced
/// collider hull, or `None` for a file the image decoder does not read (which
/// then becomes a node instead). Downscaled so the per-node URI stays small
/// (the face draws at ~24-120px). The hull is canvas's shared tracer (the
/// meerkat-harvest promotion), so the node collides at its picture.
pub(in super::super) fn decode_sprite(path: &Path) -> Option<(String, Vec<(f32, f32)>)> {
    const SPRITE_MAX: u32 = 256;
    let rgba = image::open(path)
        .ok()?
        .thumbnail(SPRITE_MAX, SPRITE_MAX)
        .to_rgba8();
    let (w, h) = rgba.dimensions();
    let hull = mere::canvas::sprite_hull::trace_sprite_hull(rgba.as_raw(), w, h);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    use base64::Engine as _;
    Some((
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&png)
        ),
        hull,
    ))
}
