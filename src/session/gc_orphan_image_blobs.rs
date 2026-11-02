// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Mark/sweep the session's file-sidecar image store against every live graph
/// reference. Only 64-character hex filenames are store members; unrelated
/// files in the directory are left alone. Returns blobs actually removed.
pub fn gc_orphan_image_blobs(data_root: &Path, graph: &Graph) -> usize {
    let referenced: std::collections::HashSet<String> = graph
        .nodes()
        .flat_map(|(_, node)| node.images.values())
        .map(|image| image.hex())
        .collect();
    let Ok(entries) = std::fs::read_dir(images_dir(data_root)) else {
        return 0;
    };
    let mut dropped = 0;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let is_digest = name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit());
        if is_digest && !referenced.contains(&name) && std::fs::remove_file(entry.path()).is_ok() {
            dropped += 1;
        }
    }
    dropped
}
