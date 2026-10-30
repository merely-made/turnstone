// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The visited-document liveness budget in force, default or overridden.
///
/// The right number depends on the link, not on this code: a LAN holder answers
/// in milliseconds, one behind a relay on a bad connection may not. Exposed so
/// a deployment that knows its link can say so.
pub(super) fn visit_liveness_budget() -> Duration {
    env_integer("TURNSTONE_KNOT_VISIT_LIVENESS_MS", 0u64)
        .ok()
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(VISIT_LIVENESS_BUDGET)
}
