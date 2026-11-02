// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn utc_from_tt(
    epoch: JulianDate<TerrestrialTime>,
) -> Result<Timestamp, SkyLocalTimeError> {
    let (year, month, day, hour, minute, second, nanosecond) = epoch.to_epoch().to_gregorian_utc();
    if second == 60 {
        return Err(SkyLocalTimeError::LeapSecond(epoch));
    }
    let year = i16::try_from(year).map_err(|_| SkyLocalTimeError::UtcCalendarOutOfRange(epoch))?;
    let utc = CivilDateTime::new(
        year,
        month as i8,
        day as i8,
        hour as i8,
        minute as i8,
        second as i8,
        nanosecond as i32,
    )
    .map_err(|_| SkyLocalTimeError::UtcCalendarOutOfRange(epoch))?;
    TimeZone::UTC
        .to_timestamp(utc)
        .map_err(|_| SkyLocalTimeError::UtcCalendarOutOfRange(epoch))
}
