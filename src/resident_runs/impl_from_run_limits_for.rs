// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<RunLimits> for DiskLimits {
    fn from(v: RunLimits) -> Self {
        Self {
            decisions: v.decisions,
            tool_calls: v.tool_calls,
            tokens: v.tokens,
            elapsed_ms: v.elapsed_ms,
            consecutive_failures: v.consecutive_failures,
        }
    }
}
