// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One signed delegation admitting `subject` to both Commons domains.
///
/// Deterministic in its nonce, so a caller can recompute the certificate id
/// later to revoke it without having retained the certificate.
pub fn place_delegation(
    issuer: &dyn IdentityProvider,
    moot: [u8; 32],
    subject: [u8; 32],
    issued_at_ms: u64,
    not_before_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<SignedDelegationCertificate, String> {
    let mut nonce_input = Vec::with_capacity(64);
    nonce_input.extend_from_slice(&moot);
    nonce_input.extend_from_slice(&subject);
    let nonce = place_tag(
        b"turnstone.place.delegation.v1/",
        *blake3::hash(&nonce_input).as_bytes(),
    );
    SignedDelegationCertificate::issue(
        &ProviderRef(issuer),
        DelegationCertificate::new(
            DelegationParent::Root(root_grant_id(moot)),
            issuer.master_public_key().to_bytes(),
            subject,
            place_scope(moot),
            issued_at_ms,
            not_before_ms,
            expires_at_ms,
            0,
            nonce,
        ),
    )
    .map_err(|error| format!("issue place delegation: {error}"))
}
