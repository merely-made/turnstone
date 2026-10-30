// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn certs_path(session_dir: &Path, subject_hex: &str) -> PathBuf {
    session_dir
        .join("denizens")
        .join(format!("{subject_hex}.certs.json"))
}
