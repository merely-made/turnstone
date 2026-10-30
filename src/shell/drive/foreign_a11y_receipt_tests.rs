// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::parse_public_fixture_a11y_receipt;

#[test]
fn page_text_requires_exact_public_fixture_optin() {
    assert!(!parse_public_fixture_a11y_receipt(None).unwrap());
    assert!(!parse_public_fixture_a11y_receipt(Some("0")).unwrap());
    assert!(parse_public_fixture_a11y_receipt(Some("1")).unwrap());
    for invalid in ["", "true", "yes", "2", " 1"] {
        assert!(parse_public_fixture_a11y_receipt(Some(invalid)).is_err());
    }
}
