// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn default_policy_path(data_root: &Path) -> PathBuf {
    data_root
        .join("profiles")
        .join("default")
        .join("web-policy.json")
}
