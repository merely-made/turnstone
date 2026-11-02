// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The session's image-blob directory: `<session>/images/<hex>`.
///
/// After the node-image externalization the graph carries ~40-byte references
/// and the pixels live out of line. Turnstone's session persistence is
/// file-sidecar shaped (`graph.json`, `facets.json`), so its blob store is a
/// directory of the same shape rather than the eidetic-backed
/// `pandect::image_store` — same content-addressed `<hex>` key, so the
/// two converge cleanly if this session ever gains a real store.
pub(super) fn images_dir(data_root: &Path) -> std::path::PathBuf {
    data_root.join("images")
}
