// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolve_sky_day(
    date: Date,
    time_zone: TimeZone,
    time_zone_name: String,
) -> Result<ResolvedSkyDay, SkyRequestError> {
    let following_date = date
        .tomorrow()
        .map_err(|_| SkyRequestError::CivilDateOverflow)?;
    // A calendar day still exists when an offset transition skips or repeats
    // midnight. Jiff's compatible date resolution chooses the first valid
    // instant belonging to each date. Caller-selected anchors remain strict.
    let start_utc = date
        .to_zoned(time_zone.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| SkyRequestError::CivilDayBoundaryOutOfRange {
            date,
            time_zone: time_zone_name.clone(),
        })?;
    let end_utc = following_date
        .to_zoned(time_zone.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| SkyRequestError::CivilDayBoundaryOutOfRange {
            date: following_date,
            time_zone: time_zone_name.clone(),
        })?;
    if end_utc <= start_utc {
        return Err(SkyRequestError::NonIncreasingCivilDay);
    }
    Ok(ResolvedSkyDay {
        date,
        time_zone,
        time_zone_name,
        start_utc,
        end_utc,
        start_tt: tt_from_utc(start_utc),
        end_tt: tt_from_utc(end_utc),
    })
}
