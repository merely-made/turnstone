// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The per-user data root (`<data_dir>/turnstone`). A `TURNSTONE_ROOT` override
/// points the whole root at a scratch profile, so a headed-verification run
/// (or any throwaway session) isolates from the real per-user data dir (the
/// meerkat `MERE_ROOT` convention).
pub fn default_turnstone_root() -> PathBuf {
    if let Some(root) = std::env::var_os("TURNSTONE_ROOT") {
        return PathBuf::from(root);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("turnstone")
}
