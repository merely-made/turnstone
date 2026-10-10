// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What a local session is, independent of who opened it.
//!
//! A browser relay and a first-party application differ in how they announce
//! themselves and in what they are then allowed to ask for. They do not differ
//! in what a session *is*: a local grant minted against this identity, a
//! closed network policy admitting exactly the projection service, and an
//! endpoint composed over the resident surface. That middle is here, so the
//! two doors cannot drift into two answers about what an admitted local client
//! holds.

use personae::delegation::Issue;
use std::sync::Arc;

use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use notochord::{
    AdmittedSession, LocalNetworkPolicy, NetworkId, ProfileRef, RevocationLedger, ServiceAccess,
    ServiceRule, TrustedRoot,
};
use personae::delegation::DelegationError;
use personae::{IdentityError, IdentityProvider, InMemoryProvider, RetainedKeys};
use tokio::io::DuplexStream;

use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
use crate::browser_carrier::{BrowserSessionClient, LocalLink, admit_local_session};
use crate::identity_endpoint::IdentityEndpoint;
use crate::lifecycle::SessionAuthority;
use crate::native::browser_host::{BrowserHostError, now_ms};
use crate::native::device_broker::DeviceSurface;
use crate::native::resident_identity::ResidentIdentity;

const NETWORK_DOMAIN: &[u8] = b"mere.graphshell/local-browser-network/v1";
const ROOT_DOMAIN: &[u8] = b"mere.graphshell/local-browser-root/v1";
const PROFILE_ID: &str = "mere.base";

/// An admitted local client, before any door-specific framing.
pub(crate) struct LocalSession {
    /// The client end of the admitted duplex.
    pub client: BrowserSessionClient,
    /// The admitted session the endpoint is served over.
    pub admitted: AdmittedSession<DuplexStream>,
    /// Revocations consulted for the life of the session.
    pub revocations: RevocationLedger,
}

/// What a door admits its sessions with (vault lock rulings 40 and 46): a
/// restricted provider holding only the door's two keys, which the vault lock
/// leaves in place. They are the grant's delegation signer and the session
/// signer bound to the door's local network, so they open nothing else.
pub trait DoorIdentity: Send + Sync {
    /// The door's kept keys, or why there are none.
    fn door_keys(&self) -> Result<Arc<RetainedKeys>, IdentityError>;
}

/// The resident host keeps the door keys through every lock.
/// A fixture provider never locks; it captures afresh.
impl DoorIdentity for InMemoryProvider {
    fn door_keys(&self) -> Result<Arc<RetainedKeys>, IdentityError> {
        RetainedKeys::capture(self, &door_salts(self.master_public_key().to_bytes())).map(Arc::new)
    }
}

/// The door's two salts for `subject`: the local grant's delegation signer
/// and the session signer bound to its local network.
pub fn door_salts(subject: [u8; 32]) -> Vec<Vec<u8>> {
    let network = local_network(subject);
    vec![
        insigne::delegation::delegation_signing_salt(&local_scope(network)),
        notochord::network_session_signing_salt(&network),
    ]
}

/// Mint a local grant for `link` and admit it.
///
/// The domains are named `local-browser-*` because the browser door minted the
/// first of these. They are kept as they are on purpose: they are hashed into
/// a network id, so renaming them would silently change every derived network
/// rather than clarify anything.
pub(crate) async fn admit_local_client<D: DoorIdentity + ?Sized>(
    door: &D,
    link: &LocalLink,
    session_duration_ms: u64,
) -> Result<LocalSession, BrowserHostError> {
    let keys = door.door_keys().map_err(BrowserHostError::DoorKeys)?;
    let identity = keys.as_ref();
    let subject = identity.master_public_key().to_bytes();
    let network = local_network(subject);
    let root = local_root(subject);
    let grant = local_grant(
        identity,
        network,
        root,
        link.host_nonce,
        now_ms().saturating_add(session_duration_ms),
    )?;
    let profile = ProfileRef {
        id: PROFILE_ID.to_string(),
        revision: 1,
    };
    let mut policy = LocalNetworkPolicy::closed(network);
    policy.trusted_roots = vec![TrustedRoot {
        authority: root,
        issuer: subject,
    }];
    policy.accepted_profiles = vec![profile.clone()];
    policy.services.insert(
        PROJECTION_SERVICE.to_string(),
        ServiceRule::new(
            ServiceAccess::MemberOnly,
            GRAPHSHELL_DOMAIN,
            [CONNECT_ACTION],
            false,
            Some(1),
        ),
    );
    let revocations = RevocationLedger::new();
    let (client, admitted) = admit_local_session(
        identity,
        network,
        profile,
        vec![grant],
        link,
        &policy,
        &revocations,
        now_ms(),
    )
    .await?;
    Ok(LocalSession {
        client,
        admitted,
        revocations,
    })
}

/// Compose the identity endpoint an admitted local client is served.
///
/// Both doors compose the same one. A card naming content the session cannot
/// then read would be a card that lies, so the reader and the released blobs
/// travel with the cards rather than being wired per door.
pub(crate) fn identity_endpoint_for<A: ResidentIdentity + ?Sized>(
    personae: Arc<A>,
    authority: &SessionAuthority,
    surface: DeviceSurface,
) -> IdentityEndpoint<A> {
    let mut endpoint =
        IdentityEndpoint::for_admitted_with_cards(personae, authority, surface.cards);
    endpoint.with_decisions(surface.decisions);
    // Content a card names but no transfer staged — a receipt's captures — is
    // read from the store on demand rather than held resident, so browsing a
    // graph of receipts costs one blob at a time instead of all of them.
    if let Some(reader) = surface.blob_reader.clone() {
        endpoint.with_reader(Box::new(move |resource| reader(resource)));
    }
    // A transfer whose bytes are too large to hold resident is refused here
    // rather than part-served: the session continues without it, and the
    // reason is logged where the operator can see it. Serving half a transfer
    // would look to the client like a transfer that worked.
    if !surface.released_blobs.is_empty() {
        let count = surface.released_blobs.len();
        match endpoint.release_transfer(surface.released_blobs) {
            Ok(()) => tracing::info!(blobs = count, "released transfer blobs to this client"),
            Err(error) => tracing::warn!(%error, "transfer blobs were not released"),
        }
    }
    endpoint
}

fn local_network(subject: [u8; 32]) -> NetworkId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(NETWORK_DOMAIN);
    hasher.update(&subject);
    NetworkId(*hasher.finalize().as_bytes())
}

fn local_scope(network: NetworkId) -> CapabilityScope {
    CapabilityScope {
        domain: GRAPHSHELL_DOMAIN.to_string(),
        resource: network.0.to_vec(),
        path_prefix: PROJECTION_SERVICE.to_string(),
        actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
    }
}

fn local_root(subject: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ROOT_DOMAIN);
    hasher.update(&subject);
    *hasher.finalize().as_bytes()
}

fn local_grant<P: IdentityProvider>(
    identity: &P,
    network: NetworkId,
    root: [u8; 32],
    nonce: [u8; 32],
    expires_at_ms: u64,
) -> Result<SignedDelegationCertificate, DelegationError> {
    let issued_at_ms = now_ms().saturating_sub(5_000);
    SignedDelegationCertificate::issue(
        identity,
        DelegationCertificate::new(
            DelegationParent::Root(root),
            identity.master_public_key().to_bytes(),
            identity.master_public_key().to_bytes(),
            local_scope(network),
            issued_at_ms,
            issued_at_ms,
            Some(expires_at_ms),
            1,
            nonce,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_carrier::{BrowserChallenge, BrowserMessage};

    /// The transcript binds the client label, so two clients answering the
    /// same challenge with the same nonce still get different sessions. This
    /// is what keeps a first-party name from being replayable as a browser.
    #[test]
    fn two_labels_on_one_challenge_do_not_share_a_link() {
        let challenge = BrowserChallenge::fresh();
        let connect = BrowserMessage::Connect {
            schema: "mere.graphshell/browser-connect/v1".to_string(),
            host_nonce: challenge.host_nonce.clone(),
            client_nonce: challenge.host_nonce.clone(),
        };
        let app = LocalLink::accept("turnstone".to_string(), &challenge, connect.clone()).unwrap();
        let browser =
            LocalLink::accept("chrome-extension://abc/".to_string(), &challenge, connect).unwrap();
        assert_ne!(app.shared_link, browser.shared_link);
    }
}
