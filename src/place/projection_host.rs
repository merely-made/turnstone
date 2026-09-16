// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! T5c: the founder serves its Knot vault to place members by projection.
//!
//! Option A. The document belongs to the mere that holds it; a member edits
//! it live over a projection session and never holds a replica. Everything
//! below the accept already exists in mere — `accept_projection_session`
//! decides who is admitted, `ResidentEndpointCatalog` decides what they
//! reach, `serve_admitted_session_notifying` serves and rings. This module
//! only decides what a *place* offers, to whom, and on which transport.
//!
//! ## The subject is the transport key, not the root
//!
//! Notochord denies a session whose claimed subject is not the peer the
//! carrier authenticated, and the place transport is keyed per place
//! (`lanes::transport_salt`) so that joining two places is not linkable. The
//! two facts collide: a certificate naming a member's Personae root cannot be
//! presented as the leaf of a hello that arrives on the member's place key.
//!
//! So the grant the founder issues has one delegation hop left, and the
//! visitor spends it at dial time on itself: a leaf from the grant to its own
//! place-transport key, which is what the hello then claims. The chain
//! validates, the subject equals the peer, and neither the per-place key nor
//! the invitation's meaning had to move. See `lanes::dial_holder`.
//!
//! Nothing here is authority over Commons or Gemot. The scope is the
//! Graphshell projection triple and nothing else.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
use graphshell::carrier::{admit_accepted_session, projection_alpn, projection_policy};
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::projection_host::ResidentProjectionHost;
use identity::IdentityProvider;
use identity::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use notochord::{NetworkId, ProfileRef, TrustedRoot};
use transport::{P2pandaTransport, Transport};

use crate::place::PlaceProjectionSnapshot;
use crate::place::worker::{AuthorityClock, ProviderRef, place_tag, root_grant_id};

/// The one catalog route a place serves, and the name the status line prints.
pub(crate) const PROJECTION_ROUTE: &str = "knot";

/// How often a served session polls its endpoint for a revision notice. The
/// same order as the K2 test's: a bell is a person waiting to see an edit.
const NOTICE_POLL: Duration = Duration::from_millis(50);

/// The viewing profile both ends name. One value, because both ends are
/// Turnstone; a policy that accepted anything would accept a stranger's.
const PROJECTION_PROFILE_ID: &str = "mere.base";

/// What a live place needs in order to serve its vault, or `None` when this
/// profile has no Knot vault and the place serves nothing.
///
/// Assembled by the worker from its settings and the open binding. The lanes
/// module reads no environment: a vault root is a host input, and the one
/// place it is read is beside every other `TURNSTONE_KNOT_*` variable.
#[derive(Clone, Debug)]
pub(crate) struct ProjectionSetup {
    /// The Knot vault directory this place serves, one endpoint per session.
    pub(crate) root: PathBuf,
    /// The write grant's ceiling, the same one directory mode uses.
    pub(crate) max_source_bytes: u64,
    /// The place's Moot id: the `NetworkId` and the certificate's resource.
    pub(crate) moot: [u8; 32],
    /// The root authority a presented chain must terminate at.
    pub(crate) authority: [u8; 32],
    /// The Personae root entitled to issue under that authority.
    pub(crate) issuer: [u8; 32],
    /// The same clock every other place authority decision reads.
    pub(crate) clock: AuthorityClock,
}

impl ProjectionSetup {
    /// The policy this place admits projection sessions under.
    ///
    /// `projection_policy` is `MemberOnly`, so a dial with no chain that
    /// terminates at `authority` is refused at the door. No membership check
    /// beside the certificate handshake, by the slice's stop rule.
    fn policy(&self) -> notochord::LocalNetworkPolicy {
        projection_policy(
            NetworkId(self.moot),
            vec![TrustedRoot {
                authority: self.authority,
                issuer: self.issuer,
            }],
            vec![projection_profile()],
            None,
        )
    }
}

/// The profile ref both the host's policy and a visitor's hello name.
pub(crate) fn projection_profile() -> ProfileRef {
    ProfileRef {
        id: PROJECTION_PROFILE_ID.into(),
        revision: 1,
    }
}

/// The capability one projection grant carries: connect to this place's
/// projection service, and nothing else anywhere.
pub(crate) fn projection_scope(moot: [u8; 32]) -> CapabilityScope {
    CapabilityScope {
        domain: GRAPHSHELL_DOMAIN.into(),
        resource: moot.to_vec(),
        path_prefix: PROJECTION_SERVICE.into(),
        actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
    }
}

/// Issue the grant a writer's invitation carries.
///
/// One hop is left on purpose: the subject spends it at dial time on a leaf
/// to its own place-transport key, because the handshake binds the claimed
/// subject to the authenticated peer. Zero here would make the grant
/// unusable over a per-place transport.
///
/// The nonce is derived, so the certificate id can be recomputed later to
/// revoke it without having retained the certificate.
pub(crate) fn issue_projection_grant(
    issuer: &dyn IdentityProvider,
    moot: [u8; 32],
    subject: [u8; 32],
    now_ms: u64,
) -> Result<SignedDelegationCertificate, String> {
    let mut nonce_input = Vec::with_capacity(64);
    nonce_input.extend_from_slice(&moot);
    nonce_input.extend_from_slice(&subject);
    let nonce = place_tag(
        b"turnstone.place.projection-grant.v1/",
        *blake3::hash(&nonce_input).as_bytes(),
    );
    SignedDelegationCertificate::issue(
        &ProviderRef(issuer),
        DelegationCertificate::new(
            DelegationParent::Root(root_grant_id(moot)),
            issuer.master_public_key().to_bytes(),
            subject,
            projection_scope(moot),
            now_ms,
            now_ms,
            None,
            1,
            nonce,
        ),
    )
    .map_err(|error| format!("issue projection grant: {error}"))
}

/// The certificate as an invitation artifact carries it.
///
/// JSON rather than a wire codec: the envelope is already a host envelope of
/// digest-checked artifacts, and nothing here is a protocol frame.
pub(crate) fn encode_grant(grant: &SignedDelegationCertificate) -> Result<Vec<u8>, String> {
    serde_json::to_vec(grant).map_err(|error| format!("encode projection grant: {error}"))
}

pub(crate) fn decode_grant(bytes: &[u8]) -> Result<SignedDelegationCertificate, String> {
    serde_json::from_slice(bytes).map_err(|error| format!("decode projection grant: {error}"))
}

/// One place's projection host: an accept loop and what it has served.
///
/// The counters are observations. `live_sessions` is how many peers are being
/// served right now; `refused` is how many were turned away at the door since
/// this bind came up. Neither says a member can reach this host.
pub(crate) struct ProjectionServing {
    live: Arc<AtomicU32>,
    refused: Arc<AtomicU64>,
    /// Every session ever admitted. Not reported to anybody: it exists so a
    /// sampler can tell "one visitor came and went" from "nothing happened",
    /// which `live` alone cannot say once it is back where it started.
    admitted: Arc<AtomicU64>,
    accept: Option<tokio::task::JoinHandle<()>>,
}

impl ProjectionServing {
    /// Register the vault route and start accepting on the place transport.
    ///
    /// The transport is shared rather than borrowed because the accept loop
    /// outlives this call and `accept_one` takes `&T`; the lane owner aborts
    /// this task before it closes the endpoint.
    pub(crate) fn start(
        transport: Arc<P2pandaTransport>,
        handle: &tokio::runtime::Handle,
        setup: &ProjectionSetup,
    ) -> Result<Self, String> {
        let root = setup.root.clone();
        let max_source_bytes = setup.max_source_bytes;
        let mut catalog = ResidentEndpointCatalog::new();
        catalog
            .register_resumable_notifying(PROJECTION_ROUTE, "Knot", move |_| {
                // The admitted context is deliberately unused: the projection
                // is identified by the vault it serves, and the write grant is
                // the holder's own. Authority here is the holder's, always.
                knot::KnotEndpoint::open_writable(
                    &root,
                    knot::KnotWriteGrant::new(max_source_bytes),
                )
                .map_err(|error| error.to_string())
            })
            .map_err(|error| format!("register the place vault route: {error}"))?;
        let route = ResidentEndpointRoute::new(PROJECTION_ROUTE, NOTICE_POLL)
            .map_err(|error| format!("configure the place vault route: {error}"))?;
        let policy = setup.policy();
        let mut host = ResidentProjectionHost::new(policy.clone(), route, catalog);

        let live = Arc::new(AtomicU32::new(0));
        let refused = Arc::new(AtomicU64::new(0));
        let admitted = Arc::new(AtomicU64::new(0));
        let clock = setup.clock;
        let task_live = Arc::clone(&live);
        let task_refused = Arc::clone(&refused);
        let task_admitted = Arc::clone(&admitted);
        let accept = handle.spawn(async move {
            loop {
                // Accept first, THEN sample the clock and the revocation
                // ledger: a grant issued after this loop started waiting must
                // still be judged as of when the visitor actually connected,
                // not the moment the loop began its wait.
                let accepted = match transport.accept(projection_alpn()).await {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        tracing::debug!(%error, "place projection accept loop ended");
                        break;
                    },
                };
                let now_ms = clock.now_ms();
                let ledger_snapshot = host
                    .revocations()
                    .read()
                    .expect("the revocation ledger lock is never poisoned by this host")
                    .clone();
                let live_sessions = host.live_sessions();
                let outcome = admit_accepted_session(
                    accepted,
                    &policy,
                    &ledger_snapshot,
                    now_ms,
                    live_sessions,
                )
                .await;
                match outcome {
                    Ok(Ok(admitted)) => {
                        match host.serve_admitted(admitted, move || clock.now_ms()) {
                            Ok(served) => {
                                task_admitted.fetch_add(1, Ordering::SeqCst);
                                task_live.fetch_add(1, Ordering::SeqCst);
                                let slot = Arc::clone(&task_live);
                                // The host already spawned the serving future;
                                // this only holds the slot until that future
                                // is finished.
                                tokio::spawn(async move {
                                    if let Err(error) = served.finished().await {
                                        tracing::warn!(
                                            %error,
                                            "a served projection did not finish"
                                        );
                                    }
                                    slot.fetch_sub(1, Ordering::SeqCst);
                                });
                            },
                            Err(error) => {
                                tracing::debug!(%error, "place projection accept loop ended");
                                break;
                            },
                        }
                    },
                    Ok(Err(refusal)) => {
                        task_refused.fetch_add(1, Ordering::SeqCst);
                        tracing::info!(?refusal, "projection session refused at the door");
                    },
                    // A handshake that never reached a decision is not a
                    // refusal anyone was told about, but it is a peer turned
                    // away, so it counts and the loop keeps listening.
                    Err(graphshell::carrier::ProjectionAcceptError::Handshake(error)) => {
                        task_refused.fetch_add(1, Ordering::SeqCst);
                        tracing::info!(%error, "projection handshake did not complete");
                    },
                    Err(error) => {
                        tracing::debug!(%error, "place projection accept loop ended");
                        break;
                    },
                }
            }
        });

        Ok(Self {
            live,
            refused,
            admitted,
            accept: Some(accept),
        })
    }

    /// What the lane watcher samples: a number that changes whenever a session
    /// is admitted, refused, or ends. `admitted` rises with `live` on a start
    /// and stands still on an end, so a visit that opens and closes inside one
    /// watch tick still moves the sum.
    pub(crate) fn watch_counters(&self) -> ProjectionWatchCounters {
        ProjectionWatchCounters {
            live: Arc::clone(&self.live),
            refused: Arc::clone(&self.refused),
            admitted: Arc::clone(&self.admitted),
        }
    }

    pub(crate) fn snapshot(&self) -> PlaceProjectionSnapshot {
        PlaceProjectionSnapshot {
            live_sessions: self.live.load(Ordering::SeqCst),
            refused: self.refused.load(Ordering::SeqCst),
            route: PROJECTION_ROUTE.to_string(),
        }
    }

    /// Stop accepting. A session already being served releases its own slot
    /// when its stream ends, exactly as it would have anyway.
    pub(crate) fn abort(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        let accept = self.accept.take();
        if let Some(handle) = &accept {
            handle.abort();
        }
        accept
    }
}

/// The projection counters the lane watcher samples, held apart from the host
/// so the watcher task outlives nothing it does not own.
pub(crate) struct ProjectionWatchCounters {
    live: Arc<AtomicU32>,
    refused: Arc<AtomicU64>,
    admitted: Arc<AtomicU64>,
}

impl ProjectionWatchCounters {
    pub(crate) fn total(&self) -> u64 {
        u64::from(self.live.load(Ordering::SeqCst))
            + self.refused.load(Ordering::SeqCst)
            + self.admitted.load(Ordering::SeqCst)
    }
}
