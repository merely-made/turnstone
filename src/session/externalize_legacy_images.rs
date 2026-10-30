// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Move a legacy snapshot's inline image bytes into the session blob
/// directory, leaving references on the nodes. Returns how many blobs were
/// written.
///
/// The file-sidecar counterpart of `pandect::image_store::
/// migrate_legacy_images`, which needs an eidetic `Store` this host does not
/// have. Same digest and same `<hex>` key, so the two agree.
///
/// The legacy favicon is raw RGBA with no container; encode it once here so
/// every durable image blob has the same PNG format.
pub(super) fn externalize_legacy_images(
    snapshot: &mut mere::kernel::persistence::GraphSnapshot,
    data_root: &Path,
) -> std::io::Result<usize> {
    use mere::kernel::types::{ImageRef, ImageRole};

    let mut written = 0usize;
    for node in &mut snapshot.nodes {
        if let Some(png) = node.legacy_thumbnail_png.take() {
            let digest = *eidetic::Hash::of(&png).as_bytes();
            let image = ImageRef::new(
                digest,
                node.legacy_thumbnail_width,
                node.legacy_thumbnail_height,
            );
            try_save_image_blob(data_root, &image.hex(), &png)?;
            node.images.insert(ImageRole::Preview, image);
            node.legacy_thumbnail_width = 0;
            node.legacy_thumbnail_height = 0;
            written += 1;
        }
        if let Some(rgba) = node.legacy_favicon_rgba.take() {
            let Some(png) =
                encode_rgba_png(&rgba, node.legacy_favicon_width, node.legacy_favicon_height)
            else {
                node.legacy_favicon_rgba = Some(rgba);
                continue;
            };
            let digest = *eidetic::Hash::of(&png).as_bytes();
            let image = ImageRef::new(
                digest,
                node.legacy_favicon_width,
                node.legacy_favicon_height,
            );
            try_save_image_blob(data_root, &image.hex(), &png)?;
            node.images.insert(ImageRole::Favicon, image);
            node.legacy_favicon_width = 0;
            node.legacy_favicon_height = 0;
            written += 1;
        }
    }
    debug_assert_eq!(
        snapshot.legacy_image_count(),
        0,
        "every legacy image must be externalized before the snapshot materializes"
    );
    Ok(written)
}
