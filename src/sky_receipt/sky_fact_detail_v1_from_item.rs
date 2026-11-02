// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyFactDetailV1 {
    pub(super) fn from_item(item: &SkyTimelineItem) -> Self {
        match item {
            SkyTimelineItem::Position(fact) => {
                let observation = fact.observation().value();
                let equatorial = observation.equatorial();
                let equatorial_direction = equatorial.direction();
                let horizon = observation.horizon();
                Self::Position {
                    right_ascension_radians: equatorial_direction.longitude().radians(),
                    declination_radians: equatorial_direction.latitude().radians(),
                    azimuth_radians: horizon.longitude().radians(),
                    altitude_radians: horizon.latitude().radians(),
                    distance_meters: equatorial.distance().meters(),
                }
            },
            SkyTimelineItem::LunarPhase(event) => Self::LunarPhase {
                angular_separation_radians: event.angular_separation().radians(),
            },
            SkyTimelineItem::RiseSet(event) => Self::RiseSet {
                airless_center_altitude_radians: event.airless_center_altitude().radians(),
                refraction_offset_radians: event.refraction_offset().radians(),
                limb_offset_radians: event.limb_offset().radians(),
                horizon_dip_offset_radians: event.horizon_dip_offset().radians(),
            },
            SkyTimelineItem::MeridianTransit(event) => Self::MeridianTransit {
                midpoint_altitude_radians: event.midpoint_altitude().radians(),
            },
        }
    }

    pub(super) fn kind(&self) -> SkyFactDetailKind {
        match self {
            Self::Position { .. } => SkyFactDetailKind::Position,
            Self::LunarPhase { .. } => SkyFactDetailKind::LunarPhase,
            Self::RiseSet { .. } => SkyFactDetailKind::RiseSet,
            Self::MeridianTransit { .. } => SkyFactDetailKind::MeridianTransit,
        }
    }

    pub(super) fn validate(&self, index: usize) -> Result<(), SkyReceiptJsonError> {
        match self {
            Self::Position {
                right_ascension_radians,
                declination_radians,
                azimuth_radians,
                altitude_radians,
                distance_meters,
            } => {
                validate_half_open_range(
                    &format!("Sky position fact {index} right ascension"),
                    *right_ascension_radians,
                    0.0,
                    TAU,
                )?;
                validate_closed_range(
                    &format!("Sky position fact {index} declination"),
                    *declination_radians,
                    -FRAC_PI_2,
                    FRAC_PI_2,
                )?;
                validate_half_open_range(
                    &format!("Sky position fact {index} azimuth"),
                    *azimuth_radians,
                    0.0,
                    TAU,
                )?;
                validate_closed_range(
                    &format!("Sky position fact {index} altitude"),
                    *altitude_radians,
                    -FRAC_PI_2,
                    FRAC_PI_2,
                )?;
                validate_nonnegative(
                    &format!("Sky position fact {index} distance"),
                    *distance_meters,
                )
            },
            Self::LunarPhase {
                angular_separation_radians,
            } => validate_closed_range(
                &format!("Sky lunar-phase fact {index} angular separation"),
                *angular_separation_radians,
                0.0,
                PI,
            ),
            Self::RiseSet {
                airless_center_altitude_radians,
                refraction_offset_radians,
                limb_offset_radians,
                horizon_dip_offset_radians,
            } => {
                validate_closed_range(
                    &format!("Sky rise/set fact {index} airless center altitude"),
                    *airless_center_altitude_radians,
                    -FRAC_PI_2,
                    FRAC_PI_2,
                )?;
                validate_closed_range(
                    &format!("Sky rise/set fact {index} refraction offset"),
                    *refraction_offset_radians,
                    0.0,
                    FRAC_PI_2,
                )?;
                validate_closed_range(
                    &format!("Sky rise/set fact {index} limb offset"),
                    *limb_offset_radians,
                    0.0,
                    FRAC_PI_2,
                )?;
                validate_closed_range(
                    &format!("Sky rise/set fact {index} horizon-dip offset"),
                    *horizon_dip_offset_radians,
                    0.0,
                    FRAC_PI_2,
                )
            },
            Self::MeridianTransit {
                midpoint_altitude_radians,
            } => validate_closed_range(
                &format!("Sky meridian-transit fact {index} midpoint altitude"),
                *midpoint_altitude_radians,
                -FRAC_PI_2,
                FRAC_PI_2,
            ),
        }
    }
}
