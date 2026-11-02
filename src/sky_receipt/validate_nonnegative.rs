// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_nonnegative(field: &str, value: f64) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(invalid(format!("{field} must be nonnegative")))
    }
}
