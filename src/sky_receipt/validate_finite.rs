// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_finite(field: &str, value: f64) -> Result<(), SkyReceiptJsonError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid(format!("{field} must be finite")))
    }
}
