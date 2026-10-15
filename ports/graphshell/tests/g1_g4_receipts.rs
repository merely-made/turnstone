// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The committed G1 and G4 receipts against the views that render them.
//!
//! The views are `graphshell_host::view` and `graphshell_host::sessions`
//! (stack lift plan, after P5); the receipts, their binaries (`g1_receipt`,
//! `g4_sessions`) and their docs are this port's, so the checks that tie them
//! together are here.

use graphshell_host::view::render_g1_receipt;

#[test]
fn committed_receipt_matches_the_live_loopback_view() {
    let committed = include_str!("../docs/receipts/g1_loopback.html").replace("\r\n", "\n");
    assert_eq!(render_g1_receipt().unwrap(), committed);
}

#[test]
fn committed_g4_receipt_contains_both_products_and_all_three_sessions() {
    let html = include_str!("../docs/receipts/g4_session_switch.html");
    assert_eq!(html.matches("role=\"tab\"").count(), 3);
    assert_eq!(html.matches("<iframe").count(), 3);
    assert!(html.contains("Turnstone · Browsing graph"));
    assert!(html.contains("Isometry · Player overmap"));
    assert!(html.contains("Isometry · Moor crossing"));
    assert!(html.contains("open-address requires an address"));
    assert!(html.contains("player projection grant is read-only for campaign travel"));
}
