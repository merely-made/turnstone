// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyFactSpanV1 {
    pub(super) fn from_span(span: SkyFactSpan) -> Self {
        Self {
            start: TtJulianDateV1::from_tt(span.start()),
            end: TtJulianDateV1::from_tt(span.end()),
        }
    }

    pub(super) fn midpoint_day(&self) -> f64 {
        (self.start.summed_day() + self.end.summed_day()) / 2.0
    }
}
