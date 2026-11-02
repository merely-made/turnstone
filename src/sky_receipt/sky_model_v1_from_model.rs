// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyModelV1 {
    pub(super) fn from_model(model: Model) -> Self {
        Self {
            name: model.name().to_owned(),
            revision: model.revision().to_owned(),
        }
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        validate_nonempty(&format!("{field} name"), &self.name)?;
        validate_nonempty(&format!("{field} revision"), &self.revision)
    }
}
