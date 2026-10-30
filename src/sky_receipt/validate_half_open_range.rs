// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_half_open_range(
    field: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if (minimum..maximum).contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{field} must belong to the half-open range [{minimum}, {maximum})"
        )))
    }
}
