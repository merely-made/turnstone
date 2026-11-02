// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyFactSpan {
    pub fn at(epoch: JulianDate<TerrestrialTime>) -> Self {
        Self {
            start: epoch,
            end: epoch,
        }
    }

    pub fn start(self) -> JulianDate<TerrestrialTime> {
        self.start
    }

    pub fn end(self) -> JulianDate<TerrestrialTime> {
        self.end
    }

    pub fn midpoint(self) -> JulianDate<TerrestrialTime> {
        JulianDate::from_julian_day((self.start.day() + self.end.day()) / 2.0)
            .expect("a span between finite epochs has a finite midpoint")
    }
}
