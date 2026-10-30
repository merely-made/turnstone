// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for SkyRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTimeZone(name) => write!(formatter, "unknown IANA time zone '{name}'"),
            Self::EmptyBodySet => formatter.write_str("a Sky day requires at least one body"),
            Self::DuplicateBody(body) => {
                write!(formatter, "Sky body '{}' was selected twice", body.name())
            },
            Self::CivilDateOverflow => {
                formatter.write_str("the following civil date is outside range")
            },
            Self::CivilDayBoundaryOutOfRange { date, time_zone } => write!(
                formatter,
                "civil date {date} cannot be resolved in {time_zone}"
            ),
            Self::AmbiguousLocalTime { local, time_zone } => write!(
                formatter,
                "local time {local} occurs twice in {time_zone}; select an unambiguous anchor"
            ),
            Self::NonexistentLocalTime { local, time_zone } => write!(
                formatter,
                "local time {local} does not exist in {time_zone}"
            ),
            Self::NonIncreasingCivilDay => {
                formatter.write_str("resolved civil-day end must follow its start")
            },
            Self::AnchorOutsideDay => {
                formatter.write_str("resolved Sky anchor does not belong to the selected day")
            },
        }
    }
}
