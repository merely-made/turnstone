// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_open_closed_range(
    field: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if value > minimum && value <= maximum {
        Ok(())
    } else {
        Err(invalid(format!(
            "{field} must belong to the range ({minimum}, {maximum}]"
        )))
    }
}
