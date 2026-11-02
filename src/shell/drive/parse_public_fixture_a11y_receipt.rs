// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn parse_public_fixture_a11y_receipt(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err("TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT must be 0 or 1".into()),
    }
}
