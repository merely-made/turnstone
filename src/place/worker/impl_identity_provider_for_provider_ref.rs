// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl IdentityProvider for ProviderRef<'_> {
    fn master_public_key(&self) -> identity::Ed25519PublicKey {
        self.0.master_public_key()
    }

    fn derive_keypair(
        &self,
        salt: &[u8],
    ) -> Result<identity::Ed25519Keypair, identity::IdentityError> {
        self.0.derive_keypair(salt)
    }

    fn attest_derived_key(
        &self,
        salt: &[u8],
    ) -> Result<insigne::DerivedKeyAttestation, identity::IdentityError> {
        self.0.attest_derived_key(salt)
    }
}
