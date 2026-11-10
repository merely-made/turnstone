// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use insigne::DerivedKeyAttestation;
use murm_sync::{StreamSyncLimits, StreamSyncOutcome};
use transport::security::SetupLimits;
use transport::sync_lane::{SyncLane, SyncLaneError};
use transport::{
    AcceptedSession, Alpn, PeerID, ReticulumInterface, ReticulumStream, ReticulumTransport,
    Transport,
};

use super::{
    Emitter, IdentityProvider, MootAccessLevel, OpenPlace, PlaceWorkerCommand, PlaceWorkerSettings,
    RootIdentity, SessionId, Update, place_snapshot,
};
use crate::place::SharedContainerId;

const CARRIER_SALT: &[u8] = b"turnstone.commons.reticulum.v1/";
const NOISE_SALT: &[u8] = b"turnstone.commons.noise.v1/";
const ACCEPT_DEADLINE: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(50);

/// The TCP interface for one graph catch-up round.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReticulumGraphRoute {
    Listen(SocketAddr),
    Dial(SocketAddr),
}

/// The optional per-place route read when the worker reconnects.
pub const RETICULUM_GRAPH_FILE: &str = "place-reticulum-graph.json";

/// Host-selected contact and polling interval for an already enrolled graph.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReticulumGraphConfig {
    pub container: SharedContainerId,
    pub route: ReticulumGraphRoute,
    pub peer: ReticulumGraphContact,
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}

fn default_poll_ms() -> u64 {
    2_000
}

impl ReticulumGraphConfig {
    pub(super) fn load(directory: &Path) -> Result<Option<Self>, String> {
        let file = match std::fs::File::open(directory.join(RETICULUM_GRAPH_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("open Reticulum graph route: {error}")),
        };
        const MAX_BYTES: u64 = 16 * 1024;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read Reticulum graph route: {error}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Reticulum graph route exceeds its byte limit".into());
        }
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode Reticulum graph route: {error}"))?;
        if !(250..=60_000).contains(&config.poll_ms) {
            return Err("Reticulum graph poll_ms must be between 250 and 60000".into());
        }
        Ok(Some(config))
    }
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
    if open.lanes.is_some() || open.reticulum.is_some() {
        return Err("open the cached place before Reticulum graph catch-up".into());
    }
    let expected = authorize(open, identity, peer)?;
    let link = GraphTransport::bind(open.binding.root, identity, route, peer)?;
    let inbound = match route {
        ReticulumGraphRoute::Listen(_) => Some(link.runtime.block_on(async {
            tokio::time::timeout(
                ACCEPT_DEADLINE,
                link.transport.accept(link.protocol.clone()),
            )
            .await
            .map_err(|_| "Reticulum graph accept timed out".to_string())?
            .map_err(|error| format!("accept Reticulum graph route: {error}"))
        })?),
        ReticulumGraphRoute::Dial(_) => None,
    };
    let outcome = link
        .round(
            open,
            expected,
            inbound,
            StreamSyncLimits::default(),
            ACCEPT_DEADLINE,
        )
        .map_err(|error| format!("catch up Commons graph: {error}"))?;
    if outcome.lagged != 0 {
        return Err("Commons graph catch-up fell behind; retry the round".into());
    }
    Ok(())
}

fn authorize(
    open: &mut OpenPlace,
    identity: &dyn IdentityProvider,
    peer: &ReticulumGraphContact,
) -> Result<PeerID, String> {
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
    PeerID::from_bytes(attestation.derived())
        .map_err(|error| format!("read peer's graph Noise key: {error}"))
}

struct GraphTransport {
    transport: ReticulumTransport,
    runtime: tokio::runtime::Runtime,
    noise: identity::DerivedKeypair,
    protocol: Alpn,
    destination: PeerID,
}

impl GraphTransport {
    fn bind(
        container: SharedContainerId,
        identity: &dyn IdentityProvider,
        route: ReticulumGraphRoute,
        peer: &ReticulumGraphContact,
    ) -> Result<Self, String> {
        let noise = identity
            .derived_keypair(&[NOISE_SALT, &container.0].concat())
            .map_err(|error| format!("derive graph Noise key: {error}"))?;
        let carrier = identity
            .derived_keypair(&[CARRIER_SALT, &container.0].concat())
            .map_err(|error| format!("derive graph carrier key: {error}"))?;
        let destination = PeerID::from_bytes(&peer.carrier)
            .map_err(|error| format!("read Reticulum graph destination: {error}"))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| format!("start graph catch-up runtime: {error}"))?;
        let protocol = Alpn::new("mere/commons-graph-sync/v1");
        let interface = match route {
            ReticulumGraphRoute::Listen(bind) => ReticulumInterface::TcpServer { bind },
            ReticulumGraphRoute::Dial(addr) => ReticulumInterface::TcpClient { addr },
        };
        let transport = runtime
            .block_on(async {
                ReticulumTransport::builder_for(&carrier)
                    .alpns(vec![protocol.clone()])
                    .interfaces(vec![interface])
                    .bind()
                    .await
            })
            .map_err(|error| format!("bind Reticulum graph route: {error}"))?;
        Ok(Self {
            transport,
            runtime,
            noise,
            protocol,
            destination,
        })
    }

    fn take_inbound(&self) -> Result<Option<AcceptedSession<ReticulumStream>>, String> {
        let result = self.runtime.block_on(async {
            tokio::time::timeout(Duration::ZERO, self.transport.accept(self.protocol.clone())).await
        });
        match result {
            Ok(accepted) => accepted
                .map(Some)
                .map_err(|error| format!("accept Reticulum graph route: {error}")),
            Err(_) => Ok(None),
        }
    }

    fn round(
        &self,
        open: &OpenPlace,
        expected: PeerID,
        inbound: Option<AcceptedSession<ReticulumStream>>,
        limits: StreamSyncLimits,
        setup_timeout: Duration,
    ) -> Result<StreamSyncOutcome, SyncLaneError> {
        let lane = SyncLane::new(
            self.protocol.clone(),
            1,
            SetupLimits {
                timeout: setup_timeout,
                ..SetupLimits::default()
            },
        );
        let session = open.graph.stream_sync(limits);
        self.runtime.block_on(async {
            match inbound {
                Some(accepted) => {
                    lane.serve_noise(
                        &self.noise,
                        accepted.into_stream(),
                        |actual| actual == expected,
                        session,
                    )
                    .await
                },
                None => {
                    lane.dial_noise(
                        &self.transport,
                        self.destination,
                        &self.noise,
                        expected,
                        session,
                    )
                    .await
                },
            }
        })
    }
}

pub(super) struct AutoGraphSync {
    config: ReticulumGraphConfig,
    transport: GraphTransport,
    due: Instant,
    error: Option<String>,
}

impl AutoGraphSync {
    pub(super) fn start(
        open: &mut OpenPlace,
        identity: &dyn IdentityProvider,
        config: ReticulumGraphConfig,
    ) -> Result<Self, String> {
        if config.container != open.binding.root {
            return Err("Reticulum graph route belongs to another place".into());
        }
        authorize(open, identity, &config.peer)?;
        let transport =
            GraphTransport::bind(config.container, identity, config.route, &config.peer)?;
        Ok(Self {
            config,
            transport,
            due: Instant::now(),
            error: None,
        })
    }

    pub(super) fn edited(&mut self) {
        self.due = Instant::now();
    }

    fn poll(
        &mut self,
        open: &mut OpenPlace,
        identity: &dyn IdentityProvider,
    ) -> Result<bool, String> {
        let inbound = match self.config.route {
            ReticulumGraphRoute::Listen(_) => match self.transport.take_inbound()? {
                Some(stream) => Some(stream),
                None => return Ok(false),
            },
            ReticulumGraphRoute::Dial(_) => {
                if Instant::now() < self.due
                    || !self
                        .transport
                        .transport
                        .announced_peers(&self.transport.protocol)
                        .contains(&self.transport.destination)
                {
                    return Ok(false);
                }
                None
            },
        };
        let expected = authorize(open, identity, &self.config.peer);
        let result = expected.map(|expected| {
            let limits = StreamSyncLimits::new(1 << 20, 64 << 20, Duration::from_secs(5))
                .expect("valid automatic graph sync limits");
            self.transport
                .round(open, expected, inbound, limits, Duration::from_secs(3))
        });
        self.due = Instant::now() + Duration::from_millis(self.config.poll_ms);
        match result? {
            Ok(outcome) => {
                self.error = None;
                Ok(outcome.received != 0)
            },
            Err(SyncLaneError::Sync(error)) => {
                // Admission may already have stored operations when the round
                // failed. Publish the retained state and let the next poll retry.
                self.report_failure(error.to_string());
                Ok(true)
            },
            Err(error) => Err(format!("catch up Commons graph: {error}")),
        }
    }

    fn report_failure(&mut self, error: String) {
        if self.error.as_deref() != Some(&error) {
            tracing::warn!(%error, "Reticulum graph catch-up failed");
            self.error = Some(error);
        }
    }
}

fn poll_open(open: &mut OpenPlace, identity: &dyn IdentityProvider) -> bool {
    let Some(mut sync) = open.reticulum.take() else {
        return false;
    };
    let refreshed = match sync.poll(open, identity) {
        Ok(refreshed) => refreshed,
        Err(error) => {
            sync.report_failure(error);
            false
        },
    };
    open.reticulum = Some(sync);
    refreshed
}

pub(super) fn next_command(
    commands: &Receiver<PlaceWorkerCommand>,
    open: Option<&mut OpenPlace>,
    scope: Option<(SessionId, u64)>,
    identity: Option<&RootIdentity>,
    settings: &PlaceWorkerSettings,
    out: &Emitter<Update>,
) -> Option<PlaceWorkerCommand> {
    let (Some(open), Some((session, generation)), Some(identity)) = (open, scope, identity) else {
        return commands.recv().ok();
    };
    if open.reticulum.is_none() {
        return commands.recv().ok();
    }
    loop {
        // A queued close or generation change takes precedence over starting
        // a new round against the old place.
        match commands.try_recv() {
            Ok(command) => return Some(command),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {},
        }
        if poll_open(open, identity) {
            let result = place_snapshot(open, open.binding.moot.0, settings);
            out.emit(Update::PlaceOpened {
                session,
                generation,
                result,
            });
        }
        match commands.recv_timeout(TICK) {
            Ok(command) => return Some(command),
            Err(RecvTimeoutError::Timeout) => {},
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}
