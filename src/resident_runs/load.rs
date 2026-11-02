// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn load(dir: &Path) -> Result<ResidentRuns, String> {
    load_with_limits(dir, StorageLimits::default())
}
