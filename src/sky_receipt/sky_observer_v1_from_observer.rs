// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyObserverV1 {
    pub(super) fn from_observer(observer: Observer) -> Self {
        Self {
            longitude_east_radians: observer.longitude().radians(),
            latitude_radians: observer.latitude().radians(),
            height_meters_above_wgs84_ellipsoid: observer.height().meters(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        validate_half_open_range(
            "Sky observer east longitude",
            self.longitude_east_radians,
            -PI,
            PI,
        )?;
        validate_closed_range(
            "Sky observer latitude",
            self.latitude_radians,
            -FRAC_PI_2,
            FRAC_PI_2,
        )?;
        validate_finite(
            "Sky observer ellipsoid height",
            self.height_meters_above_wgs84_ellipsoid,
        )
    }
}
