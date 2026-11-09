// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::net::SocketAddr;
use std::time::Duration;

use insigne::DerivedKeyAttestation;
use murm_sync::StreamSyncLimits;
use transport::security::SetupLimits;
use transport::sync_lane::SyncLane;
use transport::{Alpn, PeerID, ReticulumInterface, ReticulumTransport, Transport};

use super::{IdentityProvider, MootAccessLevel, OpenPlace};
use crate::place::SharedContainerId;

const CARRIER_SALT: &[u8] = b"turnstone.commons.reticulum.v1/";
const NOISE_SALT: &[u8] = b"turnstone.commons.noise.v1/";
const ACCEPT_DEADLINE: Duration = Duration::from_secs(10);

/// The TCP interface for one graph catch-up round.
#[derive(Clone, Copy, Debug)]
pub enum ReticulumGraphRoute {
    Listen(SocketAddr),
    Dial(SocketAddr),
}

/// Out-of-band routing hint and signed session identity for one graph.
/// The carrier address helps locate the peer. Read admission uses the
/// root that signed the Noise key and the place's retained membership.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ReticulumGraphContact {
    pub carrier: [u8; 32],
    pub noise: DerivedKeyAttestation,
}

impl ReticulumGraphContact {
    /// Build this profile's contact for exchange with an enrolled peer.
    pub fn for_graph(
        identity: &dyn IdentityProvider,
        container: SharedContainerId,
    ) -> Result<Self, identity::IdentityError> {
        let carrier = identity.derived_keypair(&[CARRIER_SALT, &container.0].concat())?;
        let noise = identity.attest_derived_key(&[NOISE_SALT, &container.0].concat())?;
        Ok(Self {
            carrier: carrier.public_key().to_bytes(),
            noise,
        })
    }
}

pub(super) fn catch_up(
    open: &mut OpenPlace,
    identity: &dyn IdentityProvider,
    route: ReticulumGraphRoute,
    peer: &ReticulumGraphContact,
) -> Result<(), String> {
    if open.lanes.is_some() {
        return Err("open the cached place before Reticulum graph catch-up".into());
    }
    let container = open.binding.root.0;
    let noise_salt = [NOISE_SALT, &container].concat();
    let attestation = peer
        .noise
        .check(&noise_salt)
        .map_err(|error| format!("verify peer's graph Noise key: {error}"))?;
    open.follow_group_keys(identity)?;
    let snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("read graph membership: {error}"))?;
    let group = open
        .group
        .session
        .members()
        .map_err(|error| format!("read graph group: {error}"))?;
    for root in [open.subject, *attestation.master()] {
        let can_read = snapshot
            .membership
            .members
            .iter()
            .any(|member| member.member == root && member.access >= MootAccessLevel::Read);
        let in_group = open
            .group
            .session
            .recipient_for_root(root)
            .is_some_and(|recipient| group.contains(&recipient));
        if !can_read || !in_group {
            return Err("graph catch-up requires both roots to be enrolled readers".into());
        }
    }
    let expected = PeerID::from_bytes(attestation.derived())
        .map_err(|error| format!("read peer's graph Noise key: {error}"))?;
    let noise = identity
        .derived_keypair(&noise_salt)
        .map_err(|error| format!("derive graph Noise key: {error}"))?;
    let carrier = identity
        .derived_keypair(&[CARRIER_SALT, &container].concat())
        .map_err(|error| format!("derive graph carrier key: {error}"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("start graph catch-up runtime: {error}"))?;
    runtime.block_on(async {
        let protocol = Alpn::new("mere/commons-graph-sync/v1");
        let interface = match route {
            ReticulumGraphRoute::Listen(bind) => ReticulumInterface::TcpServer { bind },
            ReticulumGraphRoute::Dial(addr) => ReticulumInterface::TcpClient { addr },
        };
        let transport = ReticulumTransport::builder_for(&carrier)
            .alpns(vec![protocol.clone()])
            .interfaces(vec![interface])
            .bind()
            .await
            .map_err(|error| format!("bind Reticulum graph route: {error}"))?;
        let lane = SyncLane::new(protocol.clone(), 1, SetupLimits::default());
        let session = open.graph.stream_sync(StreamSyncLimits::default());
        let outcome = match route {
            ReticulumGraphRoute::Listen(_) => {
                let accepted = tokio::time::timeout(ACCEPT_DEADLINE, transport.accept(protocol))
                    .await
                    .map_err(|_| "Reticulum graph accept timed out".to_string())?
                    .map_err(|error| format!("accept Reticulum graph route: {error}"))?;
                lane.serve_noise(
                    &noise,
                    accepted.into_stream(),
                    |actual| actual == expected,
                    session,
                )
                .await
            },
            ReticulumGraphRoute::Dial(_) => {
                let destination = PeerID::from_bytes(&peer.carrier)
                    .map_err(|error| format!("read Reticulum graph destination: {error}"))?;
                lane.dial_noise(&transport, destination, &noise, expected, session)
                    .await
            },
        }
        .map_err(|error| format!("catch up Commons graph: {error}"))?;
        if outcome.lagged != 0 {
            return Err("Commons graph catch-up fell behind; retry the round".into());
        }
        Ok(())
    })
}
