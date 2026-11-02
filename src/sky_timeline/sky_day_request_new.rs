// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyDayRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        date: Date,
        time_zone_name: impl Into<String>,
        observer: Observer,
        anchor_local_time: Time,
        bodies: Vec<SkyBodySelection>,
        search: SkySearchControls,
    ) -> Result<Self, SkyRequestError> {
        let time_zone_name = time_zone_name.into();
        let time_zone = TimeZone::get(&time_zone_name)
            .map_err(|_| SkyRequestError::UnknownTimeZone(time_zone_name.clone()))?;
        if bodies.is_empty() {
            return Err(SkyRequestError::EmptyBodySet);
        }
        for (index, selected) in bodies.iter().enumerate() {
            if bodies[..index]
                .iter()
                .any(|prior| prior.body == selected.body)
            {
                return Err(SkyRequestError::DuplicateBody(selected.body));
            }
        }
        let request = Self {
            date,
            time_zone,
            time_zone_name,
            observer,
            anchor_local_time,
            bodies,
            search,
        };
        let day = request.resolve_day()?;
        let _ = request.resolve_anchor(&day)?;
        Ok(request)
    }

    pub fn date(&self) -> Date {
        self.date
    }

    pub fn time_zone_name(&self) -> &str {
        &self.time_zone_name
    }

    pub fn observer(&self) -> Observer {
        self.observer
    }

    pub fn anchor_local_time(&self) -> Time {
        self.anchor_local_time
    }

    pub fn bodies(&self) -> &[SkyBodySelection] {
        &self.bodies
    }

    pub fn search(&self) -> SkySearchControls {
        self.search
    }

    pub fn resolve_day(&self) -> Result<ResolvedSkyDay, SkyRequestError> {
        resolve_sky_day(
            self.date,
            self.time_zone.clone(),
            self.time_zone_name.clone(),
        )
    }

    pub(super) fn resolve_anchor(
        &self,
        day: &ResolvedSkyDay,
    ) -> Result<JulianDate<TerrestrialTime>, SkyRequestError> {
        let local = self.date.to_datetime(self.anchor_local_time);
        let utc = resolve_local(&self.time_zone, local, &self.time_zone_name)?;
        let anchor = tt_from_utc(utc);
        if !day.contains(anchor) {
            return Err(SkyRequestError::AnchorOutsideDay);
        }
        Ok(anchor)
    }
}
