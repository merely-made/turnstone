// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Founder-only constitution with ONE root grant over both Commons domains.
///
/// `checkpoint_signer` is the key that authors constitutional events;
/// `grant_subject` is the Personae root that may delegate beneath the grant.
/// They differ in product (a derived founding key signs, the root holds
/// authority) and coincide in fixtures.
pub fn place_rules(
    checkpoint_signer: [u8; 32],
    grant_subject: [u8; 32],
    moot: [u8; 32],
    not_before_ms: u64,
    expires_at_ms: Option<u64>,
) -> ConstitutionRules {
    let mut rules = ConstitutionRules::founder_only(checkpoint_signer);
    rules.grant(CapabilityGrant {
        id: root_grant_id(moot),
        subject: grant_subject,
        path_prefix: place_capability_prefix(),
        not_before_ms,
        expires_at_ms,
        delegation_depth: 2,
    });
    rules
}
