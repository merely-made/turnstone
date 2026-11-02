// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_time_zone_identifier(value: &str) -> Result<(), SkyReceiptJsonError> {
    validate_nonempty("Sky IANA time-zone identifier", value)?;
    if value.len() > 255
        || !value.is_ascii()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.split('/').any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component.starts_with('-')
                || !component
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        })
    {
        return Err(invalid(format!(
            "Sky time-zone identifier '{value}' is not a sane IANA-style name"
        )));
    }
    Ok(())
}
