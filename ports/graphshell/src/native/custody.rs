// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The custody route's wire: what an application asks djinn for, and what it
//! hears back (dramatis repo plan, rulings D5, D11, D12, D17).
//!
//! Every application reaches identity and secrets by calling djinn on the
//! first-party door, never by opening a vault, storage or wallet (D5). The
//! route is [`CUSTODY_ROUTE`]; the server half is the resident's
//! [`ResidentIdentity::custody`](super::resident_identity::ResidentIdentity::custody),
//! the client half [`super::custody_client`]. Both name these types, so they
//! cannot drift.
//!
//! The split follows D11 ("Mixed"): root-level acts ([`CustodyCall::Attest`],
//! [`CustodyCall::Sign`], [`CustodyCall::IssueStationGrant`],
//! [`CustodyCall::RevokeDevice`]) happen inside djinn and only their results
//! cross. The one thing that crosses as a secret is a namespaced derived key
//! an application needs continuously ([`CustodyCall::Release`]): djinn checks
//! the salt against its release policy, never releases the master, and a lock
//! revokes what it released ([`CustodyCall::WatchLock`]).

use std::path::PathBuf;

use dramatis::roster::Roster;
use dramatis::view::{VaultLockView, VaultProtectionView};
use insigne::DerivedKeyAttestation;
use pandect::RemoteAuthRevocationOutcome;
use personae::ProfileId;
use personae::carry::DeviceGrantSet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The resident route custody is served on.
pub const CUSTODY_ROUTE: &str = "custody";

/// Which root a derived key, attestation or signature comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    /// The vault persona in use: transport and sealing keys, mesh authors,
    /// session keys.
    Persona,
    /// The wallet's identity seed: device grants and the stations they
    /// commission (D8, D17).
    Wallet,
}

/// One request on the custody route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum CustodyCall {
    /// The lock, the protection and the public roots. Answered Locked or
    /// not; an application uses it to decide whether to show the identity
    /// as pending (D12).
    Status,
    /// The vault's personas, as a picker shows them.
    Roster,
    /// Switch the persona in use.
    ChooseProfile { profile: ProfileId },
    /// Mint a persona and switch to it.
    CreateProfile {
        profile: ProfileId,
        display_name: String,
    },
    /// Release the derived keys for `salts`, with an attestation for each
    /// (D11). Refused for any salt outside djinn's release policy.
    Release {
        source: KeySource,
        #[serde(with = "b64_list")]
        salts: Vec<Vec<u8>>,
    },
    /// The master's attestation of the key `salt` derives. Signed inside
    /// djinn; no key crosses.
    Attest {
        source: KeySource,
        #[serde(with = "b64")]
        salt: Vec<u8>,
    },
    /// Sign `message` with the key `salt` derives, inside djinn.
    Sign {
        source: KeySource,
        #[serde(with = "b64")]
        salt: Vec<u8>,
        #[serde(with = "b64")]
        message: Vec<u8>,
    },
    /// Issue a sited station's narrow RemoteAuth grant with the wallet's
    /// seed (D17). `issuer_public_key` is the root the station's material
    /// was derived under, checked against the wallet.
    IssueStationGrant {
        request: StationGrantRequest,
        #[serde(with = "b64_32")]
        issuer_public_key: [u8; 32],
    },
    /// Revoke one delegated device in the wallet.
    RevokeDevice { device_id: Uuid },
    /// Wait until the lock leaves `seen`, then answer the new state. An
    /// application holding released keys drops them when this answers
    /// Locked.
    WatchLock { seen: VaultLockView },
}

/// A sited station's grant request, as plain fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationGrantRequest {
    pub device_id: Uuid,
    #[serde(with = "b64_32")]
    pub station_ed25519_public_key: [u8; 32],
    pub label: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

/// What the resident knows about its identity, public parts only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResidentStatus {
    pub lock: VaultLockView,
    pub protection: VaultProtectionView,
    /// The persona in use, absent before the vault has one.
    pub profile: Option<ProfileId>,
    /// The persona's master public key, absent while Locked.
    #[serde(default, with = "b64_32_opt")]
    pub persona_public_key: Option<[u8; 32]>,
    /// The wallet root's public key, absent with no wallet or while Locked.
    #[serde(default, with = "b64_32_opt")]
    pub wallet_public_key: Option<[u8; 32]>,
    /// Where the wallet's public records live, for an application that reads
    /// already-public state (grant sets, the device roster) itself.
    pub wallet_root: Option<PathBuf>,
}

/// One released key: its salt, the derived seed and the master's attestation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasedKey {
    #[serde(with = "b64")]
    pub salt: Vec<u8>,
    #[serde(with = "b64_32")]
    pub seed: [u8; 32],
    pub attestation: DerivedKeyAttestation,
}

impl std::fmt::Debug for ReleasedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReleasedKey")
            .field("salt_len", &self.salt.len())
            .finish_non_exhaustive()
    }
}

impl Drop for ReleasedKey {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.seed);
    }
}

/// djinn's answer to one call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum CustodyAnswer {
    Status(ResidentStatus),
    Roster(Roster),
    /// The call changed state and has nothing to report.
    Done,
    Released {
        #[serde(with = "b64_32")]
        master: [u8; 32],
        keys: Vec<ReleasedKey>,
    },
    Attestation(DerivedKeyAttestation),
    Signature {
        #[serde(with = "b64_32")]
        public_key: [u8; 32],
        #[serde(with = "b64")]
        signature: Vec<u8>,
    },
    StationGrant(DeviceGrantSet),
    Revoked(RemoteAuthRevocationOutcome),
    Lock(VaultLockView),
}

/// Why djinn did not answer a call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum CustodyRefusal {
    /// This host serves no custody route.
    #[error("this host does not serve custody")]
    NotServed,
    /// The vault (or the wallet) is locked: the identity is pending (D12).
    #[error("the identity is locked")]
    Locked,
    /// The salt is outside the release policy; the key stays in djinn.
    #[error("djinn does not release this key")]
    NotReleasable,
    /// The resident has no wallet to act with.
    #[error("the resident has no wallet")]
    NoWallet,
    /// The request itself was refused, with the reason.
    #[error("refused: {reason}")]
    Refused { reason: String },
    /// The act failed inside djinn.
    #[error("failed: {reason}")]
    Failed { reason: String },
}

mod b64 {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        URL_SAFE_NO_PAD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

mod b64_list {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(list: &[Vec<u8>], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(list.iter().map(|bytes| URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Vec<u8>>, D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .into_iter()
            .map(|text| {
                URL_SAFE_NO_PAD
                    .decode(text)
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

mod b64_32 {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        super::b64::serialize(bytes, serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let bytes = super::b64::deserialize(deserializer)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

mod b64_32_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        bytes: &Option<[u8; 32]>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(bytes) => super::b64_32::serialize(bytes, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<[u8; 32]>, D::Error> {
        use base64::Engine;
        let Some(text) = Option::<String>::deserialize(deserializer)? else {
            return Ok(None);
        };
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(text)
            .map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_and_its_answer_round_trip_as_json() {
        let call = CustodyCall::Release {
            source: KeySource::Persona,
            salts: vec![b"mesh-author".to_vec()],
        };
        let text = serde_json::to_string(&call).unwrap();
        assert_eq!(serde_json::from_str::<CustodyCall>(&text).unwrap(), call);

        let status = CustodyAnswer::Status(ResidentStatus {
            lock: VaultLockView::Locked,
            protection: VaultProtectionView::Ephemeral,
            profile: Some(ProfileId("default".into())),
            persona_public_key: None,
            wallet_public_key: Some([3; 32]),
            wallet_root: None,
        });
        let text = serde_json::to_string(&status).unwrap();
        assert_eq!(
            serde_json::from_str::<CustodyAnswer>(&text).unwrap(),
            status
        );
    }

    #[test]
    fn a_released_seed_never_prints() {
        let provider = personae::InMemoryProvider::from_seed([9; 32]);
        let key = ReleasedKey {
            salt: b"mesh-author".to_vec(),
            seed: [0x5a; 32],
            attestation: personae::IdentityProvider::attest_derived_key(&provider, b"mesh-author")
                .unwrap(),
        };
        assert!(!format!("{key:?}").contains("90"));
    }
}
