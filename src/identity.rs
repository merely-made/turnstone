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
//! **In djinn.** Turnstone opens no vault, storage or wallet (dramatis repo
//! plan, D5). It calls djinn over Graphshell's custody route as the
//! `turnstone` app and speaks as the persona djinn has chosen, so its root is
//! the user's persona, the same one the SSH agent and Graphshell speak as.
//!
//! - Root-level acts happen inside djinn (D11): an install or endpoint
//!   delegation is signed there through [`RootIdentity::issue_certificate`].
//! - The derived keys Turnstone needs continuously (place transports and
//!   sealing, Commons and Gemot writers, Gemini client identities, the
//!   projection endpoint) are released by djinn under its policy and dropped
//!   when the vault locks.
//! - With djinn absent or Locked the identity is **pending** (D12): [`bind`]
//!   answers `None`, nothing signs, nothing is sealed, and there is no
//!   fallback key. Already-public state (the browser, read-only places)
//!   stays readable. [`watch_for_unlock`] asks djinn again until it
//!   answers, and the app, the worker and the services adopt the root then.

use std::sync::Arc;

use graphshell::native::custody_identity::CustodyIdentity;
use identity::delegation::DelegationError;
use identity::{Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider};
use insigne::DerivedKeyAttestation;
use insigne::delegation::{DelegationCertificate, SignedDelegationCertificate};

/// The name Turnstone is admitted to djinn's custody route under.
pub const CUSTODY_APP: &str = "turnstone";

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

/// Reach the profile's root identity through djinn, or `None` while it is
/// pending (djinn absent or Locked, D12). Never mints, opens or falls back
/// to a key of Turnstone's own.
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

/// How often [`watch_for_unlock`] asks djinn again.
const UNLOCK_POLL: std::time::Duration = std::time::Duration::from_secs(1);

/// Wait for a pending identity and hand back the root once djinn answers
/// (djinn started, or a user act unlocked its vault). Calls `wake` after
/// sending; ends with the root sent or the receiver gone.
pub fn watch_for_unlock(
    wake: Arc<dyn Fn() + Send + Sync>,
) -> std::sync::mpsc::Receiver<Arc<RootIdentity>> {
    watch_with(bind, UNLOCK_POLL, wake)
}

fn watch_with(
    bind: impl Fn() -> Option<Arc<RootIdentity>> + Send + 'static,
    poll: std::time::Duration,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> std::sync::mpsc::Receiver<Arc<RootIdentity>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("turnstone-unlock-watch".into())
        .spawn(move || loop {
            std::thread::sleep(poll);
            if let Some(root) = bind() {
                if tx.send(root).is_ok() {
                    wake();
                }
                return;
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not watch for djinn; the profile identity stays pending");
    }
    rx
}

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

    /// The watch hands the root back once djinn answers, and not before.
    #[test]
    fn the_unlock_watch_adopts_the_root_once_djinn_answers() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let poll = std::time::Duration::from_millis(20);
        let asks = Arc::new(AtomicUsize::new(0));
        let counted = asks.clone();
        let woken = Arc::new(AtomicBool::new(false));
        let flag = woken.clone();
        let roots = watch_with(
            move || {
                (counted.fetch_add(1, Ordering::SeqCst) >= 3)
                    .then(|| Arc::new(RootIdentity::from_seed([4; 32])))
            },
            poll,
            Arc::new(move || flag.store(true, Ordering::SeqCst)),
        );
        let root = roots
            .recv_timeout(poll * 100)
            .expect("the root arrives once djinn answers");
        assert!(asks.load(Ordering::SeqCst) >= 4, "pending answers came first");
        assert!(woken.load(Ordering::SeqCst));
        assert_eq!(
            root_subject(root.as_ref()),
            root_subject(&RootIdentity::from_seed([4; 32]))
        );
    }
}
