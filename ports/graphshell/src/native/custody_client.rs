// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The application side of the custody route: how an app reaches identity
//! and secrets by calling djinn (dramatis repo plan, D5).
//!
//! [`CustodyClient`] is async, for an app that already runs tokio;
//! [`BlockingCustodyClient`] owns a current-thread runtime, for a CLI or a
//! synchronous seam, and must not be used from inside a runtime.
//!
//! **Pending (D12).** When djinn is absent or Locked an app shows the identity
//! as pending: it never falls back to a key of its own and never writes
//! unsealed, and it may still read already-public state.
//! [`CustodyClientError::is_pending`] is the one test for that.
//!
//! **Releases (D11).** [`CustodyClient::release`] returns a
//! [`RetainedKeys`] that answers exactly the salts asked for, every other salt
//! being Locked, with each attestation checked against the released master.
//! An app holding one watches [`CustodyClient::watch_lock`] and drops it
//! when the vault locks.

use dramatis::roster::Roster;
use dramatis::view::VaultLockView;
use insigne::DerivedKeyAttestation;
use pandect::{DeviceId, RemoteAuthRevocationOutcome};
use personae::carry::DeviceGrantSet;
use personae::{Ed25519Keypair, Ed25519PublicKey, IdentityError, ProfileId, RetainedKeys};

use crate::native::app_admission::{AppId, AppRouteId, configured_app_endpoint};
use crate::native::app_client::{AppBrokerClient, AppClientError};
use crate::native::custody::{
    CUSTODY_ROUTE, CustodyAnswer, CustodyCall, CustodyRefusal, EpochKeyRequest, KeySource,
    ReleasedEpochKey, ResidentStatus, StationGrantRequest,
};

/// Why a custody call did not answer.
#[derive(Debug, thiserror::Error)]
pub enum CustodyClientError {
    /// djinn's door could not be reached: the resident is not running, or
    /// this app is not admitted to the custody route.
    #[error("djinn is not reachable: {0}")]
    Absent(AppClientError),
    /// djinn answered with a refusal.
    #[error(transparent)]
    Refused(#[from] CustodyRefusal),
    /// The conversation broke after it opened.
    #[error(transparent)]
    Transport(#[from] AppClientError),
    /// djinn answered a different question.
    #[error("djinn answered {got} to a {asked} call")]
    UnexpectedAnswer {
        asked: &'static str,
        got: &'static str,
    },
    /// A release did not check out against its own attestations.
    #[error("the released keys did not verify: {0}")]
    Release(IdentityError),
}

impl CustodyClientError {
    /// Whether the app must show its identity as pending (D12): djinn is
    /// absent, or the vault is Locked. Every other error is a real failure.
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            Self::Absent(_) | Self::Refused(CustodyRefusal::Locked)
        )
    }
}

/// An open custody session against djinn, as one application.
pub struct CustodyClient {
    inner: AppBrokerClient,
}

impl CustodyClient {
    /// Connect to the configured first-party endpoint as `app`.
    pub async fn open(app: AppId) -> Result<Self, CustodyClientError> {
        Self::open_at(&configured_app_endpoint(), app).await
    }

    /// The same, at an explicit endpoint. Tests and receipts use this.
    pub async fn open_at(endpoint: &str, app: AppId) -> Result<Self, CustodyClientError> {
        let route = AppRouteId::new(CUSTODY_ROUTE).expect("the custody route id is valid");
        let inner = AppBrokerClient::open_route_at(endpoint, app, route)
            .await
            .map_err(CustodyClientError::Absent)?;
        Ok(Self { inner })
    }

    /// One raw call. The typed calls below are what an app normally uses.
    pub async fn call(&mut self, call: CustodyCall) -> Result<CustodyAnswer, CustodyClientError> {
        Ok(self.inner.custody_call(call).await??)
    }

    /// The lock, the protection and the public roots.
    pub async fn status(&mut self) -> Result<ResidentStatus, CustodyClientError> {
        expect_status(self.call(CustodyCall::Status).await?)
    }

    /// The vault's personas, for a picker.
    pub async fn roster(&mut self) -> Result<Roster, CustodyClientError> {
        expect_roster(self.call(CustodyCall::Roster).await?)
    }

    /// Switch the persona in use.
    pub async fn choose_profile(&mut self, profile: ProfileId) -> Result<(), CustodyClientError> {
        expect_done(self.call(CustodyCall::ChooseProfile { profile }).await?)
    }

    /// Mint a persona and switch to it.
    pub async fn create_profile(
        &mut self,
        profile: ProfileId,
        display_name: impl Into<String>,
    ) -> Result<(), CustodyClientError> {
        let display_name = display_name.into();
        expect_done(
            self.call(CustodyCall::CreateProfile {
                profile,
                display_name,
            })
            .await?,
        )
    }

    /// The derived keys for `salts`, released under djinn's policy (D11).
    pub async fn release(
        &mut self,
        source: KeySource,
        salts: Vec<Vec<u8>>,
    ) -> Result<RetainedKeys, CustodyClientError> {
        expect_released(self.call(CustodyCall::Release { source, salts }).await?)
    }

    /// The master's attestation for the key `salt` derives.
    pub async fn attest(
        &mut self,
        source: KeySource,
        salt: Vec<u8>,
    ) -> Result<DerivedKeyAttestation, CustodyClientError> {
        expect_attestation(self.call(CustodyCall::Attest { source, salt }).await?)
    }

    /// Sign `message` inside djinn with the key `salt` derives: the signing
    /// key's public half and the signature.
    pub async fn sign(
        &mut self,
        source: KeySource,
        salt: Vec<u8>,
        message: Vec<u8>,
    ) -> Result<([u8; 32], Vec<u8>), CustodyClientError> {
        expect_signature(
            self.call(CustodyCall::Sign {
                source,
                salt,
                message,
            })
            .await?,
        )
    }

    /// Issue a sited station's grant with the wallet (D17).
    pub async fn issue_station_grant(
        &mut self,
        request: StationGrantRequest,
        issuer_public_key: [u8; 32],
    ) -> Result<DeviceGrantSet, CustodyClientError> {
        expect_grant(
            self.call(CustodyCall::IssueStationGrant {
                request,
                issuer_public_key,
            })
            .await?,
        )
    }

    /// Revoke one delegated device in the wallet.
    pub async fn revoke_device(
        &mut self,
        device_id: DeviceId,
    ) -> Result<RemoteAuthRevocationOutcome, CustodyClientError> {
        expect_revoked(
            self.call(CustodyCall::RevokeDevice {
                device_id: *device_id.as_uuid(),
            })
            .await?,
        )
    }

    /// Keys derived from `persona`'s current private epoch, released under
    /// djinn's policy (D11), in request order.
    pub async fn release_epoch_keys(
        &mut self,
        persona: uuid::Uuid,
        device_label: Option<String>,
        keys: Vec<EpochKeyRequest>,
    ) -> Result<Vec<ReleasedEpochKey>, CustodyClientError> {
        expect_epoch_keys(
            self.call(CustodyCall::ReleaseEpochKeys {
                persona,
                device_label,
                keys,
            })
            .await?,
        )
    }

    /// Wait until the lock leaves `seen`; the new state.
    pub async fn watch_lock(
        &mut self,
        seen: VaultLockView,
    ) -> Result<VaultLockView, CustodyClientError> {
        expect_lock(self.call(CustodyCall::WatchLock { seen }).await?)
    }
}

/// [`CustodyClient`] on its own current-thread runtime, for synchronous
/// callers. Never call it from inside a tokio runtime.
pub struct BlockingCustodyClient {
    runtime: tokio::runtime::Runtime,
    client: CustodyClient,
}

impl BlockingCustodyClient {
    /// Connect to the configured first-party endpoint as `app`.
    pub fn open(app: AppId) -> Result<Self, CustodyClientError> {
        Self::open_at(&configured_app_endpoint(), app)
    }

    /// The same, at an explicit endpoint.
    pub fn open_at(endpoint: &str, app: AppId) -> Result<Self, CustodyClientError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CustodyClientError::Absent(AppClientError::Io(error)))?;
        let client = runtime.block_on(CustodyClient::open_at(endpoint, app))?;
        Ok(Self { runtime, client })
    }

    /// One raw call.
    pub fn call(&mut self, call: CustodyCall) -> Result<CustodyAnswer, CustodyClientError> {
        self.runtime.block_on(self.client.call(call))
    }

    /// See [`CustodyClient::status`].
    pub fn status(&mut self) -> Result<ResidentStatus, CustodyClientError> {
        expect_status(self.call(CustodyCall::Status)?)
    }

    /// See [`CustodyClient::roster`].
    pub fn roster(&mut self) -> Result<Roster, CustodyClientError> {
        expect_roster(self.call(CustodyCall::Roster)?)
    }

    /// See [`CustodyClient::release`].
    pub fn release(
        &mut self,
        source: KeySource,
        salts: Vec<Vec<u8>>,
    ) -> Result<RetainedKeys, CustodyClientError> {
        expect_released(self.call(CustodyCall::Release { source, salts })?)
    }

    /// See [`CustodyClient::attest`].
    pub fn attest(
        &mut self,
        source: KeySource,
        salt: Vec<u8>,
    ) -> Result<DerivedKeyAttestation, CustodyClientError> {
        expect_attestation(self.call(CustodyCall::Attest { source, salt })?)
    }

    /// See [`CustodyClient::sign`].
    pub fn sign(
        &mut self,
        source: KeySource,
        salt: Vec<u8>,
        message: Vec<u8>,
    ) -> Result<([u8; 32], Vec<u8>), CustodyClientError> {
        expect_signature(self.call(CustodyCall::Sign {
            source,
            salt,
            message,
        })?)
    }

    /// See [`CustodyClient::issue_station_grant`].
    pub fn issue_station_grant(
        &mut self,
        request: StationGrantRequest,
        issuer_public_key: [u8; 32],
    ) -> Result<DeviceGrantSet, CustodyClientError> {
        expect_grant(self.call(CustodyCall::IssueStationGrant {
            request,
            issuer_public_key,
        })?)
    }

    /// See [`CustodyClient::release_epoch_keys`].
    pub fn release_epoch_keys(
        &mut self,
        persona: uuid::Uuid,
        device_label: Option<String>,
        keys: Vec<EpochKeyRequest>,
    ) -> Result<Vec<ReleasedEpochKey>, CustodyClientError> {
        expect_epoch_keys(self.call(CustodyCall::ReleaseEpochKeys {
            persona,
            device_label,
            keys,
        })?)
    }

    /// See [`CustodyClient::revoke_device`].
    pub fn revoke_device(
        &mut self,
        device_id: DeviceId,
    ) -> Result<RemoteAuthRevocationOutcome, CustodyClientError> {
        expect_revoked(self.call(CustodyCall::RevokeDevice {
            device_id: *device_id.as_uuid(),
        })?)
    }
}

fn answer_name(answer: &CustodyAnswer) -> &'static str {
    match answer {
        CustodyAnswer::Status(_) => "a status",
        CustodyAnswer::Roster(_) => "a roster",
        CustodyAnswer::Done => "done",
        CustodyAnswer::Released { .. } => "a release",
        CustodyAnswer::Attestation(_) => "an attestation",
        CustodyAnswer::Signature { .. } => "a signature",
        CustodyAnswer::StationGrant(_) => "a station grant",
        CustodyAnswer::Revoked(_) => "a revocation",
        CustodyAnswer::Lock(_) => "a lock state",
        CustodyAnswer::EpochKeys(_) => "epoch keys",
    }
}

fn mismatch(asked: &'static str, got: &CustodyAnswer) -> CustodyClientError {
    CustodyClientError::UnexpectedAnswer {
        asked,
        got: answer_name(got),
    }
}

fn expect_status(answer: CustodyAnswer) -> Result<ResidentStatus, CustodyClientError> {
    match answer {
        CustodyAnswer::Status(status) => Ok(status),
        other => Err(mismatch("status", &other)),
    }
}

fn expect_roster(answer: CustodyAnswer) -> Result<Roster, CustodyClientError> {
    match answer {
        CustodyAnswer::Roster(roster) => Ok(roster),
        other => Err(mismatch("roster", &other)),
    }
}

fn expect_done(answer: CustodyAnswer) -> Result<(), CustodyClientError> {
    match answer {
        CustodyAnswer::Done => Ok(()),
        other => Err(mismatch("profile", &other)),
    }
}

fn expect_released(answer: CustodyAnswer) -> Result<RetainedKeys, CustodyClientError> {
    let (master, keys) = match answer {
        CustodyAnswer::Released { master, keys } => (master, keys),
        other => return Err(mismatch("release", &other)),
    };
    let master = Ed25519PublicKey::from_bytes(&master).map_err(CustodyClientError::Release)?;
    RetainedKeys::from_released(
        master,
        keys.iter().map(|key| {
            (
                key.salt.clone(),
                Ed25519Keypair::from_seed(key.seed),
                key.attestation.clone(),
            )
        }),
    )
    .map_err(CustodyClientError::Release)
}

fn expect_attestation(answer: CustodyAnswer) -> Result<DerivedKeyAttestation, CustodyClientError> {
    match answer {
        CustodyAnswer::Attestation(attestation) => Ok(attestation),
        other => Err(mismatch("attest", &other)),
    }
}

fn expect_signature(answer: CustodyAnswer) -> Result<([u8; 32], Vec<u8>), CustodyClientError> {
    match answer {
        CustodyAnswer::Signature {
            public_key,
            signature,
        } => Ok((public_key, signature)),
        other => Err(mismatch("sign", &other)),
    }
}

fn expect_grant(answer: CustodyAnswer) -> Result<DeviceGrantSet, CustodyClientError> {
    match answer {
        CustodyAnswer::StationGrant(grant) => Ok(grant),
        other => Err(mismatch("station grant", &other)),
    }
}

fn expect_revoked(
    answer: CustodyAnswer,
) -> Result<RemoteAuthRevocationOutcome, CustodyClientError> {
    match answer {
        CustodyAnswer::Revoked(outcome) => Ok(outcome),
        other => Err(mismatch("revoke", &other)),
    }
}

fn expect_epoch_keys(answer: CustodyAnswer) -> Result<Vec<ReleasedEpochKey>, CustodyClientError> {
    match answer {
        CustodyAnswer::EpochKeys(keys) => Ok(keys),
        other => Err(mismatch("release epoch keys", &other)),
    }
}

fn expect_lock(answer: CustodyAnswer) -> Result<VaultLockView, CustodyClientError> {
    match answer {
        CustodyAnswer::Lock(lock) => Ok(lock),
        other => Err(mismatch("watch lock", &other)),
    }
}

#[cfg(test)]
mod tests {
    use personae::{IdentityProvider, InMemoryProvider};

    use super::*;
    use crate::native::custody::ReleasedKey;

    fn release_of(provider: &InMemoryProvider, salts: &[&[u8]]) -> CustodyAnswer {
        CustodyAnswer::Released {
            master: provider.master_public_key().to_bytes(),
            keys: salts
                .iter()
                .map(|salt| ReleasedKey {
                    salt: salt.to_vec(),
                    seed: provider.derive_keypair(salt).unwrap().to_seed(),
                    attestation: provider.attest_derived_key(salt).unwrap(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_release_answers_its_salts_and_locks_the_rest() {
        let provider = InMemoryProvider::from_seed([0x41; 32]);
        let keys = expect_released(release_of(&provider, &[b"mesh-author"])).unwrap();
        assert_eq!(
            keys.derive_keypair(b"mesh-author").unwrap().to_seed(),
            provider.derive_keypair(b"mesh-author").unwrap().to_seed()
        );
        assert!(matches!(
            keys.derive_keypair(b"anything-else"),
            Err(IdentityError::Locked)
        ));
    }

    #[test]
    fn a_release_whose_key_is_not_its_attestations_is_refused() {
        let provider = InMemoryProvider::from_seed([0x42; 32]);
        let CustodyAnswer::Released { master, mut keys } = release_of(&provider, &[b"mesh-author"])
        else {
            unreachable!()
        };
        keys[0].seed = [0x13; 32];
        let forged = CustodyAnswer::Released { master, keys };
        assert!(matches!(
            expect_released(forged),
            Err(CustodyClientError::Release(_))
        ));
    }

    #[test]
    fn absent_and_locked_are_pending_and_nothing_else_is() {
        let absent = CustodyClientError::Absent(AppClientError::Closed);
        assert!(absent.is_pending());
        assert!(CustodyClientError::Refused(CustodyRefusal::Locked).is_pending());
        assert!(!CustodyClientError::Refused(CustodyRefusal::NotReleasable).is_pending());
    }
}
