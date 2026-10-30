// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn path(dir: &Path) -> PathBuf {
    dir.join("denizens").join("resident-runs.json")
}
