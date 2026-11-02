// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn state_name(state: RunStateV1) -> &'static str {
    match state {
        RunStateV1::Queued => "Queued",
        RunStateV1::Running => "Generating",
        RunStateV1::Paused => "Paused",
        RunStateV1::Finished => "Finished",
        RunStateV1::Cancelled => "Cancelled",
        RunStateV1::Failed => "Failed",
    }
}
