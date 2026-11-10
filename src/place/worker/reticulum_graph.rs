// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use gemot::moot::MootGroupExt;
use gemot::moot::delegation::MootDelegationExt;
use insigne::DerivedKeyAttestation;
use murm_sync::{StreamSession, StreamSync, StreamSyncError, StreamSyncLimits, StreamSyncOutcome};
use stickleback::{GroupKeyAdmission, GroupKeyExt, Operation, group_key_sync_topic};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
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
const POLL_SETUP: Duration = Duration::from_secs(3);
const TICK: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum PlaceLane {
    Membership = 0,
    Delegations = 1,
    Keys = 2,
    Graph = 3,
}

impl PlaceLane {
    const ORDER: [Self; 4] = [Self::Membership, Self::Delegations, Self::Keys, Self::Graph];
}

#[derive(Clone, Copy, Debug)]
enum SyncMode {
    Dial(PlaceLane),
    Listen,
    ListenGraph,
}

/// The TCP interface for place catch-up.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReticulumGraphRoute {
    Listen(SocketAddr),
    Dial(SocketAddr),
}

/// The optional per-place route read when the worker reconnects.
pub const RETICULUM_GRAPH_FILE: &str = "place-reticulum-graph.json";

/// Host-selected contact and polling interval for an already enrolled place.
/// Automatic catch-up reconciles membership, delegations, and group keys before the graph.
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
    let (mode, inbound) = match route {
        ReticulumGraphRoute::Listen(_) => {
            let stream = link.runtime.block_on(async {
                tokio::time::timeout(
                    ACCEPT_DEADLINE,
                    link.transport.accept(link.protocol.clone()),
                )
                .await
                .map_err(|_| "Reticulum graph accept timed out".to_string())?
                .map_err(|error| format!("accept Reticulum graph route: {error}"))
            })?;
            (SyncMode::ListenGraph, Some(stream))
        },
        ReticulumGraphRoute::Dial(_) => (SyncMode::Dial(PlaceLane::Graph), None),
    };
    let outcome = link
        .round(
            expected,
            inbound,
            PlaceSync {
                open,
                mode,
                limits: StreamSyncLimits::default(),
            },
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
        let interface = match route {
            ReticulumGraphRoute::Listen(bind) => ReticulumInterface::TcpServer { bind },
            ReticulumGraphRoute::Dial(addr) => ReticulumInterface::TcpClient { addr },
        };
        let protocol = Alpn::new("mere/commons-place-sync/v1");
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
        expected: PeerID,
        inbound: Option<AcceptedSession<ReticulumStream>>,
        session: impl StreamSession,
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
    phase: usize,
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
            phase: 0,
            error: None,
        })
    }

    pub(super) fn edited(&mut self) {
        self.due = Instant::now();
    }

    pub(super) fn authority_changed(&mut self) {
        self.phase = 0;
        self.edited();
    }

    fn poll(
        &mut self,
        open: &mut OpenPlace,
        identity: &dyn IdentityProvider,
    ) -> Result<bool, String> {
        let (mode, inbound) = match self.config.route {
            ReticulumGraphRoute::Listen(_) => match self.transport.take_inbound()? {
                Some(stream) => (SyncMode::Listen, Some(stream)),
                None => return Ok(false),
            },
            ReticulumGraphRoute::Dial(_) => {
                let lane = PlaceLane::ORDER[self.phase];
                if Instant::now() < self.due
                    || !self
                        .transport
                        .transport
                        .announced_peers(&self.transport.protocol)
                        .contains(&self.transport.destination)
                {
                    return Ok(false);
                }
                (SyncMode::Dial(lane), None)
            },
        };
        tracing::debug!(?mode, "Reticulum place sync round");
        let phase = std::mem::take(&mut self.phase);
        let limits = StreamSyncLimits::new(1 << 20, 64 << 20, Duration::from_secs(5))
            .expect("valid automatic place sync limits");
        let result = authorize(open, identity, &self.config.peer).map(|expected| {
            self.transport.round(
                expected,
                inbound,
                PlaceSync { open, mode, limits },
                POLL_SETUP,
            )
        });
        self.due = Instant::now() + Duration::from_millis(self.config.poll_ms);
        let result = result?;
        tracing::debug!(?mode, ?result, "Reticulum place sync round finished");
        if matches!(mode, SyncMode::Listen | SyncMode::Dial(PlaceLane::Keys)) {
            // Even a failed round can have stored a usable prefix.
            open.follow_group_keys(identity)?;
        }
        match result {
            Ok(outcome) if outcome.lagged != 0 => {
                self.report_failure("Reticulum place catch-up fell behind; retrying".into());
                Ok(outcome.received != 0)
            },
            Ok(outcome) => {
                if matches!(mode, SyncMode::Dial(lane) if !matches!(lane, PlaceLane::Graph)) {
                    // Let queued worker commands run before the next lane.
                    self.phase = phase + 1;
                    self.due = Instant::now();
                }
                if matches!(mode, SyncMode::Listen | SyncMode::Dial(PlaceLane::Graph)) {
                    self.error = None;
                }
                Ok(outcome.received != 0)
            },
            Err(SyncLaneError::Sync(error)) => {
                // Admission may already have stored operations when the round
                // failed. Publish the retained state and let the next poll retry.
                self.report_failure(format!("{mode:?}: {error}"));
                Ok(true)
            },
            Err(error) => Err(format!("catch up Reticulum place {mode:?}: {error}")),
        }
    }

    fn report_failure(&mut self, error: String) {
        if self.error.as_deref() != Some(&error) {
            tracing::warn!(%error, "Reticulum place catch-up failed");
            self.error = Some(error);
        }
    }
}

struct PlaceSync<'a> {
    open: &'a OpenPlace,
    mode: SyncMode,
    limits: StreamSyncLimits,
}

impl StreamSession for PlaceSync<'_> {
    async fn run<S>(self, mut stream: S) -> Result<StreamSyncOutcome, StreamSyncError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let Self { open, mode, limits } = self;
        // One Reticulum announcement covers all lanes. Select one domain per
        // stream, after Murm has authenticated and admitted the peer.
        let lane = tokio::time::timeout(POLL_SETUP, async {
            if let SyncMode::Dial(lane) = mode {
                stream.write_u8(lane as u8).await?;
                stream.flush().await?;
                Ok(lane)
            } else {
                let id = stream.read_u8().await?;
                PlaceLane::ORDER
                    .into_iter()
                    .find(|lane| *lane as u8 == id)
                    .filter(|lane| {
                        !matches!(mode, SyncMode::ListenGraph) || matches!(lane, PlaceLane::Graph)
                    })
                    .ok_or_else(|| std::io::Error::other("unknown or disabled place lane"))
            }
        })
        .await
        .map_err(|_| StreamSyncError::Deadline)?
        .map_err(|error| StreamSyncError::Protocol(error.to_string()))?;
        let moot = open.binding.moot.0;
        match lane {
            PlaceLane::Membership => {
                let store = open.moot.membership_store().clone();
                let session = StreamSync::<_, u64, _, _>::new(
                    moot,
                    store.sync_store(),
                    limits,
                    move |operation: Operation<MootGroupExt>| {
                        let store = store.clone();
                        async move { matches!(store.accept(&operation).await, Ok(true)) }
                    },
                );
                session.run(stream).await
            },
            PlaceLane::Delegations => {
                let store = open.moot.delegation_store().clone();
                let session = StreamSync::<_, u64, _, _>::new(
                    moot,
                    store.sync_store(),
                    limits,
                    move |operation: Operation<MootDelegationExt>| {
                        let store = store.clone();
                        async move { matches!(store.accept(&operation).await, Ok(true)) }
                    },
                );
                session.run(stream).await
            },
            PlaceLane::Keys => {
                let lane = &open.group.lane;
                let receiver = lane.receiver();
                let session = StreamSync::<_, u64, _, _>::new(
                    group_key_sync_topic(lane.group()),
                    lane.sync_store(),
                    limits,
                    move |operation: Operation<GroupKeyExt>| {
                        let receiver = receiver.clone();
                        async move {
                            matches!(
                                receiver.accept(&operation).await,
                                Ok(GroupKeyAdmission::Inserted { .. })
                            )
                        }
                    },
                );
                session.run(stream).await
            },
            PlaceLane::Graph => open.graph.stream_sync(limits).run(stream).await,
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
