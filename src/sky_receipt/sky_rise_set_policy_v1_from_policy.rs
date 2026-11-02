// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyRiseSetPolicyV1 {
    pub(super) fn from_policy(policy: ConventionalRiseSetPolicy) -> Self {
        Self {
            refraction: SkyRefractionV1::from_model(policy.refraction()),
            limb: SkyLimbV1::from_model(policy.limb()),
            horizon_dip: SkyHorizonDipV1::from_model(policy.horizon_dip()),
        }
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        self.refraction.validate(field)?;
        self.limb.validate(field)?;
        self.horizon_dip.validate(field)
    }
}
