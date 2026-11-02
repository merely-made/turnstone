// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyEphemerisV1 {
    pub(super) fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        self.model.validate("Sky ephemeris model")?;
        validate_optional_nonempty("Sky ephemeris snapshot", self.snapshot.as_deref())?;
        if let Some(accuracy) = &self.angular_accuracy {
            accuracy.validate("Sky ephemeris accuracy")?;
        }
        Ok(())
    }
}
