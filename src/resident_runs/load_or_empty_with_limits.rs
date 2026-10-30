// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn load_or_empty_with_limits(
    dir: &Path,
    limits: StorageLimits,
) -> Result<ResidentRuns, String> {
    match path(dir).try_exists() {
        Ok(false) => Ok(ResidentRuns::with_limits(limits)),
        Ok(true) => load_with_limits(dir, limits),
        Err(error) => Err(format!("cannot inspect resident run state: {error}")),
    }
}
