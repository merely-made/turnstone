// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The committed N4 owner-policy receipt still matches the live scenario.
//!
//! The scenario and its renderer are `graphshell_host::policy_projection`
//! (stack lift plan, L2); the receipt, its binary (`n4_policy_receipt`) and
//! its docs are this port's, so the check that ties them together is here.

use graphshell_host::policy_projection::{render_n4_policy_receipt, run_n4_policy_scenario};
use pandect::PersonaId;

#[test]
fn committed_n4_receipt_matches_the_live_policy_scenario() {
    let root = std::env::temp_dir().join(format!("graphshell-n4-receipt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let receipt = run_n4_policy_scenario(&root, PersonaId::default_persona()).expect("scenario");
    let rendered = render_n4_policy_receipt(&receipt);

    assert_eq!(
        rendered,
        include_str!("../docs/receipts/n4_owner_policy.html")
    );
    std::fs::remove_dir_all(root).expect("remove scratch");
}
