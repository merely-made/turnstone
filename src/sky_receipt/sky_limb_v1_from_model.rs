// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyLimbV1 {
    pub(super) fn from_model(limb: LimbModel) -> Self {
        let model = SkyModelV1::from_model(limb.model());
        match (limb.angular_radius(), limb.physical_radius()) {
            (None, None) => Self::Center { model },
            (Some(radius), None) => Self::UpperAngularRadius {
                model,
                angular_radius_radians: radius.radians(),
            },
            (None, Some(radius)) => Self::UpperPhysicalRadius {
                model,
                physical_radius_meters: radius.meters(),
            },
            (Some(_), Some(_)) => {
                unreachable!("a validated Turquet limb has one radius representation")
            },
        }
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        match self {
            Self::Center { model } => model.validate(&format!("{field} limb model")),
            Self::UpperAngularRadius {
                model,
                angular_radius_radians,
            } => {
                model.validate(&format!("{field} limb model"))?;
                validate_open_closed_range(
                    &format!("{field} upper-limb angular radius"),
                    *angular_radius_radians,
                    0.0,
                    FRAC_PI_2,
                )
            },
            Self::UpperPhysicalRadius {
                model,
                physical_radius_meters,
            } => {
                model.validate(&format!("{field} limb model"))?;
                validate_positive(
                    &format!("{field} upper-limb physical radius"),
                    *physical_radius_meters,
                )
            },
        }
    }
}
