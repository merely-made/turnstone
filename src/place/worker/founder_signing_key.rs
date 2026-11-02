// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Gemot authors delegation facts under the scope-derived key that signed
/// the certificate, not the master key: the master secret stays behind the
/// provider.
pub fn founder_signing_key(
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<identity::Ed25519Keypair, String> {
    identity
        .derive_keypair(&delegation_signing_salt(&place_scope(moot)))
        .map_err(|error| format!("derive place delegation signing key: {error}"))
}
