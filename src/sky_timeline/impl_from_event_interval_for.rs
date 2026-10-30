// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<EventInterval> for SkyFactSpan {
    fn from(interval: EventInterval) -> Self {
        Self {
            start: interval.start(),
            end: interval.end(),
        }
    }
}
