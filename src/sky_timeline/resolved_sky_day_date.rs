// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ResolvedSkyDay {
    pub fn date(&self) -> Date {
        self.date
    }

    pub fn time_zone_name(&self) -> &str {
        &self.time_zone_name
    }

    pub fn start_utc(&self) -> Timestamp {
        self.start_utc
    }

    pub fn end_utc(&self) -> Timestamp {
        self.end_utc
    }

    pub fn start_tt(&self) -> JulianDate<TerrestrialTime> {
        self.start_tt
    }

    pub fn end_tt(&self) -> JulianDate<TerrestrialTime> {
        self.end_tt
    }

    pub fn elapsed_seconds(&self) -> i64 {
        self.end_utc.as_second() - self.start_utc.as_second()
    }

    pub fn contains(&self, epoch: JulianDate<TerrestrialTime>) -> bool {
        epoch.day() >= self.start_tt.day() && epoch.day() < self.end_tt.day()
    }

    pub fn local_time(
        &self,
        epoch: JulianDate<TerrestrialTime>,
    ) -> Result<Zoned, SkyLocalTimeError> {
        Ok(utc_from_tt(epoch)?.to_zoned(self.time_zone.clone()))
    }
}
