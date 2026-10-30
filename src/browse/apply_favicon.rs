// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A page's favicon arrived: decode it to RGBA for this frame, encode that
/// canonical pixel result as PNG for durability, and stamp its reference on
/// the requesting node if it still lives at the page the icon belongs to.
pub fn apply_favicon(
    canvas: &mut Canvas,
    node: Uuid,
    owner_url: &str,
    bytes: &[u8],
) -> Vec<Effect> {
    if !still_current(canvas, node, owner_url) {
        tracing::info!(%node, url = %owner_url, "favicon for a superseded node; dropped");
        return Vec::new();
    }
    let Some(decoded) = decode_image_bytes(bytes) else {
        return Vec::new();
    };
    let Some(png) = crate::session::encode_rgba_png(&decoded.rgba, decoded.width, decoded.height)
    else {
        return Vec::new();
    };
    // The graph carries a content-addressed reference; the pixels go to the
    // canvas cache (so the icon paints this frame) and canonical PNG bytes to
    // the session's blob directory (so every image role has one durable
    // format). The digest is over exactly what is stored.
    let digest = *eidetic::Hash::of(&png).as_bytes();
    let image = mere::kernel::types::ImageRef::new(digest, decoded.width, decoded.height);
    let changed = canvas.set_node_favicon_for(node, image);
    canvas.register_resolved_image(digest, decoded.rgba, decoded.width, decoded.height);
    let mut effects = vec![Effect::StoreImage {
        hex: image.hex(),
        bytes: png,
    }];
    if changed {
        tracing::info!(url = %owner_url, "node favicon enriched from the page");
        effects.push(Effect::SaveSession);
    }
    effects.push(Effect::Redraw);
    effects
}
