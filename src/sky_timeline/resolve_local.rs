// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolve_local(
    time_zone: &TimeZone,
    local: CivilDateTime,
    time_zone_name: &str,
) -> Result<Timestamp, SkyRequestError> {
    let ambiguous = time_zone.to_ambiguous_timestamp(local);
    match ambiguous.offset() {
        AmbiguousOffset::Unambiguous { .. } => {
            ambiguous
                .unambiguous()
                .map_err(|_| SkyRequestError::NonexistentLocalTime {
                    local,
                    time_zone: time_zone_name.to_owned(),
                })
        },
        AmbiguousOffset::Fold { .. } => Err(SkyRequestError::AmbiguousLocalTime {
            local,
            time_zone: time_zone_name.to_owned(),
        }),
        AmbiguousOffset::Gap { .. } => Err(SkyRequestError::NonexistentLocalTime {
            local,
            time_zone: time_zone_name.to_owned(),
        }),
    }
}
