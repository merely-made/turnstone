// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn tt_from_utc(utc: Timestamp) -> JulianDate<TerrestrialTime> {
    let utc = TimeZone::UTC.to_datetime(utc);
    JulianDate::from_epoch(ScaleAwareEpoch::from_gregorian_utc(
        i32::from(utc.year()),
        utc.month() as u8,
        utc.day() as u8,
        utc.hour() as u8,
        utc.minute() as u8,
        utc.second() as u8,
        utc.subsec_nanosecond() as u32,
    ))
}
