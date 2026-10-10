// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Graphshell application's identity: the persona in use, as djinn
//! releases it.
//!
//! Section 3 puts this here and nowhere else. `graphshell-client` stays
//! transport-independent and takes **no** Personae dependency — a client that
//! carried an identity would be a client that could only run where that
//! identity lives, which is the opposite of the portable boundary. The
//! *application* composes Personae; the protocol crates never see it.
//!
//! ## Through djinn (dramatis repo plan, D5, D11, D12)
//!
//! Graphshell opens no vault. Its session keys are namespaced derived keys
//! djinn releases on the custody route, each with the master's attestation;
//! the master never leaves djinn. With djinn absent or Locked the identity is
//! pending ([`crate::native::custody_client::CustodyClientError::is_pending`]),
//! never a key of Graphshell's own: a peer pins the key it reached, and a
//! silently different one is indistinguishable from an impostor. The vault
//! and the profile selection this module once held moved to djinn in DR-C.
//!
//! ## Selection
//!
//! The profile is the **family choice**, which djinn resolves beside the
//! shared vault. `GRAPHSHELL_PROFILE` ([`env_profile`]) sits above the family
//! ladder as this application's own override, for a host that genuinely
//! should speak as someone else (a receipt run, a second resident on one
//! machine); djinn honours it when it resolves.

use dramatis::view::VaultProtectionView;
use insigne::DerivedKeyAttestation;
use personae::vault::ProfileId;
use personae::{Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider, RetainedKeys};

use crate::native::custody::{CustodyRefusal, KeySource};
use crate::native::custody_client::{CustodyClient, CustodyClientError};

pub const PROFILE_ENV: &str = "GRAPHSHELL_PROFILE";

pub fn env_profile() -> Option<ProfileId> {
    std::env::var(PROFILE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(ProfileId)
}

/// The derivation salt for one session's endpoint key.
pub fn session_salt(session: &str) -> Vec<u8> {
    format!("graphshell/endpoint-session/{session}").into_bytes()
}

/// The persona Graphshell speaks as, holding only the session keys djinn
/// released for it.
pub struct GraphshellIdentity {
    keys: RetainedKeys,
    profile: ProfileId,
    protection: VaultProtectionView,
}

impl GraphshellIdentity {
    /// Ask djinn for the persona in use and the session keys of `sessions`.
    /// A pending identity (djinn absent or Locked) is an error the caller
    /// shows as pending, never a reason to mint a key of its own.
    pub async fn from_djinn(
        client: &mut CustodyClient,
        sessions: &[&str],
    ) -> Result<Self, CustodyClientError> {
        let status = client.status().await?;
        let profile = status
            .profile
            .ok_or(CustodyClientError::Refused(CustodyRefusal::Locked))?;
        let salts = sessions
            .iter()
            .map(|session| session_salt(session))
            .collect();
        let keys = client.release(KeySource::Persona, salts).await?;
        Ok(Self::from_keys(keys, profile, status.protection))
    }

    /// Wrap keys already released, for a host that asked for them itself.
    pub fn from_keys(
        keys: RetainedKeys,
        profile: ProfileId,
        protection: VaultProtectionView,
    ) -> Self {
        Self {
            keys,
            profile,
            protection,
        }
    }

    /// The profile this identity speaks as.
    pub fn profile(&self) -> &ProfileId {
        &self.profile
    }

    /// What protects the key at rest, as djinn reports it — shown, never
    /// guessed.
    pub fn protection(&self) -> VaultProtectionView {
        self.protection
    }

    /// The durable identity a remote peer pins.
    pub fn master_public_key(&self) -> Ed25519PublicKey {
        self.keys.master_public_key()
    }

    /// The derivation salt for one session's endpoint key.
    pub fn session_salt(session: &str) -> Vec<u8> {
        session_salt(session)
    }

    /// A per-session endpoint keypair, derived from the profile identity.
    ///
    /// The same shape Turnstone's projection endpoint uses: a session acts
    /// under its own key rather than the master, so a compromised session key
    /// is not the profile. Locked for a session djinn did not release.
    pub fn session_key(&self, session: &str) -> Result<Ed25519Keypair, IdentityError> {
        self.keys.derive_keypair(&session_salt(session))
    }

    /// **The endpoint-key proof.** A master-signed attestation that this
    /// session's derived key belongs to this profile.
    ///
    /// This is what replaces the `endpoint_subject` field deleted in G5a.1. A
    /// key asserted in one's own frame proves nothing; an attestation is
    /// checkable against the master identity the carrier authenticated, which
    /// is what lets a client believe that the session key it is talking to and
    /// the peer its transport proved are the same party.
    pub fn attest_session_key(
        &self,
        session: &str,
    ) -> Result<DerivedKeyAttestation, IdentityError> {
        self.keys.attest_derived_key(&session_salt(session))
    }
}

impl IdentityProvider for GraphshellIdentity {
    fn master_public_key(&self) -> Ed25519PublicKey {
        self.keys.master_public_key()
    }

    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        self.keys.derive_keypair(salt)
    }

    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        self.keys.attest_derived_key(salt)
    }
}

pub fn verify_session_key(
    attestation: &DerivedKeyAttestation,
    expected_master: Ed25519PublicKey,
    session: &str,
) -> Option<Ed25519PublicKey> {
    let checked = attestation.check(&session_salt(session)).ok()?;
    if checked.master() != &expected_master.to_bytes() {
        return None;
    }
    Ed25519PublicKey::from_bytes(checked.derived()).ok()
}

#[cfg(test)]
mod tests {
    use personae::InMemoryProvider;

    use super::*;

    /// Session keys released for `sessions`, as djinn releases them.
    fn released(sessions: &[&str]) -> (InMemoryProvider, GraphshellIdentity) {
        let master = InMemoryProvider::from_seed([0x5e; 32]);
        let salts: Vec<Vec<u8>> = sessions.iter().map(|s| session_salt(s)).collect();
        let keys = RetainedKeys::capture(&master, &salts).unwrap();
        let identity = GraphshellIdentity::from_keys(
            keys,
            ProfileId("test".into()),
            VaultProtectionView::Passphrase,
        );
        (master, identity)
    }

    #[test]
    fn a_session_key_is_derived_stably_and_its_proof_verifies() {
        let (full, identity) = released(&["s1", "s2"]);

        let first = identity.session_key("s1").unwrap();
        let again = identity.session_key("s1").unwrap();
        let other = identity.session_key("s2").unwrap();
        assert_eq!(
            first.public_key().to_bytes(),
            again.public_key().to_bytes(),
            "the same session derives the same key"
        );
        assert_ne!(
            first.public_key().to_bytes(),
            other.public_key().to_bytes(),
            "a different session is a different key"
        );
        assert_eq!(
            first.to_seed(),
            full.derive_keypair(&session_salt("s1")).unwrap().to_seed(),
            "the released key is the vault's own derivation"
        );

        // The proof binds that key to this profile's master.
        let attestation = identity.attest_session_key("s1").unwrap();
        let master = identity.master_public_key();
        let proved = verify_session_key(&attestation, master, "s1").expect("the proof verifies");
        assert_eq!(proved.to_bytes(), first.public_key().to_bytes());

        // It does not verify for another session...
        assert!(
            verify_session_key(&attestation, master, "s2").is_none(),
            "a proof is bound to its session"
        );
        // ...nor against a master the carrier did not authenticate.
        let impostor = Ed25519Keypair::from_seed([9; 32]).public_key();
        assert!(
            verify_session_key(&attestation, impostor, "s1").is_none(),
            "a proof is worthless against the wrong identity"
        );
    }

    #[test]
    fn a_session_djinn_did_not_release_is_locked_not_invented() {
        // Graphshell must not invent a key: a peer pins the one it reached.
        let (_, identity) = released(&["s1"]);
        assert!(matches!(
            identity.session_key("s2"),
            Err(IdentityError::Locked)
        ));
    }
}
