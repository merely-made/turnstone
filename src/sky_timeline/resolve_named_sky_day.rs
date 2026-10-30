// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(test)]
pub(crate) fn resolve_named_sky_day(
    date: Date,
    time_zone_name: &str,
) -> Result<ResolvedSkyDay, SkyRequestError> {
    let time_zone = TimeZone::get(time_zone_name)
        .map_err(|_| SkyRequestError::UnknownTimeZone(time_zone_name.to_owned()))?;
    resolve_sky_day(date, time_zone, time_zone_name.to_owned())
}
