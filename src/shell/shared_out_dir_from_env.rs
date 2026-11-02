// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Where a shared run writes its captures and sentinel: `TURNSTONE_CAPTURE_DIR`, or
/// the scenario file's own directory.
pub(super) fn shared_out_dir_from_env() -> std::path::PathBuf {
    let dir = std::env::var_os("TURNSTONE_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("TURNSTONE_SCENARIO")
                .map(std::path::PathBuf::from)
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
