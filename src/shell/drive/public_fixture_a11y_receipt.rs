// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn public_fixture_a11y_receipt() -> Result<bool, String> {
    match std::env::var("TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT") {
        Ok(value) => parse_public_fixture_a11y_receipt(Some(&value)),
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(_) => Err("TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT is not valid Unicode".into()),
    }
}
