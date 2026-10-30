// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyObserverTransformV1 {
    pub(super) fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        self.model.validate("Sky observer-transform model")?;
        self.angular_accuracy
            .validate("Sky observer-transform accuracy")
    }
}
