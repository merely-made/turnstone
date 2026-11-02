// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_nonempty(field: &str, value: &str) -> Result<(), SkyReceiptJsonError> {
    if value.trim().is_empty() {
        Err(invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}
