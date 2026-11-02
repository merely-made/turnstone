// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_optional_nonempty(
    field: &str,
    value: Option<&str>,
) -> Result<(), SkyReceiptJsonError> {
    if let Some(value) = value {
        validate_nonempty(field, value)?;
    }
    Ok(())
}
