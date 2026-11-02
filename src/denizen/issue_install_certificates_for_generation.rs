// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Issue an explicitly re-installed resident's certificates. The generation
/// joins the nonce so a revoked earlier installation cannot make a reviewed
/// owner re-install deterministically reissue the revoked certificate.
pub fn issue_install_certificates_for_generation(
    provider: &crate::identity::RootIdentity,
    subject: Subject,
    caps: &[(Cap, Mode)],
    issued_at_ms: u64,
    generation: u64,
) -> Vec<SignedDelegationCertificate> {
    let issuer = provider.master_public_key().to_bytes();
    let hex = subject.to_hex();
    let resource = residency_resource(&hex);
    let mut signed = Vec::new();
    for (cap, mode) in caps {
        let certificate = root_certificate(
            issuer,
            subject,
            cap,
            *mode,
            resource.clone(),
            issued_at_ms,
            None,
            0,
            cert_nonce(&hex, cap, generation),
        );
        // Signed inside djinn: the residency scope's key never leaves it (D11).
        match provider.issue_certificate(certificate) {
            Ok(cert) => signed.push(cert),
            Err(err) => tracing::warn!(?err, cap = %cap, "failed to sign an install certificate"),
        }
    }
    signed
}
