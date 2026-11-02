// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyAccuracyV1 {
    pub(super) fn from_accuracy(accuracy: Accuracy) -> Self {
        Self {
            maximum_error_radians: accuracy.max_angular_error().radians(),
            evidence: match accuracy.evidence() {
                AccuracyEvidence::Conformance => SkyAccuracyEvidenceV1::Conformance,
                AccuracyEvidence::ExternalComparison => SkyAccuracyEvidenceV1::ExternalComparison,
            },
            authority: accuracy.authority().to_owned(),
            scope: accuracy.scope().to_owned(),
        }
    }

    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        validate_closed_range(
            &format!("{field} maximum angular error"),
            self.maximum_error_radians,
            0.0,
            PI,
        )?;
        validate_nonempty(&format!("{field} authority"), &self.authority)?;
        validate_nonempty(&format!("{field} scope"), &self.scope)
    }
}
