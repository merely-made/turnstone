// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TtJulianDateV1 {
    pub(super) fn from_tt(value: JulianDate<TerrestrialTime>) -> Self {
        let (day1, day2) = value.parts();
        Self { day1, day2 }
    }

    pub(super) fn summed_day(&self) -> f64 {
        self.day1 + self.day2
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        if self.day1.is_finite() && self.day2.is_finite() && self.summed_day().is_finite() {
            Ok(())
        } else {
            Err(SkyReceiptJsonError::Invalid(format!(
                "{field} must contain finite Julian-date parts"
            )))
        }
    }
}
