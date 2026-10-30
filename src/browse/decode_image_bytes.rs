// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Decode host-fetched image bytes at the product edge. Livery consumes the
/// same encoded resources for document paint; graph favicons need canonical
/// RGBA pixels for Turnstone's content-addressed cache.
pub(crate) fn decode_image_bytes(bytes: &[u8]) -> Option<DecodedImage> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(DecodedImage {
        width,
        height,
        rgba: image.into_raw(),
    })
}
