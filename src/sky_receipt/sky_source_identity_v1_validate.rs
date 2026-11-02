// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkySourceIdentityV1 {
    pub(super) fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        validate_nonempty(&format!("{field} authority"), &self.authority)?;
        validate_nonempty(&format!("{field} snapshot"), &self.snapshot)
    }
}
