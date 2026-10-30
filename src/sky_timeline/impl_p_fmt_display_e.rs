// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<P: fmt::Display, E: fmt::Display> fmt::Display for SkyTimelineError<P, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(source) => source.fmt(formatter),
            Self::SearchWindow(source) => source.fmt(formatter),
            Self::RiseSetSearch(source) => source.fmt(formatter),
            Self::TransitSearch(source) => source.fmt(formatter),
            Self::Position {
                body,
                epoch,
                source,
            } => write!(
                formatter,
                "could not position {} at TT JD {}: {source}",
                body.name(),
                epoch.day()
            ),
            Self::EarthOrientation { epoch, source } => write!(
                formatter,
                "could not resolve Earth orientation at TT JD {}: {source}",
                epoch.day()
            ),
            Self::EarthOrientationIdentityMismatch {
                expected_authority,
                expected_snapshot,
                actual_authority,
                actual_snapshot,
            } => write!(
                formatter,
                "Earth-orientation result identity {actual_authority}/{actual_snapshot} does not match provider {expected_authority}/{expected_snapshot}"
            ),
            Self::Transform {
                body,
                epoch,
                source,
            } => write!(
                formatter,
                "could not transform {} at TT JD {}: {source}",
                body.name(),
                epoch.day()
            ),
            Self::RiseSet { body, source } => {
                write!(
                    formatter,
                    "could not find {} rise/set: {source}",
                    body.name()
                )
            },
            Self::Transit { body, source } => {
                write!(
                    formatter,
                    "could not find {} transits: {source}",
                    body.name()
                )
            },
            Self::LunarPhase(source) => write!(formatter, "could not find lunar phases: {source}"),
            Self::FactProvenanceMismatch(message) => {
                write!(formatter, "Sky fact provenance mismatch: {message}")
            },
        }
    }
}
