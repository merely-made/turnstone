// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for SkyLocalTimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UtcCalendarOutOfRange(epoch) => {
                write!(
                    formatter,
                    "TT JD {} is outside the civil-time range",
                    epoch.day()
                )
            },
            Self::LeapSecond(epoch) => write!(
                formatter,
                "TT JD {} maps to a UTC leap second which Jiff cannot label",
                epoch.day()
            ),
        }
    }
}
