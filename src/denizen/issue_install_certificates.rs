// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Issue the root delegation certificates for one participant: the user's identity
/// conferring each reviewed capability directly. `depth` is 0, so an installed
/// helper may act but never sub-delegate.
pub fn issue_install_certificates(
    provider: &crate::identity::RootIdentity,
    subject: Subject,
    caps: &[(Cap, Mode)],
    issued_at_ms: u64,
) -> Vec<SignedDelegationCertificate> {
    issue_install_certificates_for_generation(provider, subject, caps, issued_at_ms, 0)
}
