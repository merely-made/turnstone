// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyRefractionV1 {
    pub(super) fn from_model(refraction: RefractionModel) -> Self {
        Self {
            model: SkyModelV1::from_model(refraction.model()),
            apparent_lift_radians: refraction.apparent_lift().radians(),
        }
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        self.model.validate(&format!("{field} refraction model"))?;
        validate_closed_range(
            &format!("{field} apparent refraction lift"),
            self.apparent_lift_radians,
            0.0,
            FRAC_PI_2,
        )
    }
}
