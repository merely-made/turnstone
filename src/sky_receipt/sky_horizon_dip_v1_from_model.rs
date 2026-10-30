// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyHorizonDipV1 {
    pub(super) fn from_model(horizon: HorizonDipModel) -> Self {
        let model = SkyModelV1::from_model(horizon.model());
        match (horizon.constant_dip(), horizon.spherical_radius()) {
            (None, None) => Self::Level { model },
            (Some(dip), None) => Self::Constant {
                model,
                dip_radians: dip.radians(),
            },
            (None, Some(radius)) => Self::Spherical {
                model,
                radius_meters: radius.meters(),
            },
            (Some(_), Some(_)) => {
                unreachable!("a validated Turquet horizon has one dip representation")
            },
        }
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        match self {
            Self::Level { model } => model.validate(&format!("{field} horizon-dip model")),
            Self::Constant { model, dip_radians } => {
                model.validate(&format!("{field} horizon-dip model"))?;
                validate_closed_range(
                    &format!("{field} constant horizon dip"),
                    *dip_radians,
                    0.0,
                    FRAC_PI_2,
                )
            },
            Self::Spherical {
                model,
                radius_meters,
            } => {
                model.validate(&format!("{field} horizon-dip model"))?;
                validate_positive(&format!("{field} spherical-horizon radius"), *radius_meters)
            },
        }
    }
}
