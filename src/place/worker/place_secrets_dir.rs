// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn place_secrets_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("place-secrets")
}
