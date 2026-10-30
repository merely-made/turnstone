// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            max_runs: 1024,
            max_events_per_run: 256,
            max_bytes: 512 * 1024,
        }
    }
}
