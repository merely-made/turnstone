// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyTimelineItem {
    pub fn instant(&self) -> JulianDate<TerrestrialTime> {
        match self {
            Self::Position(fact) => fact.epoch(),
            Self::LunarPhase(event) => event.interval().midpoint(),
            Self::RiseSet(event) => event.interval().midpoint(),
            Self::MeridianTransit(event) => event.interval().midpoint(),
        }
    }

    /// The raw Turquet interval for an event row.
    ///
    /// A sampled position is an instant rather than a Turquet event, so it has
    /// no event interval. The derivation receipt represents it with a
    /// Turnstone-owned zero-width [`SkyFactSpan`] instead.
    pub fn event_interval(&self) -> Option<EventInterval> {
        match self {
            Self::Position(_) => None,
            Self::LunarPhase(event) => Some(event.interval()),
            Self::RiseSet(event) => Some(event.interval()),
            Self::MeridianTransit(event) => Some(event.interval()),
        }
    }

    pub fn span(&self) -> SkyFactSpan {
        self.event_interval()
            .map(SkyFactSpan::from)
            .unwrap_or_else(|| SkyFactSpan::at(self.instant()))
    }

    pub fn body(&self) -> ApparentBody {
        match self {
            Self::Position(fact) => fact.body(),
            Self::LunarPhase(_) => ApparentBody::Moon,
            Self::RiseSet(event) => event.body(),
            Self::MeridianTransit(event) => event.body(),
        }
    }
}
