// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The profile's root identity: whose authority every participant grant descends
//! from.
//!
//! The capability-model round ruled (OQ2, 2026-07-24) that the user is a **root
//! subject**, not an implicit infinite authority, and that the root is a
//! personae identity rather than a placeholder constant. Install is an
//! attenuating delegation signed by this key; uninstall revokes it.
//!
//! ## Where the key lives
//!
//! **With the configured custodian.** Turnstone opens no vault, storage or
//! wallet (dramatis repo plan, D5). It reaches the current profile root through
//! Djinn's Graphshell custody route as the `turnstone` app. Djinn brokers the
//! route; the configured custodian owns and releases the root material.
//!
//! - Root-level acts happen inside djinn (D11): an install or endpoint
//!   delegation is signed there through [`RootIdentity::issue_certificate`].
//! - The derived keys Turnstone needs continuously (place transports and
//!   sealing, Commons and Gemot writers, Gemini client identities, the
//!   projection endpoint) are released by djinn under its policy and dropped
//!   when the vault locks.
//! - With the route unavailable or custody locked, private identity access is
//!   **pending** (D12). The shell retries bounded connections on a cancellable
//!   background task and adopts only a fresh binding for the expected profile
//!   and root. Already-public browser state remains available.

use std::sync::Arc;

use graphshell_endpoint::door::custody_identity::CustodyIdentity;
use identity::delegation::DelegationError;
use identity::{Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider};
use insigne::DerivedKeyAttestation;
use insigne::delegation::{DelegationCertificate, SignedDelegationCertificate};

/// The name Turnstone is admitted to djinn's custody route under.
pub const CUSTODY_APP: &str = "turnstone";

/// Nonsecret profile and root-key identity used to match a fresh binding.
pub type CustodyPrincipal = (identity::ProfileId, [u8; 32]);

/// The persona Turnstone speaks as.
pub enum RootIdentity {
    /// The persona djinn holds (the only production face).
    Custody(CustodyIdentity),
    /// A test fixture's in-memory root.
    #[cfg(test)]
    Local(identity::InMemoryProvider),
}

impl RootIdentity {
    /// What protects the key, for the boot log and any settings row.
    pub fn description(&self) -> String {
        match self {
            RootIdentity::Custody(custody) => custody.protection(),
            #[cfg(test)]
            RootIdentity::Local(_) => "test fixture (in memory)".to_string(),
        }
    }

    /// Sign a delegation as this root. Inside djinn for the real persona:
    /// the scope's signing key never reaches Turnstone (D11).
    pub fn issue_certificate(
        &self,
        certificate: DelegationCertificate,
    ) -> Result<SignedDelegationCertificate, DelegationError> {
        match self {
            RootIdentity::Custody(custody) => custody.issue_certificate(certificate),
            #[cfg(test)]
            RootIdentity::Local(provider) => {
                use identity::delegation::Issue as _;
                SignedDelegationCertificate::issue(provider, certificate)
            }
        }
    }

    fn provider(&self) -> &dyn IdentityProvider {
        match self {
            RootIdentity::Custody(custody) => custody,
            #[cfg(test)]
            RootIdentity::Local(provider) => provider,
        }
    }
}

impl IdentityProvider for RootIdentity {
    fn master_public_key(&self) -> Ed25519PublicKey {
        self.provider().master_public_key()
    }

    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        self.provider().derive_keypair(salt)
    }

    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        self.provider().attest_derived_key(salt)
    }
}

impl RootIdentity {
    /// This local observation only detects lock/loss transitions since the
    /// custody route was bound. It is never authorization by itself.
    pub fn custody_epoch(&self) -> Option<u64> {
        match self {
            Self::Custody(custody) => custody.custody_epoch(),
            #[cfg(test)]
            Self::Local(_) => Some(1),
        }
    }

    /// Return the stable principal identity used to match a fresh connection
    /// after lock. It contains no private key material.
    pub fn custody_principal(&self) -> Option<CustodyPrincipal> {
        match self {
            Self::Custody(custody) => Some((
                custody.profile()?.clone(),
                self.master_public_key().to_bytes(),
            )),
            #[cfg(test)]
            Self::Local(_) => None,
        }
    }

    /// A bounded fresh status must still name the originally connected
    /// profile and master, with the same observed custody epoch throughout.
    pub fn custody_binding_current(&self) -> bool {
        use identity::IdentityProvider as _;
        match self {
            Self::Custody(custody) => {
                let Some(before) = custody.custody_epoch() else {
                    return false;
                };
                let Ok(status) = custody.status_timeout(std::time::Duration::from_secs(2)) else {
                    return false;
                };
                status.lock == graphshell_endpoint::door::identity::VaultLockView::Unlocked
                    && custody
                        .connected_status()
                        .custody_epoch
                        .as_ref()
                        .is_some_and(|epoch| epoch.valid())
                    && status.custody_epoch == custody.connected_status().custody_epoch
                    && custody.profile().is_some()
                    && status.profile.as_ref() == custody.profile()
                    && status.persona_public_key == Some(custody.master_public_key().to_bytes())
                    && custody.custody_epoch() == Some(before)
            }
            #[cfg(test)]
            Self::Local(_) => true,
        }
    }
}

/// Used only by the cancellable background binding task. No key or passphrase
/// is read here, and the fresh connection has one deadline for its handshake.
pub fn bind_timeout(timeout: std::time::Duration) -> Option<Arc<RootIdentity>> {
    graphshell_endpoint::door::custody_identity::CustodyIdentity::connect_timeout(
        CUSTODY_APP,
        timeout,
    )
    .ok()
    .map(|identity| Arc::new(RootIdentity::Custody(identity)))
}

/// The custodian performs each operation. This adapter holds no epoch key.
impl eidetic::PayloadSealer for RootIdentity {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        bytes: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        match self {
            Self::Custody(custody) => eidetic::PayloadSealer::seal(custody, hash, bytes),
            #[cfg(test)]
            Self::Local(_) => Err(eidetic::Error::new(
                "the local identity fixture has no wallet epoch; inject a sealer",
            )),
        }
    }
    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        bytes: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        match self {
            Self::Custody(custody) => eidetic::PayloadSealer::unseal(custody, hash, marker, bytes),
            #[cfg(test)]
            Self::Local(_) => Err(eidetic::Error::new(
                "the local identity fixture has no wallet epoch; inject a sealer",
            )),
        }
    }
}

/// Synchronous identity fixture helper. Production uses the shell's bounded
/// background connection so no custody RPC runs on the UI thread.
#[cfg(test)]
pub fn bind() -> Option<Arc<RootIdentity>> {
    match CustodyIdentity::connect(CUSTODY_APP) {
        Ok(custody) => {
            let root = RootIdentity::Custody(custody);
            tracing::info!(protection = %root.description(), "profile identity (djinn)");
            Some(Arc::new(root))
        }
        Err(error) if error.is_pending() => {
            tracing::warn!(%error, "profile identity pending: djinn is absent or locked");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "profile identity pending: djinn refused the custody route");
            None
        }
    }
}

/// The refusal every identity-bound act gives while the identity is pending.
pub const PENDING: &str =
    "the profile identity is pending: djinn is not running or the vault is locked";

/// The root subject: the master public key every participant grant descends from.
pub fn root_subject(provider: &impl IdentityProvider) -> servitor::Subject {
    servitor::Subject::new(provider.master_public_key().to_bytes())
}

#[cfg(test)]
impl RootIdentity {
    /// A fixture root from a seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        RootIdentity::Local(identity::InMemoryProvider::from_seed(seed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixture_root_speaks_as_its_seed() {
        let root = RootIdentity::from_seed([3; 32]);
        assert_eq!(
            root_subject(&root),
            root_subject(&identity::InMemoryProvider::from_seed([3; 32]))
        );
    }

}
