// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's live-lane composition: dial a ticket, hold nine handles.
//!
//! The shell-owned counterpart to the domain join helpers. Everything here is
//! composition: the transport crate owns dialing and overlay tagging, Gemot
//! owns its seven lanes, Commons owns graph and chat, and this module only
//! decides what a *place* joins and in what order it lets go. Session and
//! transport identity never become content authority; every accept closure
//! runs the owning domain's admission, and projections stay
//! authority-filtered exactly as they are offline.

use std::sync::Arc;

use commons::CommonsExt;
use commons::chat::ChatExt;
use gemot::moot::MootLanes;
use graphshell::admission::open_session;
use graphshell::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use identity::IdentityProvider;
use identity::delegation::{DelegationCertificate, DelegationParent, SignedDelegationCertificate};
use notochord::{HandshakeLimits, NetworkId, TrafficClass};
use stickleback::JoinedSpace;
use transport::p2panda_transport::MdnsDiscoveryMode;
use transport::{P2pandaStream, P2pandaTransport, PeerID, Transport, sync_overlay_topic};

use crate::place::PlaceBindingV1;
use crate::place::projection_host::{
    ProjectionServing, ProjectionSetup, ProjectionWatchCounters, projection_profile,
    projection_scope,
};
use crate::place::worker::{OpenPlace, ProviderRef};

/// How often the watcher samples lane counters, and how long they must hold
/// steady before it reports. A burst of operations from one sync round then
/// settles into a single re-fold instead of one per message.
const WATCH_TICK: std::time::Duration = std::time::Duration::from_millis(250);

/// How long a close waits for all nine lanes to let go of their store clones.
const LEAVE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// One place's joined lanes, plus the transport and runtime that carry them.
///
/// The runtime is retained so the synchronous owner can await watcher
/// cancellation and endpoint close before this value's fields are dropped.
pub(crate) struct LiveLanes {
    watcher: Option<tokio::task::JoinHandle<()>>,
    /// This bind's own dialable ticket(s), read once the endpoint is up.
    local_rendezvous: Vec<String>,
    /// How many peer tickets this bind dialed. Zero is a listen-only bind.
    dialed_rendezvous: usize,
    /// Taken in `drop`: each lane is left and awaited by value, which is the
    /// only thing that releases the store clone its sync actor captured.
    joined: Option<JoinedNine>,
    /// This place's Moot id, so a visitor's dial can name the same network the
    /// holder's policy does without carrying the whole binding.
    moot: [u8; 32],
    /// The vault host, when this profile has a vault to serve. Held here
    /// because its accept loop borrows the transport and must stop first.
    projection: Option<ProjectionServing>,
    /// Shared because the projection accept loop outlives any one call and
    /// `accept_one` borrows its transport; `drop` aborts that loop first.
    _transport: Arc<P2pandaTransport>,
    /// Taken in `drop` so the runtime can be shut down with a timeout: every
    /// task holding a store clone must be gone before the caller reopens.
    _runtime: Option<tokio::runtime::Runtime>,
}

/// The nine joined lanes, held together so `drop` can move them out.
struct JoinedNine {
    moot: MootLanes,
    graph: JoinedSpace<CommonsExt>,
    chat: JoinedSpace<ChatExt>,
}

impl JoinedNine {
    /// Leave every lane and wait for its drain and sync actor to let go.
    ///
    /// Concurrently and under a bound: each lane's LogSync shutdown takes
    /// seconds on its own, and the caller is a person who clicked reconnect.
    async fn leave_and_wait(self) {
        let moot = self.moot;
        let left = [
            tokio::spawn(moot.constitution.leave_and_wait()),
            tokio::spawn(moot.delegation.leave_and_wait()),
            tokio::spawn(moot.membership.leave_and_wait()),
            tokio::spawn(moot.records.leave_and_wait()),
            tokio::spawn(moot.standing.leave_and_wait()),
            tokio::spawn(moot.tulpa.leave_and_wait()),
            tokio::spawn(moot.flora.leave_and_wait()),
        ];
        let graph = tokio::spawn(self.graph.leave_and_wait());
        let chat = tokio::spawn(self.chat.leave_and_wait());
        let all = async move {
            for lane in left {
                if let Ok(Err(error)) = lane.await {
                    tracing::warn!(%error, "place lane did not leave cleanly");
                }
            }
            let _ = graph.await;
            let _ = chat.await;
        };
        if tokio::time::timeout(LEAVE_BUDGET, all).await.is_err() {
            tracing::warn!("place lanes did not all leave within the budget");
        }
    }
}

impl Drop for LiveLanes {
    fn drop(&mut self) {
        // Explicit because the watcher outlives nothing else usefully: it
        // holds an Emitter, and a tick landing after the place closed would
        // ask the app to resync a place that is gone.
        let runtime = self._runtime.take().expect("lane runtime is taken once");
        // Before the transport closes: the accept loop holds a clone of it,
        // and a session already served releases its own slot when its stream
        // ends, exactly as it would have anyway.
        if let Some(accept) = self.projection.as_mut().and_then(ProjectionServing::abort) {
            runtime.block_on(async {
                let _ = accept.await;
            });
        }
        self.projection = None;
        if let Some(watcher) = self.watcher.take() {
            watcher.abort();
            runtime.block_on(async {
                let _ = watcher.await;
            });
        }
        self.joined = None;
        // The transport owns an iroh endpoint whose Drop path aborts
        // ungracefully. The place worker is synchronous, but retains the
        // lane runtime precisely so this owner can await endpoint shutdown
        // before the runtime and transport are dropped.
        if let Err(error) = runtime.block_on(self._transport.close()) {
            tracing::warn!(%error, "place transport did not close cleanly");
        }
        // Dropping the runtime only detaches its tasks; each still holds a
        // store clone, and redb's file lock with it. Shut down and wait.
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }
}

/// Shared counter handles across all nine lanes plus the projection host,
/// sampled by the watcher.
struct LaneCounters {
    handles: Vec<std::sync::Arc<std::sync::Mutex<stickleback::SyncStatus>>>,
    /// None when this profile has no vault, so there is no host to count.
    projection: Option<ProjectionWatchCounters>,
}

impl LaneCounters {
    /// Accepted operations across every lane, plus the projection host's own
    /// session counters. A sum is right HERE and wrong in the status surface:
    /// the watcher only needs to know that SOMETHING moved, while a person
    /// needs to know which lane it arrived on and how many peers are being
    /// served. Folding the projection in is what makes a visitor arriving or
    /// leaving nudge the app exactly like a lane arrival does.
    fn total(&self) -> u64 {
        let lanes: u64 = self
            .handles
            .iter()
            .map(|handle| handle.lock().map(|status| status.ops_received).unwrap_or(0))
            .sum();
        lanes
            + self
                .projection
                .as_ref()
                .map(ProjectionWatchCounters::total)
                .unwrap_or(0)
    }
}

impl LiveLanes {
    /// Leave every lane and wait for its sync actor to release the store
    /// clone it captured, before this value is dropped.
    ///
    /// Only a caller about to REOPEN these stores in the same process needs
    /// this: it costs seconds, and an ordinary close does not care who still
    /// holds a handle. Nothing but drop may follow it.
    pub(crate) fn leave_and_wait(&mut self) {
        if let (Some(joined), Some(runtime)) = (self.joined.take(), self._runtime.as_ref()) {
            runtime.block_on(joined.leave_and_wait());
        }
    }

    /// Stop like a killed process: every task dropped at once, no goodbye on
    /// the wire, and nothing released.
    ///
    /// The ordinary `Drop` aborts the accept loop, closes the endpoint and
    /// waits, which is exactly what a process that dies does NOT do — so a test
    /// about abrupt loss cannot use it. The leak is the point: the socket stays
    /// open and silent, which is the worst case a visitor can be handed.
    #[cfg(test)]
    pub(crate) fn abandon(mut self) {
        if let Some(runtime) = self._runtime.take() {
            runtime.shutdown_background();
        }
        std::mem::forget(self);
    }

    /// Stop answering while the socket stays open, for `hold`.
    ///
    /// The other half of "the holder went away": a process that is alive and
    /// reachable but no longer running — a frozen machine, a suspended laptop,
    /// a partitioned link. Nothing reaches the wire at all, so no transport
    /// error is coming; only a deadline of the visitor's own ends the wait.
    /// Every worker thread of the lane runtime is parked, which is what the
    /// endpoint's driver runs on.
    #[cfg(test)]
    pub(crate) fn freeze(&self, hold: std::time::Duration) {
        let handle = self._runtime.as_ref().expect("lane runtime is present").handle();
        for _ in 0..4 {
            handle.spawn(async move {
                std::thread::sleep(hold);
            });
        }
    }

    fn joined(&self) -> &JoinedNine {
        self.joined.as_ref().expect("lanes are taken only in drop")
    }

    fn counter_handles(&self) -> LaneCounters {
        let joined = self.joined();
        let mut handles = joined.moot.status_handles().to_vec();
        handles.push(joined.graph.status_handle());
        handles.push(joined.chat.status_handle());
        LaneCounters {
            handles,
            projection: self.projection.as_ref().map(ProjectionServing::watch_counters),
        }
    }

    /// Per-lane accepted-operation counters, Gemot's seven then graph then chat.
    /// These counters do not establish whether a lane is caught up.
    pub(crate) fn ops_received(&self) -> [u64; 9] {
        let joined = self.joined();
        let gemot = joined.moot.sync_status();
        [
            gemot[0].ops_received,
            gemot[1].ops_received,
            gemot[2].ops_received,
            gemot[3].ops_received,
            gemot[4].ops_received,
            gemot[5].ops_received,
            gemot[6].ops_received,
            joined.graph.sync_status().ops_received,
            joined.chat.sync_status().ops_received,
        ]
    }

    /// What this bind is serving by projection, or `None` when it serves
    /// nothing because this profile has no Knot vault.
    pub(crate) fn projection_snapshot(&self) -> Option<crate::place::PlaceProjectionSnapshot> {
        self.projection.as_ref().map(ProjectionServing::snapshot)
    }

    /// This bind's own tickets, and how many peers it dialed. Neither says a
    /// peer is reachable; the first says where this one can be reached.
    pub(crate) fn rendezvous(&self) -> (Vec<String>, usize) {
        (self.local_rendezvous.clone(), self.dialed_rendezvous)
    }

    pub(crate) fn sync_snapshot(&self) -> Vec<crate::place::PlaceLaneSnapshot> {
        const NAMES: [&str; 9] = [
            "gemot/constitution/v1",
            "gemot/delegation/v1",
            "gemot/membership/v1",
            "gemot/records/v1",
            "gemot/standing/v1",
            "gemot/tulpa/v1",
            "gemot/flora/v1",
            "commons/graph/v1",
            "commons/chat/v1",
        ];
        let joined = self.joined();
        let statuses = joined
            .moot
            .sync_status()
            .into_iter()
            .chain([joined.graph.sync_status(), joined.chat.sync_status()]);
        NAMES
            .into_iter()
            .zip(statuses)
            .map(|(name, status)| crate::place::PlaceLaneSnapshot {
                name,
                syncing: status.syncing,
                sync_rounds: status.sync_rounds,
                ops_received: status.ops_received,
                last_activity_ms: status.last_activity_ms,
            })
            .collect()
    }

    /// Push one freshly authored graph operation onto the live lane.
    ///
    /// Initial sync covers retained history for a late joiner; an operation
    /// authored while live reaches connected peers only through this. Storing
    /// it is what makes it survive, publishing is what makes it arrive.
    pub(crate) fn publish_graph(
        &self,
        operation: stickleback::Operation<CommonsExt>,
    ) -> Result<(), String> {
        self.joined()
            .graph
            .publish(operation)
            .map_err(|error| format!("publish graph operation: {error}"))
    }

    /// Push one freshly authored chat operation onto the live lane.
    pub(crate) fn publish_chat(
        &self,
        operation: stickleback::Operation<ChatExt>,
    ) -> Result<(), String> {
        self.joined()
            .chat
            .publish(operation)
            .map_err(|error| format!("publish chat operation: {error}"))
    }

    /// Push one freshly authored membership operation onto the live lane, so
    /// an admission reaches peers already connected rather than waiting for
    /// whatever restarts reconciliation.
    pub(crate) fn publish_membership(
        &self,
        operation: stickleback::Operation<gemot::moot::MootGroupExt>,
    ) -> Result<(), String> {
        self.joined()
            .moot
            .publish_membership(operation)
            .map_err(|error| format!("publish membership operation: {error}"))
    }

    /// Refuse withdrawn projection grants at this bind's door. A bind that
    /// serves no vault has no door, so there is nothing to refuse.
    pub(crate) fn withdraw_projection_grants(
        &self,
        statements: &[identity::delegation::SignedDelegationRevocation],
    ) -> Result<(), String> {
        let Some(serving) = &self.projection else {
            return Ok(());
        };
        statements
            .iter()
            .try_for_each(|statement| serving.withdraw(statement))
    }

    /// Push one freshly authored delegation operation onto the live lane.
    pub(crate) fn publish_delegation(
        &self,
        operation: stickleback::Operation<gemot::moot::delegation::MootDelegationExt>,
    ) -> Result<(), String> {
        self.joined()
            .moot
            .publish_delegation(operation)
            .map_err(|error| format!("publish delegation operation: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use chartulary::{Author, Container};
    use commons::chat::{Channel, ChatEvent, Message};
    use gemot::moot::{
        MootAccessLevel, MootFile, MootId, MootMembershipAction, MootOutboundOperation,
    };
    use identity::{IdentityProvider, InMemoryProvider};
    use muniment::RedbBackend;
    use stickleback::DataKeyring;
    use transport::P2pandaTransport;

    use graphshell::client::{ResolvedContent, RetainedEndpointSession};
    use transport::Transport;

    use crate::action::Update;
    use crate::identity::RootIdentity;
    use crate::panes::SessionId;
    use crate::place::invite::{P2PANDA_ENDPOINT_TICKET, RendezvousV1};
    use crate::place::worker::tests::{
        binding, found_place_for_authoring, founder_signing_key, place_delegation, place_rules,
        seed_exact_collection, seed_exact_collection_captures, settings,
    };
    use crate::place::worker::{
        PlaceCommand, PlaceWorkerCommand, author_invitation, found_place_group, load_group_session,
        open_cached_place, place_store_dir, prepare_group_identity, spawn_place_worker,
    };
    use crate::place::{
        CapturedCollectionSelection, CapturedCollectionSelectionStatus, PlaceBindingV1,
    };
    use commons::{Replica, chat::ChatReplica};

    /// Issue a capability delegation on the host's retained delegation lane.
    fn delegate_to(host: &Path, founder: &InMemoryProvider, moot: [u8; 32], subject: [u8; 32]) {
        let rules = place_rules(founder.master_public_key().to_bytes(), moot);
        let moot_file = pollster::block_on(gemot::moot::MootFile::open_existing(
            place_store_dir(host).join("gemot"),
            gemot::moot::MootId(moot),
            settings().retention,
        ))
        .unwrap();
        pollster::block_on(moot_file.delegation_store().author_issue(
            &founder_signing_key(founder, moot),
            &rules,
            place_delegation(founder, moot, subject),
        ))
        .unwrap();
    }

    /// Author retained graph and chat content on the host as the founder.
    fn author_host_content(host: &Path, founder: &InMemoryProvider, b: &PlaceBindingV1) {
        let moot = b.moot.0;
        let stores = place_store_dir(host);
        // The founder's own writes must be Effective on the joiner, and a root
        // grant alone covers nothing: MootDelegations::covers walks
        // certificates only, so the founder delegates to itself.
        let rules = place_rules(founder.master_public_key().to_bytes(), moot);
        let moot_file = pollster::block_on(gemot::moot::MootFile::open_existing(
            stores.join("gemot"),
            gemot::moot::MootId(moot),
            settings().retention,
        ))
        .unwrap();
        pollster::block_on(moot_file.delegation_store().author_issue(
            &founder_signing_key(founder, moot),
            &rules,
            place_delegation(founder, moot, founder.master_public_key().to_bytes()),
        ))
        .unwrap();
        drop(moot_file);

        let group = load_group_session(host, founder, moot).unwrap();
        let keyring = DataKeyring::from_bytes(&group.data_keyring_state().unwrap()).unwrap();

        let graph_backend = RedbBackend::open(stores.join("commons-graph.redb")).unwrap();
        let mut graph = Replica::for_identity(graph_backend, b.root.0, founder).unwrap();
        for index in 0..2 {
            // Address as identity, matching what ShareNode authors: the
            // fixture must produce what the product path produces, or the
            // projection is proved against something nobody writes.
            pollster::block_on(graph.edit(move |log| {
                let address = format!("shared-{index}");
                log.insert_node(
                    &Author::new("turnstone"),
                    Container::new(address.clone()).with_address(address),
                );
            }))
            .unwrap();
        }
        drop(graph);

        let chat_backend = RedbBackend::open(stores.join("commons-chat.redb")).unwrap();
        let mut chat = ChatReplica::for_identity(chat_backend, b.chat.0, founder, keyring).unwrap();
        pollster::block_on(chat.author(ChatEvent::Channel(Channel {
            id: "hall".into(),
            title: "Hall".into(),
        })))
        .unwrap();
        for index in 0..2 {
            pollster::block_on(chat.author(ChatEvent::Message(Message {
                channel: "hall".into(),
                body: format!("retained {index}"),
                sent_at_ms: index as u64,
                reply_to: None,
            })))
            .unwrap();
        }
    }



    /// The founder path through the worker: `Found` opens a place nobody was
    /// invited to, binds listen-only, and reports its own rendezvous without
    /// ever reading as connected.
    #[test]
    fn a_founder_binds_listen_only_and_reports_its_own_rendezvous() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-founder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        std::fs::create_dir_all(&host).unwrap();
        let founder = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xf1; 32]));

        let wake: armillary::Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(
            wake,
            Arc::new(founder),
            crate::place::worker::PlaceWorkerSettings::default(),
        );
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Found {
            session,
            generation: 1,
            directory: host.clone(),
            name: "Hearth".into(),
        });
        let binding = match updates.recv_timeout(Duration::from_secs(60)) {
            Ok(Update::PlaceFounded {
                result: Ok((binding, snapshot)),
                ..
            }) => {
                let sync = snapshot.sync.clone().expect("a founded place binds lanes");
                assert_eq!(sync.lanes.len(), 9);
                assert_eq!(sync.dialed_rendezvous, 0, "a founder dials nobody");
                assert!(
                    !sync.local_rendezvous.is_empty(),
                    "the founder's own ticket is what its first invitation carries"
                );
                let state = crate::place::PlaceState::Offline {
                    binding: binding.clone(),
                    generation: 1,
                    snapshot,
                };
                let status = state.status_lines();
                assert!(
                    status
                        .iter()
                        .any(|line| line == "Listening for peers: none dialed"),
                    "a founder with no peers never reads as connected: {status:?}"
                );
                assert!(
                    status
                        .iter()
                        .any(|line| line.starts_with("Local rendezvous: ")),
                    "{status:?}"
                );
                binding
            },
            Ok(Update::PlaceFounded {
                result: Err(error), ..
            }) => panic!("founding refused: {error}"),
            _ => panic!("founding answered with an unrelated update"),
        };
        assert!(
            host.join(crate::place::rendezvous::RENDEZVOUS_FILE).exists(),
            "founding leaves the empty descriptor a reconnect reopens"
        );
        assert_eq!(
            crate::session::load_place_binding(&host).unwrap().unwrap(),
            binding
        );

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }


    /// A founder restarts: the descriptor it saved has no ticket in it, so the
    /// reopen binds listen-only instead of refusing. This is the same
    /// `Reconnect` a joiner takes, which is the point — one path, two roles.
    #[test]
    fn a_founder_reconnects_through_an_empty_descriptor() {
        let root = std::env::temp_dir()
            .join(format!("turnstone-place-refound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        std::fs::create_dir_all(&host).unwrap();
        let founder = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xf2; 32]));
        let settings = crate::place::worker::PlaceWorkerSettings::default();

        // Exactly what the `Found` command leaves behind on disk, without the
        // live bind: this test is about what a LATER process finds there.
        let binding =
            crate::place::worker::found_place(&host, &founder, "Hearth", &settings).unwrap();
        crate::session::save_place_binding(&host, &binding).unwrap();
        crate::place::rendezvous::save_founder_rendezvous(&host, &binding).unwrap();

        let (worker, updates) =
            spawn_place_worker(Arc::new(|| {}), Arc::new(founder), settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 1,
            directory: host.clone(),
            binding: binding.clone(),
        });
        match updates.recv_timeout(Duration::from_secs(60)) {
            Ok(Update::PlaceOpened {
                result: Ok(snapshot),
                generation: 1,
                ..
            }) => {
                let sync = snapshot.sync.expect("a founder reconnect binds lanes");
                assert_eq!(sync.lanes.len(), 9);
                assert_eq!(sync.dialed_rendezvous, 0, "there was nothing to dial");
                assert!(!sync.local_rendezvous.is_empty());
            },
            Ok(Update::PlaceOpened {
                result: Err(error), ..
            }) => panic!("founder reconnect refused: {error}"),
            _ => panic!("reconnect answered with an unrelated update"),
        }

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The T3a lane-join receipt: a joiner admits over a ticket and catches up
    /// on the founder's retained place, render-free, through the worker's own
    /// Join and Resync commands. Seven lanes per side, one endpoint each.
    #[test]
    fn a_joiner_catches_up_on_a_live_place_over_one_ticket() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let founder = InMemoryProvider::from_seed([0xd1; 32]);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xd2; 32]));
        let b = binding(0x7a);

        // Retained state exists before anything dials.
        let joiner_root = joiner.master_public_key().to_bytes();
        found_place_for_authoring(&host, &b, &founder, joiner_root);
        found_place_group(&host, &founder, b.moot.0).unwrap();
        author_host_content(&host, &founder, &b);
        // The joiner is a member AND delegated, so it can speak once the
        // delegation lane carries the certificate to it. Membership alone
        // would leave it able to read and not to write.
        delegate_to(&host, &founder, b.moot.0, joiner_root);
        let joiner_prekey = prepare_group_identity(&guest, &joiner, b.moot.0).unwrap();

        // The host binds its transport first, because the ticket in the
        // invitation IS this endpoint.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let host_transport = runtime
            .block_on(async {
                P2pandaTransport::builder(&founder.master_keypair())
                    .gossip()
                    .bind()
                    .await
            })
            .unwrap();
        let ticket = runtime.block_on(host_transport.ticket()).unwrap();

        let invite = author_invitation(
            &host,
            &b,
            &founder,
            &joiner_prekey,
            u64::MAX,
            vec![RendezvousV1 {
                carrier: P2PANDA_ENDPOINT_TICKET.into(),
                hint: ticket,
            }],
            None,
            &settings(),
        )
        .unwrap();

        // Host side goes live: the same open path the worker uses, lanes held
        // for the duration.
        let (host_open, _) = open_cached_place(&host, &b, &founder, &settings()).unwrap();
        let (endpoint, gossip) = host_transport.sync_parts().unwrap();
        let _host_lanes = runtime
            .block_on(async {
                let moot = host_open
                    .moot
                    .join_lanes(endpoint.clone(), gossip.clone())
                    .await?;
                let graph = host_open
                    .graph
                    .join(endpoint.clone(), gossip.clone())
                    .await?;
                let chat = host_open.chat.join(endpoint, gossip).await?;
                Ok::<_, stickleback::JoinError>((moot, graph, chat))
            })
            .unwrap();

        // Guest side is the product path end to end.
        let wake: armillary::Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, Arc::new(joiner), settings());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: guest.clone(),
            invite: Box::new(invite),
        });
        let joined = updates
            .recv_timeout(Duration::from_secs(60))
            .expect("join answers");
        match joined {
            Update::PlaceJoined { result: Ok(_), .. } => {}
            Update::PlaceJoined {
                result: Err(error), ..
            } => panic!("join refused: {error}"),
            _ => panic!("join answered with an unrelated update"),
        }
        assert!(
            guest.join(crate::place::rendezvous::RENDEZVOUS_FILE).exists(),
            "successful Join persists bounded reconnect contact metadata"
        );

        // Catch-up WITHOUT polling Resync: the lane watcher nudges, the app
        // answers by resyncing, and the joiner converges on its own. This
        // stands in for the app's fold, which turns PlaceLanesAdvanced into an
        // Effect::ResyncPlace.
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut last = None;
        let mut nudges = 0;
        loop {
            assert!(
                Instant::now() < deadline,
                "did not converge after {nudges} lane nudges; last: {last:?}"
            );
            match updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::PlaceLanesAdvanced { generation: 1, .. }) => {
                    nudges += 1;
                    worker.command(PlaceWorkerCommand::Resync {
                        session,
                        generation: 1,
                    });
                }
                Ok(Update::PlaceOpened {
                    result: Ok(snapshot),
                    ..
                }) => {
                    // Content AND authority: the delegation lane must have
                    // carried the joiner's certificate too, or it converges on
                    // a place it can read and cannot speak in.
                    let done = snapshot.graph.nodes == 2
                        && snapshot.chat.messages == 2
                        && snapshot.chat.channels == 1
                        && snapshot.moot.members == 2
                        && snapshot.moot.delegated_certificates == 2;
                    last = Some(snapshot);
                    if done {
                        break;
                    }
                }
                _ => continue,
            }
        }
        assert!(
            nudges > 0,
            "the joiner converged without the watcher reporting anything"
        );

        // The joiner speaks, and its own projection carries the message back
        // immediately: authoring stores locally, publishing is what makes it
        // travel. Both halves ride the one command.
        worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request: 1,
            command: PlaceCommand::SendMessage {
                channel: "hall".into(),
                body: "from the joiner".into(),
            },
        });
        let authored = updates
            .recv_timeout(Duration::from_secs(30))
            .expect("author answers");
        match authored {
            Update::PlaceCommandDone {
                request: 1,
                result: Ok(snapshot),
                ..
            } => assert_eq!(
                snapshot.chat.messages, 3,
                "the author's own message projects at once"
            ),
            Update::PlaceCommandDone {
                result: Err(error), ..
            } => panic!("author refused: {error}"),
            _ => panic!("author answered with an unrelated update"),
        }

        // And it reaches the host over the live lane, which is the half
        // initial sync does not cover.
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            assert!(
                Instant::now() < deadline,
                "the authored message never reached the host"
            );
            std::thread::sleep(Duration::from_millis(400));
            let projection = pollster::block_on(host_open.chat.projection()).unwrap();
            if projection
                .messages
                .iter()
                .any(|message| message.message.body == "from the joiner")
            {
                break;
            }
        }

        // The shared graph reaches the app as nodes, not counts. Both of the
        // host's shared addresses are named, in deterministic order.
        let shared = last.expect("converged snapshot").shared;
        assert_eq!(shared.nodes.len(), 2);
        let addresses: Vec<&str> = shared.addresses().collect();
        assert_eq!(addresses, vec!["shared-0", "shared-1"]);

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(_host_lanes);
        drop(host_open);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Wait for one authored command's answer, stepping past the lane nudges
    /// that arrive on the same channel. A bare `recv` races the watcher.
    fn expect_authored(updates: &std::sync::mpsc::Receiver<Update>, request: u64) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::PlaceCommandDone {
                    request: got,
                    result: Ok(_),
                    ..
                }) if got == request => return,
                Ok(Update::PlaceCommandDone {
                    result: Err(error), ..
                }) => panic!("request {request} refused: {error}"),
                _ => continue,
            }
        }
        panic!("request {request} never answered");
    }

    /// A selected exact collection remains selected after a remote fauna
    /// withdrawal. The changed record must enter through the real objects
    /// lane before the receiver re-folds; no local reopening can substitute
    /// for that ingress.
    #[test]
    fn a_live_share_withdrawal_empties_the_receiver_selected_collection() {
        let root = tempfile::tempdir().unwrap();
        let host = root.path().join("host");
        let guest = root.path().join("guest");
        let b = binding(0x9c);
        let founder_seed = [b.moot.0[0].wrapping_add(40); 32];
        let founder = InMemoryProvider::from_seed(founder_seed);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0x9d; 32]));

        // Both profiles retain the same captured text up front. That lets the
        // receiver prove the selection filter, rather than merely showing
        // that a peer without the local document has no search result.
        let (host_settings, requested, selected_share) = seed_exact_collection(
            &host,
            &RootIdentity::Unsealed(InMemoryProvider::from_seed(founder_seed)),
            &b,
        );
        let (guest_settings, _, _) = seed_exact_collection_captures(&guest);

        // Invitation and group welcome require governed membership. The exact
        // collection fixture already founded the Moot, so add only the new
        // peer rather than founding a competing profile fixture.
        let host_moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(&host).join("gemot"),
            MootId(b.moot.0),
            host_settings.retention.clone(),
        ))
        .unwrap();
        pollster::block_on(host_moot.membership_store().author_for_identity(
            &founder,
            MootMembershipAction::Add {
                member: joiner.master_public_key().to_bytes(),
                access: MootAccessLevel::Write,
            },
        ))
        .unwrap();
        drop(host_moot);
        let joiner_prekey = prepare_group_identity(&guest, &joiner, b.moot.0).unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let host_transport = runtime
            .block_on(async {
                P2pandaTransport::builder(&founder.master_keypair())
                    .gossip()
                    .bind()
                    .await
            })
            .unwrap();
        let ticket = runtime.block_on(host_transport.ticket()).unwrap();
        let invite = author_invitation(
            &host,
            &b,
            &founder,
            &joiner_prekey,
            u64::MAX,
            vec![RendezvousV1 {
                carrier: P2PANDA_ENDPOINT_TICKET.into(),
                hint: ticket,
            }],
            None,
            &host_settings,
        )
        .unwrap();

        let (host_open, _) = open_cached_place(&host, &b, &founder, &host_settings).unwrap();
        let (endpoint, gossip) = host_transport.sync_parts().unwrap();
        let host_lanes = runtime
            .block_on(host_open.moot.join_lanes(endpoint, gossip))
            .unwrap();

        let wake: armillary::Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, Arc::new(joiner), guest_settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: guest.clone(),
            invite: Box::new(invite),
        });
        assert!(matches!(
            updates.recv_timeout(Duration::from_secs(60)),
            Ok(Update::PlaceJoined { result: Ok(_), .. })
        ));
        converge_until(&worker, &updates, session, "receive host collection", |snapshot| {
            snapshot
                .collection_choices
                .iter()
                .any(|choice| choice.version == requested)
        });

        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: Some(requested.clone()),
        });
        let selection_deadline = Instant::now() + Duration::from_secs(30);
        let mut selection_last = None;
        let selected = loop {
            assert!(
                Instant::now() < selection_deadline,
                "receiver did not select the exact collection version; last update: {selection_last:?}"
            );
            match updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::PlaceCollectionSet {
                    session: got,
                    generation: 1,
                    result: Ok(snapshot),
                }) if got == session => break snapshot,
                Ok(Update::PlaceCollectionSet {
                    result: Err(error), ..
                }) => panic!("receiver exact collection selection refused: {error}"),
                Ok(update) => {
                    selection_last = Some(format!(
                        "unrelated update: {:?}",
                        std::mem::discriminant(&update)
                    ))
                },
                Err(error) => selection_last = Some(format!("receive selection update: {error}")),
            }
        };
        assert_eq!(selected.captured.groups.len(), 1);
        assert!(matches!(
            selected.captured_selection,
            CapturedCollectionSelection::Collection {
                requested: ref retained,
                status: CapturedCollectionSelectionStatus::Ready {
                    effective_contributions: 1,
                    pending_facts: 0,
                    ..
                },
            } if retained == &requested
        ));

        // This is the host's normal command split: author into the retained
        // Moot, recover the signed operation, then publish it on its existing
        // objects handle. A received lane nudge is required before resync.
        let receipt = runtime
            .block_on(
                host_open
                    .moot
                    .withdraw_share_for_identity(&founder, selected_share, 30),
            )
            .unwrap();
        let MootOutboundOperation::Object(withdrawal) =
            runtime.block_on(host_open.moot.outbound(&receipt)).unwrap()
        else {
            panic!("share withdrawal did not recover an objects-lane operation");
        };
        host_lanes.records.publish(withdrawal).unwrap();

        let deadline = Instant::now() + Duration::from_secs(45);
        let mut saw_ingress_nudge = false;
        let mut last = None;
        let resynced = loop {
            assert!(
                Instant::now() < deadline,
                "withdrawn selected collection never converged; ingress nudge: {saw_ingress_nudge}; last update: {last:?}"
            );
            match updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::PlaceLanesAdvanced {
                    session: got,
                    generation: 1,
                }) if got == session => {
                    saw_ingress_nudge = true;
                    worker.command(PlaceWorkerCommand::Resync {
                        session,
                        generation: 1,
                    });
                },
                Ok(Update::PlaceOpened {
                    session: got,
                    generation: 1,
                    result: Ok(snapshot),
                }) if got == session => {
                    let withdrawn = matches!(
                        &snapshot.captured_selection,
                        CapturedCollectionSelection::Collection {
                            requested: retained,
                            status: CapturedCollectionSelectionStatus::Ready {
                                effective_contributions: 0,
                                pending_facts: 1,
                                ..
                            },
                        } if retained == &requested
                    ) && snapshot.captured.groups.is_empty()
                        && snapshot.captured.rejected.is_empty();
                    if saw_ingress_nudge && withdrawn {
                        break snapshot;
                    }
                    last = Some(format!("snapshot: {snapshot:?}"));
                },
                Ok(Update::PlaceOpened {
                    result: Err(error), ..
                }) => {
                    panic!("receiver resync refused after objects ingress: {error}")
                },
                Ok(update) => {
                    last = Some(format!(
                        "unrelated update: {:?}",
                        std::mem::discriminant(&update)
                    ))
                },
                Err(error) => last = Some(format!("receive resync update: {error}")),
            }
        };

        assert!(resynced.captured.groups.is_empty());
        assert!(resynced.captured.rejected.is_empty());
        assert!(matches!(
            resynced.captured_selection,
            CapturedCollectionSelection::Collection {
                requested: ref retained,
                status: CapturedCollectionSelectionStatus::Ready {
                    effective_contributions: 0,
                    pending_facts: 1,
                    ..
                },
            } if retained == &requested
        ));

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(host_lanes);
        drop(host_open);
    }

    /// Drive the worker until its snapshot satisfies `done`, answering lane
    /// nudges with resyncs. Returns the satisfying snapshot, or says how far
    /// it got.
    fn converge_until(
        worker: &armillary::ActorHandle<PlaceWorkerCommand>,
        updates: &std::sync::mpsc::Receiver<Update>,
        session: SessionId,
        what: &str,
        done: impl Fn(&crate::place::OfflinePlaceSnapshot) -> bool,
    ) -> crate::place::OfflinePlaceSnapshot {
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut last = None;
        // Ask once up front: what is already retained may satisfy `done`
        // without any lane traffic at all.
        worker.command(PlaceWorkerCommand::Resync {
            session,
            generation: 1,
        });
        loop {
            assert!(
                Instant::now() < deadline,
                "{what}: never happened; {last:?}"
            );
            match updates.recv_timeout(Duration::from_secs(10)) {
                Ok(Update::PlaceLanesAdvanced { .. }) => {
                    worker.command(PlaceWorkerCommand::Resync {
                        session,
                        generation: 1,
                    });
                }
                Ok(Update::PlaceOpened {
                    result: Ok(snapshot),
                    ..
                })
                | Ok(Update::PlaceCommandDone {
                    result: Ok(snapshot),
                    ..
                }) => {
                    if done(&snapshot) {
                        return snapshot;
                    }
                    last = Some(snapshot);
                    // No nudge is coming if the lanes are quiet, so keep
                    // asking rather than blocking on traffic that may not
                    // arrive.
                    std::thread::sleep(Duration::from_millis(300));
                    worker.command(PlaceWorkerCommand::Resync {
                        session,
                        generation: 1,
                    });
                }
                Ok(Update::PlaceCommandDone {
                    result: Err(error), ..
                }) => panic!("{what}: command refused: {error}"),
                _ => continue,
            }
        }
    }

    /// T3c's convergence half: a shared node crosses, a partition stops
    /// traffic without losing anything, and reconnecting converges both sides.
    #[test]
    fn a_partition_heals_and_both_sides_converge() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
        let root =
            std::env::temp_dir().join(format!("turnstone-place-partition-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let founder = InMemoryProvider::from_seed([0xf1; 32]);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xf2; 32]));
        let b = binding(0x7e);
        let joiner_root = joiner.master_public_key().to_bytes();

        found_place_for_authoring(&host, &b, &founder, joiner_root);
        found_place_group(&host, &founder, b.moot.0).unwrap();
        author_host_content(&host, &founder, &b);
        delegate_to(&host, &founder, b.moot.0, joiner_root);
        let joiner_prekey = prepare_group_identity(&guest, &joiner, b.moot.0).unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let host_transport = runtime
            .block_on(async {
                P2pandaTransport::builder(&founder.master_keypair())
                    .gossip()
                    .bind()
                    .await
            })
            .unwrap();
        let ticket = runtime.block_on(host_transport.ticket()).unwrap();
        let invite = author_invitation(
            &host,
            &b,
            &founder,
            &joiner_prekey,
            u64::MAX,
            vec![RendezvousV1 {
                carrier: P2PANDA_ENDPOINT_TICKET.into(),
                hint: ticket,
            }],
            None,
            &settings(),
        )
        .unwrap();

        let (host_open, _) = open_cached_place(&host, &b, &founder, &settings()).unwrap();
        let (endpoint, gossip) = host_transport.sync_parts().unwrap();
        let host_lanes = runtime
            .block_on(async {
                let moot = host_open
                    .moot
                    .join_lanes(endpoint.clone(), gossip.clone())
                    .await?;
                let graph = host_open
                    .graph
                    .join(endpoint.clone(), gossip.clone())
                    .await?;
                let chat = host_open.chat.join(endpoint, gossip).await?;
                Ok::<_, stickleback::JoinError>((moot, graph, chat))
            })
            .unwrap();

        let wake: armillary::Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, Arc::new(joiner), settings());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: guest.clone(),
            invite: Box::new(invite),
        });
        assert!(matches!(
            updates.recv_timeout(Duration::from_secs(60)),
            Ok(Update::PlaceJoined { result: Ok(_), .. })
        ));
        converge_until(&worker, &updates, session, "initial catch-up", |s| {
            s.graph.nodes == 2 && s.moot.delegated_certificates == 2
        });

        // A shared HTTPS node authored by the joiner reaches the host.
        worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request: 1,
            command: PlaceCommand::ShareNode {
                address: "https://shared.example/page".into(),
            },
        });
        expect_authored(&updates, 1);
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            assert!(Instant::now() < deadline, "the shared node never crossed");
            std::thread::sleep(Duration::from_millis(300));
            let projection = pollster::block_on(host_open.graph.projection()).unwrap();
            if projection
                .graph
                .graph()
                .nodes()
                .any(|(_, c)| c.id == "https://shared.example/page")
            {
                break;
            }
        }

        // Partition: the host leaves its lanes. Nothing is lost on either
        // side, because authoring stores before it publishes.
        drop(host_lanes);
        worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request: 2,
            command: PlaceCommand::SendMessage {
                channel: "hall".into(),
                body: "sent while partitioned".into(),
            },
        });
        expect_authored(&updates, 2);

        // Heal: the host rejoins and catches up on what it missed.
        let (endpoint, gossip) = host_transport.sync_parts().unwrap();
        let rejoined = runtime
            .block_on(async {
                let moot = host_open
                    .moot
                    .join_lanes(endpoint.clone(), gossip.clone())
                    .await?;
                let graph = host_open
                    .graph
                    .join(endpoint.clone(), gossip.clone())
                    .await?;
                let chat = host_open.chat.join(endpoint, gossip).await?;
                Ok::<_, stickleback::JoinError>((moot, graph, chat))
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            assert!(
                Instant::now() < deadline,
                "the partitioned message never arrived after healing; host chat accepted {} operations; status {:?}",
                rejoined.2.ops_received(), rejoined.2.sync_status(),
            );
            std::thread::sleep(Duration::from_millis(400));
            let projection = pollster::block_on(host_open.chat.projection()).unwrap();
            if projection
                .messages
                .iter()
                .any(|m| m.message.body == "sent while partitioned")
            {
                break;
            }
        }

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(rejoined);
        drop(host_open);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// T3c's restart half: what a place converged on survives the worker
    /// dying, and reopens without any network at all.
    ///
    /// It also pins the reconnection gap. `Open` does not dial: only `Join`
    /// carries a rendezvous, and the invitation is not persisted, so a
    /// restarted place comes back **offline** holding everything it had. That
    /// is correct behaviour for what exists and the wrong end state for a
    /// product; the assertion here is what will fail when reconnection lands,
    /// which is the point.
    #[test]
    fn a_restarted_place_keeps_what_it_converged_on() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let founder = InMemoryProvider::from_seed([0xa7; 32]);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xa8; 32]));
        let b = binding(0x8a);
        let joiner_root = joiner.master_public_key().to_bytes();

        found_place_for_authoring(&host, &b, &founder, joiner_root);
        found_place_group(&host, &founder, b.moot.0).unwrap();
        author_host_content(&host, &founder, &b);
        delegate_to(&host, &founder, b.moot.0, joiner_root);
        let joiner_prekey = prepare_group_identity(&guest, &joiner, b.moot.0).unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let host_transport = runtime
            .block_on(async {
                P2pandaTransport::builder(&founder.master_keypair())
                    .gossip()
                    .bind()
                    .await
            })
            .unwrap();
        let ticket = runtime.block_on(host_transport.ticket()).unwrap();
        let invite = author_invitation(
            &host,
            &b,
            &founder,
            &joiner_prekey,
            u64::MAX,
            vec![RendezvousV1 {
                carrier: P2PANDA_ENDPOINT_TICKET.into(),
                hint: ticket,
            }],
            None,
            &settings(),
        )
        .unwrap();

        let (host_open, _) = open_cached_place(&host, &b, &founder, &settings()).unwrap();
        let (endpoint, gossip) = host_transport.sync_parts().unwrap();
        let host_lanes = runtime
            .block_on(async {
                let moot = host_open
                    .moot
                    .join_lanes(endpoint.clone(), gossip.clone())
                    .await?;
                let graph = host_open
                    .graph
                    .join(endpoint.clone(), gossip.clone())
                    .await?;
                let chat = host_open.chat.join(endpoint, gossip).await?;
                Ok::<_, stickleback::JoinError>((moot, graph, chat))
            })
            .unwrap();

        let identity = Arc::new(joiner);
        let wake: armillary::Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake.clone(), Arc::clone(&identity), settings());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: guest.clone(),
            invite: Box::new(invite),
        });
        assert!(matches!(
            updates.recv_timeout(Duration::from_secs(60)),
            Ok(Update::PlaceJoined { result: Ok(_), .. })
        ));
        let converged = converge_until(&worker, &updates, session, "catch-up", |s| {
            s.graph.nodes == 2 && s.chat.messages == 2
        });

        // The worker dies, taking every guest lane and handle with it. Keep
        // the host lane alive so reconnect exercises a real peer.
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(worker);
        drop(updates);

        // A fresh worker first reopens cached state offline, preserving the
        // existing restart receipt.
        let (restarted, restarted_updates) = spawn_place_worker(wake, identity, settings());
        // Retried because a real restart is a new PROCESS: reopening the same
        // redb in-process can briefly race the previous handle's file lock
        // under load. That is an artifact of testing restart without
        // restarting, not a condition the product meets.
        let deadline = Instant::now() + Duration::from_secs(30);
        let reopened = loop {
            restarted.command(PlaceWorkerCommand::Open {
                session,
                generation: 2,
                directory: guest.clone(),
                binding: b.clone(),
            });
            match restarted_updates.recv_timeout(Duration::from_secs(15)) {
                Ok(Update::PlaceOpened {
                    result: Ok(snapshot),
                    ..
                }) => break snapshot,
                Ok(Update::PlaceOpened {
                    result: Err(error), ..
                }) => {
                    assert!(
                        Instant::now() < deadline && error.contains("Cannot acquire lock"),
                        "reopen failed: {error}"
                    );
                    std::thread::sleep(Duration::from_millis(500));
                }
                _ => continue,
            }
        };

        // Everything survived, including the authority that made it visible.
        assert_eq!(reopened.graph.nodes, converged.graph.nodes);
        assert_eq!(reopened.chat.messages, converged.chat.messages);
        assert_eq!(reopened.moot.members, converged.moot.members);
        assert_eq!(
            reopened.moot.delegated_certificates,
            converged.moot.delegated_certificates
        );
        assert_eq!(reopened.shared, converged.shared);
        assert!(
            reopened.group.has_current_epoch,
            "the sealed epoch reopened"
        );
        assert!(reopened.sync.is_none(), "cached Open has no live lane status");

        // Cached reopening remains usable offline, as the original restart
        // receipt requires.
        restarted.command(PlaceWorkerCommand::Author {
            session,
            generation: 2,
            request: 8,
            command: PlaceCommand::SendMessage {
                channel: "hall".into(),
                body: "authored while offline".into(),
            },
        });
        match restarted_updates.recv_timeout(Duration::from_secs(30)) {
            Ok(Update::PlaceCommandDone {
                request: 8,
                result: Ok(snapshot),
                ..
            }) => assert_eq!(snapshot.chat.messages, converged.chat.messages + 1),
            _ => panic!("offline authoring answered unexpectedly"),
        }

        // Reconnect from bounded contact metadata, without replaying the
        // invitation, then author a fact through the live peer.
        restarted.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 3,
            directory: guest.clone(),
            binding: b.clone(),
        });
        let reconnect_deadline = Instant::now() + Duration::from_secs(30);
        let reconnected = loop {
            assert!(Instant::now() < reconnect_deadline, "reconnect answer deadline");
            match restarted_updates.recv_timeout(Duration::from_secs(30)) {
                Ok(Update::PlaceOpened {
                    result: Ok(snapshot),
                    generation: 3,
                    ..
                }) => break snapshot,
                Ok(Update::PlaceOpened {
                    result: Err(error), ..
                }) => panic!("reconnect failed: {error}"),
                Err(error) => panic!("reconnect answer timed out: {error}"),
                _ => continue,
            }
        };
        assert_eq!(reconnected.graph.nodes, reopened.graph.nodes);
        assert_eq!(reconnected.chat.messages, reopened.chat.messages + 1);
        assert_eq!(
            reconnected.sync.as_ref().map(|status| status.lanes.len()),
            Some(9),
            "Reconnect reports all seven Gemot plus graph and chat lanes"
        );
        // Repeating the same lifecycle generation is idempotent and does not
        // tear down or duplicate the already-live lanes.
        restarted.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 3,
            directory: guest.clone(),
            binding: b.clone(),
        });
        let duplicate_deadline = Instant::now() + Duration::from_secs(30);
        let duplicate = loop {
            assert!(Instant::now() < duplicate_deadline, "duplicate reconnect deadline");
            match restarted_updates.recv_timeout(Duration::from_secs(1)) {
            Ok(Update::PlaceOpened {
                generation: 3,
                result: Ok(snapshot),
                ..
            }) => break snapshot,
            Ok(Update::PlaceOpened {
                result: Err(error), ..
            }) => panic!("duplicate reconnect refused: {error}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => panic!("duplicate reconnect channel closed: {error}"),
            _ => continue,
            }
        };
        assert_eq!(duplicate.chat.messages, reconnected.chat.messages);
        restarted.command(PlaceWorkerCommand::Author {
            session,
            generation: 3,
            request: 9,
            command: PlaceCommand::SendMessage {
                channel: "hall".into(),
                body: "authored after reconnect".into(),
            },
        });
        let author_deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(Instant::now() < author_deadline, "author response deadline");
            match restarted_updates.recv_timeout(Duration::from_secs(1)) {
                Ok(Update::PlaceCommandDone {
                    request: 9, result: Ok(snapshot), ..
                }) => {
                    assert_eq!(snapshot.chat.messages, converged.chat.messages + 2);
                    break;
                },
                Ok(Update::PlaceCommandDone {
                    request: 9, result: Err(error), ..
                }) => panic!("reconnected authoring refused: {error}"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(error) => panic!("author response channel closed: {error}"),
                _ => continue,
            }
        }

        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            assert!(Instant::now() < deadline, "reconnected author never reached host");
            std::thread::sleep(Duration::from_millis(300));
            let projection = pollster::block_on(host_open.chat.projection()).unwrap();
            if projection
                .messages
                .iter()
                .any(|message| message.message.body == "authored while offline")
                && projection
                    .messages
                    .iter()
                    .any(|message| message.message.body == "authored after reconnect")
            {
                break;
            }
        }

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        restarted.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(host_lanes);
        drop(host_open);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A profile with no effective capability is refused before it authors.
    /// The refusal is local: an operation nobody would project is not worth
    /// storing, and "you may not" is a better answer than silent filtering.
    #[test]
    fn an_unauthorized_profile_is_refused_before_it_authors() {
        let root = std::env::temp_dir().join(format!(
            "turnstone-place-unauthorized-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let founder = InMemoryProvider::from_seed([0xe1; 32]);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xe2; 32]));
        let b = binding(0x7c);

        // Membership without a delegation: the joiner belongs to the Moot but
        // holds no capability, which is exactly the pending case.
        found_place_for_authoring(&host, &b, &founder, joiner.master_public_key().to_bytes());
        found_place_group(&host, &founder, b.moot.0).unwrap();
        let joiner_prekey = prepare_group_identity(&guest, &joiner, b.moot.0).unwrap();
        let invite = author_invitation(
            &host,
            &b,
            &founder,
            &joiner_prekey,
            u64::MAX,
            Vec::new(),
            None,
            &settings(),
        )
        .unwrap();

        let wake: armillary::Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, Arc::new(joiner), settings());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: guest,
            invite: Box::new(invite),
        });
        assert!(matches!(
            updates.recv_timeout(Duration::from_secs(30)),
            Ok(Update::PlaceJoined { result: Ok(_), .. })
        ));

        worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request: 1,
            command: PlaceCommand::SendMessage {
                channel: "hall".into(),
                body: "unauthorized".into(),
            },
        });
        match updates.recv_timeout(Duration::from_secs(30)) {
            Ok(Update::PlaceCommandDone {
                result: Err(error), ..
            }) => assert!(error.contains("no effective capability"), "{error}"),
            Ok(Update::PlaceCommandDone { result: Ok(_), .. }) => {
                panic!("an unauthorized profile must not author")
            }
            _ => panic!("author answered with an unrelated update"),
        }

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Reconnect-in-place: a live bind closed in a resident process must
    /// leave redb's process-wide lock free for the very next reopen. The
    /// generation bump in the worker's Reconnect path does exactly this, so
    /// any lag here lands the place in Degraded for a person who clicked once.
    #[test]
    fn a_closed_live_bind_releases_the_store_lock_for_a_reopen() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-relock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        std::fs::create_dir_all(&host).unwrap();
        let founder = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xb7; 32]));
        let settings = settings();

        let b = crate::place::worker::found_place(&host, &founder, "Hearth", &settings).unwrap();
        let (open, _) = open_cached_place(&host, &b, &founder, &settings).unwrap();
        let mut lanes = super::join_live(&open, &b, &founder, &[], None, None).unwrap();
        assert_eq!(lanes.ops_received().len(), 9);

        lanes.leave_and_wait();
        drop(lanes);
        drop(open);
        let dropped_at = Instant::now();

        let deadline = dropped_at + Duration::from_secs(5);
        let reopened = loop {
            match open_cached_place(&host, &b, &founder, &settings) {
                Ok(reopened) => break Some(reopened),
                Err(error) => {
                    assert!(
                        error.contains("already open"),
                        "reopen failed for an unrelated reason: {error}"
                    );
                    if Instant::now() >= deadline {
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        };
        let waited = dropped_at.elapsed();
        eprintln!("store lock released after {} ms", waited.as_millis());
        let reopened = reopened.unwrap_or_else(|| {
            panic!("the store lock was still held 5 s after the live bind was dropped")
        });
        drop(reopened);
        // The residual is p2panda's, not ours (see RECONNECT_REOPEN_BUDGET):
        // this holds the lanes to what the worker's retry window can absorb.
        assert!(
            waited < crate::place::worker::RECONNECT_REOPEN_BUDGET,
            "reopen waited {} ms on the store lock",
            waited.as_millis()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One founder, live and serving a vault, plus the envelope a joiner was
    /// admitted with. The founder half of both projection tests.
    struct ServedFounder {
        root: std::path::PathBuf,
        vault: std::path::PathBuf,
        document: std::path::PathBuf,
        worker: armillary::ActorHandle<PlaceWorkerCommand>,
        updates: std::sync::mpsc::Receiver<Update>,
        session: SessionId,
        binding: PlaceBindingV1,
        ticket: String,
        invite: Box<crate::place::invite::PlaceInviteV1>,
        guest: std::path::PathBuf,
    }

    /// Found a place whose worker serves a one-document vault, then admit one
    /// root at `access` through the product `Invite`.
    fn found_and_invite(
        tag: &str,
        seed: u8,
        joiner: &InMemoryProvider,
        access: crate::place::PlaceInviteAccess,
    ) -> ServedFounder {
        found_and_invite_with(tag, seed, joiner, access, settings(), None)
    }

    /// [`found_and_invite`] on a chosen clock, with an optional grant lifetime.
    fn found_and_invite_with(
        tag: &str,
        seed: u8,
        joiner: &InMemoryProvider,
        access: crate::place::PlaceInviteAccess,
        clock: crate::place::worker::PlaceWorkerSettings,
        lifetime_ms: Option<u64>,
    ) -> ServedFounder {
        let root = std::env::temp_dir().join(format!(
            "turnstone-place-projection-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let vault = root.join("vault");
        for directory in [&host, &guest, &vault] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let document = vault.join("field.knot");
        std::fs::write(&document, "# Field\n").unwrap();

        let founder = RootIdentity::Unsealed(InMemoryProvider::from_seed([seed; 32]));
        let host_settings = crate::place::worker::PlaceWorkerSettings {
            knot_root: Some(vault.clone()),
            ..clock
        };
        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), Arc::new(founder), host_settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Found {
            session,
            generation: 1,
            directory: host.clone(),
            name: "Hearth".into(),
        });
        let (binding, ticket) = loop {
            match updates.recv_timeout(Duration::from_secs(60)) {
                Ok(Update::PlaceFounded {
                    result: Ok((binding, snapshot)),
                    ..
                }) => {
                    let sync = snapshot.sync.expect("a founded place binds lanes");
                    let projection = sync
                        .projection
                        .as_ref()
                        .expect("a founder with a vault serves it");
                    assert_eq!(projection.route, "knot");
                    assert_eq!(projection.live_sessions, 0, "nobody has dialed yet");
                    let ticket = sync.local_rendezvous[0].clone();
                    break (binding, ticket);
                },
                Ok(Update::PlaceFounded {
                    result: Err(error), ..
                }) => panic!("founding refused: {error}"),
                Ok(_) => continue,
                Err(error) => panic!("founding never answered: {error}"),
            }
        };

        let prekey = prepare_group_identity(&guest, joiner, binding.moot.0).unwrap();
        worker.command(PlaceWorkerCommand::Invite {
            session,
            generation: 1,
            directory: host.clone(),
            prekey,
            access,
            lifetime_ms,
        });
        let invite = expect_invited(&updates, "the joiner invitation");
        ServedFounder {
            root,
            vault,
            document,
            worker,
            updates,
            session,
            binding,
            ticket,
            invite,
            guest,
        }
    }

    /// The founder's current projection observation, through a product Resync.
    fn founder_projection(founder: &ServedFounder) -> Option<crate::place::PlaceProjectionSnapshot> {
        founder.worker.command(PlaceWorkerCommand::Resync {
            session: founder.session,
            generation: 1,
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(Instant::now() < deadline, "the founder never re-folded");
            match founder.updates.recv_timeout(Duration::from_secs(5)) {
                Ok(Update::PlaceOpened {
                    result: Ok(snapshot),
                    ..
                }) => return snapshot.sync.and_then(|sync| sync.projection),
                Ok(_) => continue,
                Err(error) => panic!("the founder never answered a resync: {error}"),
            }
        }
    }

    /// The joiner's own live bind: admitted by the product path, then bound
    /// exactly as the worker binds it, so the test can hold the lanes.
    fn joiner_lanes(founder: &ServedFounder, joiner: &InMemoryProvider) -> super::LiveLanes {
        crate::place::worker::admit_invitation(&founder.guest, &founder.invite, joiner, &settings())
            .expect("the joiner is admitted");
        let (open, _) =
            open_cached_place(&founder.guest, &founder.binding, joiner, &settings()).unwrap();
        super::join_live(
            &open,
            &founder.binding,
            joiner,
            std::slice::from_ref(&founder.ticket),
            // A joiner holds no vault of its own in this proof, so it serves
            // nothing and its own status says so.
            None,
            None,
        )
        .expect("the joiner binds its lanes")
    }

    fn viewing_profile() -> chirograph::CapabilityProfile {
        chirograph::CapabilityProfile::new([
            chirograph::PresentationCapability::EditableText,
            chirograph::PresentationCapability::PortableCard,
        ])
    }

    fn hex_head(bytes: &[u8]) -> String {
        bytes[..4].iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// T5c: a writer admitted to a place edits the document its founder holds,
    /// over the place's own transport, and the founder's file is the truth.
    ///
    /// Every seam is the product one: the grant is issued by `admit_member`
    /// and carried by the invitation `PlaceWorkerCommand::Invite` authored,
    /// stored by `admit_invitation` beside the rendezvous, spent by
    /// `dial_holder` on a leaf to the joiner's own place-transport key, and
    /// admitted by the host `join_live` started. Nothing is stubbed and no
    /// replica exists anywhere: the save is an intent the holder runs.
    #[test]
    fn a_writer_edits_the_document_its_founder_holds() {
        let joiner = InMemoryProvider::from_seed([0xc2; 32]);
        let founder = found_and_invite(
            "writer",
            0xc1,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
        );
        assert!(
            founder.invite.projection_grant.is_some(),
            "a writer invitation carries the projection grant"
        );
        founder.invite.validate().expect("and still validates");

        let lanes = joiner_lanes(&founder, &joiner);
        assert!(
            lanes.projection_snapshot().is_none(),
            "the joiner holds no vault, so it serves nothing"
        );
        let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
            .expect("admission stored the grant beside the rendezvous");

        let dialed = Instant::now();
        let (carrier, holder) =
            super::dial_holder(&lanes, &founder.ticket, &grant, &joiner, [21; 32])
                .expect("the founder admits this writer");
        let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
            .expect("discover the holder endpoint");
        let session = retained.mount(0).expect("mount the projected vault");
        eprintln!(
            "writer: dialed holder {} and mounted in {} ms",
            hex_head(&holder.to_bytes()),
            dialed.elapsed().as_millis()
        );

        let (target, source, token, action) = retained
            .resolve_all(&session)
            .expect("resolve the projection")
            .into_iter()
            .find_map(|(target, presentation)| match presentation.content {
                ResolvedContent::EditableText(editable)
                    if editable.address.ends_with("field.knot") =>
                {
                    Some((
                        target,
                        editable.source,
                        editable.base_token,
                        presentation.semantics.actions[0].clone(),
                    ))
                },
                _ => None,
            })
            .expect("the holder disclosed its document as editable source");
        assert_eq!(source, "# Field\n", "the writer opened the holder document");

        // The founder is serving exactly one session, and says so.
        let serving = founder_projection(&founder).expect("the founder still serves");
        assert_eq!(serving.live_sessions, 1, "one visitor is being served");
        assert_eq!(serving.refused, 0, "and nobody was turned away");

        let saved = retained
            .invoke(
                &session,
                target,
                &action,
                &chirograph::SaveTextV1 {
                    base_token: token,
                    source: "# Field\n\nThe writer was here.\n".into(),
                },
            )
            .expect("submit the save");
        assert_eq!(
            saved,
            chirograph::IntentResult::Accepted,
            "the holder took the writer edit"
        );
        // The holder still has this session open right up to the close: the
        // save was an intent it ran, not a reconnect.
        assert_eq!(
            founder_projection(&founder)
                .expect("the founder still serves")
                .live_sessions,
            1,
            "the writer session survived its own save"
        );
        // Close is the visitor letting go. On a QUIC stream the holder's
        // serving loop can finish first, which the carrier reports as a lost
        // connection rather than a clean stop; the session is over either way
        // and the slot below is what proves it.
        if let Err(error) = retained.close() {
            eprintln!("writer: close reported {error}");
        }

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let serving = founder_projection(&founder).expect("the founder still serves");
            if serving.live_sessions == 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the served session never released its slot"
            );
            std::thread::sleep(Duration::from_millis(200));
        }

        assert_eq!(
            std::fs::read_to_string(&founder.document).unwrap(),
            "# Field\n\nThe writer was here.\n",
            "the holder own file is what changed; the writer holds no replica"
        );
        assert!(
            founder.vault.join("field.knot").exists(),
            "and it is still the same vault"
        );

        drop(lanes);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        founder.worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// Drain the founder's update stream until nothing has arrived for
    /// `quiet`, so a later nudge can only be the one this test provoked.
    fn settle(founder: &ServedFounder, quiet: Duration) -> usize {
        let mut seen = 0usize;
        while founder.updates.recv_timeout(quiet).is_ok() {
            seen += 1;
        }
        seen
    }

    /// Wait for the founder worker's own watch nudge.
    fn next_nudge(founder: &ServedFounder, budget: Duration) -> Option<Duration> {
        let started = Instant::now();
        let deadline = started + budget;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            match founder.updates.recv_timeout(left) {
                Ok(Update::PlaceLanesAdvanced { .. }) => return Some(started.elapsed()),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    /// The founder's status said "0 live sessions" through a whole visit,
    /// because the app only re-folds a place when the lane watcher nudges it
    /// and the watcher sampled lane operations alone. A visitor arriving is
    /// not a lane operation, so nothing asked.
    ///
    /// Both edges matter: the arrival AND the departure. A counter that only
    /// rises would leave the status claiming a visitor who left.
    #[test]
    fn a_projection_session_nudges_the_founder_the_way_a_lane_arrival_does() {
        let joiner = InMemoryProvider::from_seed([0xf2; 32]);
        let founder = found_and_invite(
            "nudge",
            0xf1,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
        );
        let lanes = joiner_lanes(&founder, &joiner);
        let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
            .expect("admission stored the grant beside the rendezvous");

        // The join itself moves the lanes. Let that settle first, so the nudge
        // measured below cannot be a leftover.
        let drained = settle(&founder, Duration::from_secs(3));
        eprintln!("nudge: {drained} updates drained before the visit");
        assert_eq!(
            founder_projection(&founder)
                .expect("the founder serves its vault")
                .live_sessions,
            0,
            "nobody has dialed yet"
        );
        settle(&founder, Duration::from_millis(500));

        let (carrier, _holder) =
            super::dial_holder(&lanes, &founder.ticket, &grant, &joiner, [31; 32])
                .expect("the founder admits this writer");
        let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
            .expect("discover the holder endpoint");
        retained.mount(0).expect("mount the projected vault");

        // Two watch ticks to see the change and two to confirm it settled,
        // plus room for the scheduler: the budget the watcher itself sets.
        let budget = super::WATCH_TICK * 8;
        let arrived = next_nudge(&founder, budget)
            .expect("an admitted projection session nudges the app like a lane arrival");
        eprintln!("nudge: arrival reported after {} ms", arrived.as_millis());
        let serving = founder_projection(&founder).expect("the founder still serves");
        assert_eq!(serving.live_sessions, 1, "and the snapshot says one visitor");
        assert_eq!(serving.refused, 0, "nobody was turned away");

        settle(&founder, Duration::from_millis(500));
        if let Err(error) = retained.close() {
            eprintln!("nudge: close reported {error}");
        }
        let left = next_nudge(&founder, budget)
            .expect("a session that ENDS nudges too; a counter that only rises lies");
        eprintln!("nudge: departure reported after {} ms", left.as_millis());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let serving = founder_projection(&founder).expect("the founder still serves");
            if serving.live_sessions == 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the served session never released its slot"
            );
            std::thread::sleep(Duration::from_millis(100));
        }

        drop(lanes);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        founder.worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// The holder does not close; it DIES. No goodbye reaches the wire, the
    /// visitor's carrier read stays blocked, and the surface would go on
    /// offering a document nobody is holding until the transport's own idle
    /// timeout expires tens of seconds later.
    ///
    /// `LiveLanes::abandon` is the worst case rather than a convenience: the
    /// socket stays open and silent, so nothing but the hub's own liveness
    /// budget can end the wait.
    #[test]
    fn a_visited_document_reports_unavailable_when_the_holder_dies_without_closing() {
        let joiner = InMemoryProvider::from_seed([0xf4; 32]);
        let founder = found_and_invite(
            "abrupt",
            0xf3,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
        );
        let holder_root = InMemoryProvider::from_seed([0xf3; 32])
            .master_public_key()
            .to_bytes();
        let address = crate::knot_authoring::place_held_address(&holder_root, "field.knot");

        let lanes = joiner_lanes(&founder, &joiner);
        let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
            .expect("admission stored the grant beside the rendezvous");
        let dial = super::holder_dial(&lanes, &founder.ticket, &grant, &joiner, [32; 32])
            .expect("the grant is spent on a leaf to this place-transport key");
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let hub = crate::knot_authoring::visit_holder(dial, wake)
            .expect("the founder admits this writer at the projection door");
        let probe = crate::knot_authoring::HubProbe::open(hub, &address)
            .expect("the visited document resolves by its place-held address");
        assert_eq!(probe.source(), "# Field\n");

        // Killed, not released: the accept loop is not aborted, the endpoint
        // is not closed, and nothing is drained.
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        founder.worker.command(PlaceWorkerCommand::Abandon(ack_tx));
        ack_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the founder's lanes were abandoned");
        let died = Instant::now();

        let bound = crate::knot_authoring::VISIT_LIVENESS_BUDGET + Duration::from_secs(9);
        let reason = probe
            .wait_for_unavailable(bound)
            .unwrap_or_else(|| panic!("no unavailable within {} s", bound.as_secs()));
        let noticed = died.elapsed();
        eprintln!(
            "abrupt: unavailable after {} ms with {reason}",
            noticed.as_millis()
        );
        assert!(
            noticed < Duration::from_secs(15),
            "the product promises under 15 s; took {} ms",
            noticed.as_millis()
        );

        drop(probe);
        drop(lanes);
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// The other half of holder loss, and the one no transport error covers:
    /// the holder is still there, its socket is still open, and it has simply
    /// stopped running. Nothing will ever arrive, so the only thing that can
    /// end the visitor's wait is a deadline it set itself.
    ///
    /// This is the positive control for the hub's liveness watch: the abrupt
    /// test above passes on the carrier's own error, and would pass with the
    /// watch removed.
    #[test]
    fn a_visited_document_reports_unavailable_when_the_holder_goes_silent() {
        let joiner = InMemoryProvider::from_seed([0xf6; 32]);
        let founder = found_and_invite(
            "silent",
            0xf5,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
        );
        let holder_root = InMemoryProvider::from_seed([0xf5; 32])
            .master_public_key()
            .to_bytes();
        let address = crate::knot_authoring::place_held_address(&holder_root, "field.knot");

        let lanes = joiner_lanes(&founder, &joiner);
        let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
            .expect("admission stored the grant beside the rendezvous");
        let dial = super::holder_dial(&lanes, &founder.ticket, &grant, &joiner, [33; 32])
            .expect("the grant is spent on a leaf to this place-transport key");
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let hub = crate::knot_authoring::visit_holder(dial, wake)
            .expect("the founder admits this writer at the projection door");
        let probe = crate::knot_authoring::HubProbe::open(hub, &address)
            .expect("the visited document resolves by its place-held address");

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        founder
            .worker
            .command(PlaceWorkerCommand::Freeze(Duration::from_secs(20), ack_tx));
        ack_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the founder's lane runtime was parked");
        let froze = Instant::now();

        let bound = Duration::from_secs(15);
        let reason = probe
            .wait_for_unavailable(bound)
            .unwrap_or_else(|| panic!("no unavailable within {} s", bound.as_secs()));
        let noticed = froze.elapsed();
        eprintln!(
            "silent: unavailable after {} ms with {reason}",
            noticed.as_millis()
        );
        assert!(
            noticed < bound,
            "the product promises under 15 s; took {} ms",
            noticed.as_millis()
        );

        drop(probe);
        drop(lanes);
        // The founder's runtime is still parked; its stores unlock when the
        // hold expires, and nothing here waits on that.
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// Regression for the accept-loop timing bug: the founder's door must
    /// judge a visitor's grant as of the moment it actually dials, not the
    /// moment the accept loop started waiting. A fixed test clock cannot
    /// exercise this at all (it never advances), so both ends run
    /// `AuthorityClock::SystemTime` and the founder goes live well before the
    /// writer's grant even exists.
    #[test]
    fn a_writer_invited_after_the_founder_is_already_live_is_admitted() {
        let tag = "late-invite";
        let seed = 0xc6u8;
        let root = std::env::temp_dir().join(format!(
            "turnstone-place-projection-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let vault = root.join("vault");
        for directory in [&host, &guest, &vault] {
            std::fs::create_dir_all(directory).unwrap();
        }
        std::fs::write(vault.join("field.knot"), "# Field\n").unwrap();

        let clocked = crate::place::worker::PlaceWorkerSettings {
            authority_clock: crate::place::worker::AuthorityClock::SystemTime,
            ..settings()
        };

        let founder = RootIdentity::Unsealed(InMemoryProvider::from_seed([seed; 32]));
        let host_settings = crate::place::worker::PlaceWorkerSettings {
            knot_root: Some(vault.clone()),
            ..clocked.clone()
        };
        let (worker, updates) =
            spawn_place_worker(Arc::new(|| {}), Arc::new(founder), host_settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Found {
            session,
            generation: 1,
            directory: host.clone(),
            name: "Hearth".into(),
        });
        // The founder is live now, on a real clock, with no writer invited
        // yet — the accept loop's `now_ms` (if sampled here) would already be
        // stale by the time a grant minted below arrives.
        let (binding, ticket) = loop {
            match updates.recv_timeout(Duration::from_secs(60)) {
                Ok(Update::PlaceFounded {
                    result: Ok((binding, snapshot)),
                    ..
                }) => {
                    let sync = snapshot.sync.expect("a founded place binds lanes");
                    let projection = sync
                        .projection
                        .as_ref()
                        .expect("a founder with a vault serves it");
                    assert_eq!(projection.live_sessions, 0, "nobody has dialed yet");
                    break (binding, sync.local_rendezvous[0].clone());
                },
                Ok(Update::PlaceFounded {
                    result: Err(error), ..
                }) => panic!("founding refused: {error}"),
                Ok(_) => continue,
                Err(error) => panic!("founding never answered: {error}"),
            }
        };

        // At least 50 ms of real time before the invitation — and therefore
        // the grant's `not_before_ms` — is minted.
        std::thread::sleep(Duration::from_millis(75));

        let joiner = InMemoryProvider::from_seed([0xc7; 32]);
        let prekey = prepare_group_identity(&guest, &joiner, binding.moot.0).unwrap();
        worker.command(PlaceWorkerCommand::Invite {
            session,
            generation: 1,
            directory: host.clone(),
            prekey,
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: None,
        });
        let invite = expect_invited(&updates, "the late writer invitation");
        assert!(
            invite.projection_grant.is_some(),
            "a writer invitation carries the projection grant"
        );
        invite.validate().expect("and still validates");

        crate::place::worker::admit_invitation(&guest, &invite, &joiner, &clocked)
            .expect("the joiner is admitted");
        let (open, _) = open_cached_place(&guest, &binding, &joiner, &clocked).unwrap();
        let lanes = super::join_live(&open, &binding, &joiner, std::slice::from_ref(&ticket), None, None)
            .expect("the joiner binds its lanes");
        let grant = crate::place::rendezvous::load_projection_grant(&guest)
            .expect("admission stored the grant beside the rendezvous");

        let dialed = Instant::now();
        let (carrier, _holder) = super::dial_holder(&lanes, &ticket, &grant, &joiner, [24; 32])
            .expect(
                "the founder must admit this writer even though the grant postdates \
                 the moment the accept loop started waiting",
            );
        let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
            .expect("discover the holder endpoint");
        retained.mount(0).expect("mount the projected vault");
        eprintln!(
            "late writer: dialed and mounted in {} ms",
            dialed.elapsed().as_millis()
        );

        let _ = retained.close();
        drop(lanes);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// T5c, the visiting half: the writer opens the founder's document by its
    /// place-held address through the ordinary Knot hub, authors a revision by
    /// the hub's ordinary save path, and the founder's file is what changed.
    /// Then the holder goes away and the hub says unavailable, not gone.
    ///
    /// Everything the shell would do, minus the window: `holder_dial` prepares
    /// the dial on the identity's own thread, `visit_holder` opens it on the
    /// visiting thread and runs the same `run_hub` a local vault runs, and the
    /// address the visitor asked for is resolved against what the holder
    /// disclosed, which is the holder's own local path.
    #[test]
    fn a_writer_visits_a_held_document_and_is_told_when_the_holder_goes() {
        let joiner = InMemoryProvider::from_seed([0xe2; 32]);
        let founder = found_and_invite(
            "visit",
            0xe1,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
        );
        let holder_root = InMemoryProvider::from_seed([0xe1; 32])
            .master_public_key()
            .to_bytes();
        let address = crate::knot_authoring::place_held_address(&holder_root, "field.knot");

        let lanes = joiner_lanes(&founder, &joiner);
        let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
            .expect("admission stored the grant beside the rendezvous");
        let dialed = Instant::now();
        let dial = super::holder_dial(&lanes, &founder.ticket, &grant, &joiner, [23; 32])
            .expect("the grant is spent on a leaf to this place-transport key");
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let hub = crate::knot_authoring::visit_holder(dial, wake)
            .expect("the founder admits this writer at the projection door");

        let mut probe = crate::knot_authoring::HubProbe::open(hub, &address)
            .expect("the visited document resolves by its place-held address");
        eprintln!(
            "visit: mounted {address} in {} ms, disclosed as {}",
            dialed.elapsed().as_millis(),
            probe.disclosed_address()
        );
        assert!(
            probe.disclosed_address().ends_with("field.knot")
                && probe.disclosed_address() != address,
            "the holder discloses its own local address, not the shared one"
        );
        assert_eq!(probe.source(), "# Field\n");

        let authored = "# Field\n\nThe visitor was here.\n";
        assert_eq!(
            probe.save(authored).expect("the holder took the visited edit"),
            authored
        );
        assert_eq!(
            std::fs::read_to_string(&founder.document).unwrap(),
            authored,
            "the holder's own file is what changed; the visitor holds no replica"
        );

        // The holder goes: its worker releases the lanes the projection host
        // was serving on. The visited document is unavailable, which is not
        // the same as revoked — nothing was taken away, it is out of reach.
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        founder.worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        let lost = Instant::now();
        let reason = probe
            .wait_for_unavailable(Duration::from_secs(60))
            .expect("the visited surface is told its holder is gone");
        eprintln!(
            "visit: unavailable after {} ms with {reason}",
            lost.elapsed().as_millis()
        );

        drop(probe);
        drop(lanes);
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// The same place, a reader: no grant in the envelope, nothing stored, and
    /// a dial with no certificate is refused at the door rather than served a
    /// read-only view of somebody's vault.
    #[test]
    fn a_reader_is_refused_at_the_projection_door() {
        let joiner = InMemoryProvider::from_seed([0xd2; 32]);
        let founder = found_and_invite(
            "reader",
            0xd1,
            &joiner,
            crate::place::PlaceInviteAccess::Reader,
        );
        assert!(
            founder.invite.projection_grant.is_none(),
            "a reader invitation carries no projection grant"
        );
        founder.invite.validate().expect("and still validates");

        let lanes = joiner_lanes(&founder, &joiner);
        assert!(
            crate::place::rendezvous::load_projection_grant(&founder.guest).is_none(),
            "nothing was stored beside the rendezvous"
        );

        let refused = dial_without_a_grant(&lanes, &founder.ticket, &joiner)
            .expect_err("a reader must not be served the holder vault");
        eprintln!("reader: refused at the door with {refused}");

        // The refusal is the host's, not a dropped connection: its own
        // observation counts the peer it turned away.
        let deadline = Instant::now() + Duration::from_secs(30);
        let counted = loop {
            let serving = founder_projection(&founder).expect("the founder still serves");
            if serving.refused > 0 || Instant::now() > deadline {
                break serving;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        assert_eq!(counted.refused, 1, "the host counted exactly one refusal");
        assert_eq!(counted.live_sessions, 0, "and served nobody");

        drop(lanes);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        founder.worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// `dial_holder` without the certificate: the same hello on the same
    /// transport, presenting no delegation at all.
    fn dial_without_a_grant(
        lanes: &super::LiveLanes,
        ticket: &str,
        identity: &dyn IdentityProvider,
    ) -> Result<(), String> {
        let moot = lanes.moot;
        let derived = identity
            .derive_keypair(&super::transport_salt(moot))
            .unwrap();
        let visitor = InMemoryProvider::from_seed(derived.to_seed());
        let transport = Arc::clone(&lanes._transport);
        let hello = graphshell::admission::open_session(
            &visitor,
            notochord::NetworkId(moot),
            crate::place::projection_host::projection_profile(),
            notochord::TrafficClass::Interactive,
            [31; 32],
            &graphshell::network_carrier::projection_binding(transport.local_peer_id()),
            Vec::new(),
        )
        .unwrap();
        let runtime = lanes._runtime.as_ref().expect("lane runtime is present");
        runtime.block_on(async {
            let peer = transport
                .add_peer_ticket(ticket)
                .await
                .map_err(|error| error.to_string())?;
            match graphshell::network_carrier::dial_projection_session(
                &*transport,
                peer,
                &hello,
                &notochord::HandshakeLimits::default(),
            )
            .await
            {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(reason)) => Err(format!("{reason:?}")),
                Err(error) => Err(error.to_string()),
            }
        })
    }

    /// Take the next invitation off a worker's update channel, stepping past
    /// the lane nudges that share it.
    fn expect_invited(
        updates: &std::sync::mpsc::Receiver<Update>,
        what: &str,
    ) -> Box<crate::place::invite::PlaceInviteV1> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            assert!(Instant::now() < deadline, "{what}: never answered");
            match updates.recv_timeout(Duration::from_secs(5)) {
                Ok(Update::PlaceInvited {
                    result: Ok(invite), ..
                }) => return invite,
                Ok(Update::PlaceInvited {
                    result: Err(error), ..
                }) => panic!("{what}: refused: {error}"),
                _ => continue,
            }
        }
    }

    /// One Gemot lane's counters out of a joiner's snapshot.
    fn lane_counters(
        snapshot: &crate::place::OfflinePlaceSnapshot,
        name: &str,
    ) -> (u64, u64) {
        let sync = snapshot.sync.as_ref().expect("a live joiner has lanes");
        let lane = sync
            .lanes
            .iter()
            .find(|lane| lane.name == name)
            .unwrap_or_else(|| panic!("{name} is one of the nine"));
        (lane.sync_rounds, lane.ops_received)
    }

    /// A membership admission authored AFTER a joiner is connected reaches it
    /// live, on the membership lane, without a new reconciliation round.
    ///
    /// Both sides are the product path: the founder's `Found` binds nine
    /// lanes listen-only, the joiner's `Join` dials the founder's own ticket
    /// through `join_live`, and the third root is admitted by
    /// `PlaceWorkerCommand::Invite`, which is the only caller of
    /// `admit_member`. The graph node stays as the positive control: an
    /// absence on the membership lane would mean nothing unless something
    /// else crossed in the same run.
    #[test]
    fn a_membership_admission_and_a_shared_node_race_to_a_connected_joiner() {
        an_admission_reaches_a_connected_joiner(crate::place::PlaceInviteAccess::Reader);
    }

    /// The Writer half: admitting at Write authors a delegation too, and it
    /// has to travel the same way, on its own lane, to be usable.
    #[test]
    fn a_writer_admission_carries_its_delegation_to_a_connected_joiner() {
        an_admission_reaches_a_connected_joiner(crate::place::PlaceInviteAccess::Writer);
    }

    fn an_admission_reaches_a_connected_joiner(access: crate::place::PlaceInviteAccess) {
        let writer = matches!(access, crate::place::PlaceInviteAccess::Writer);
        let root = std::env::temp_dir().join(format!(
            "turnstone-place-admit-live-{}-{}",
            if writer { "writer" } else { "reader" },
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let newcomer = root.join("third");
        for directory in [&host, &guest, &newcomer] {
            std::fs::create_dir_all(directory).unwrap();
        }

        let founder = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xa1; 32]));
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xa2; 32]));
        let third = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xa3; 32]));

        // The founder: `Found` opens the place and binds its nine lanes
        // listen-only, minting the ticket its invitations carry.
        let (host_worker, host_updates) =
            spawn_place_worker(Arc::new(|| {}), Arc::new(founder), settings());
        let host_session = SessionId::new();
        host_worker.command(PlaceWorkerCommand::Found {
            session: host_session,
            generation: 1,
            directory: host.clone(),
            name: "Hearth".into(),
        });
        let binding = loop {
            match host_updates.recv_timeout(Duration::from_secs(60)) {
                Ok(Update::PlaceFounded {
                    result: Ok((binding, snapshot)),
                    ..
                }) => {
                    let sync = snapshot.sync.expect("a founded place binds lanes");
                    assert_eq!(sync.dialed_rendezvous, 0, "a founder dials nobody");
                    assert!(!sync.local_rendezvous.is_empty(), "the founder has a ticket");
                    break binding;
                },
                Ok(Update::PlaceFounded {
                    result: Err(error), ..
                }) => panic!("founding refused: {error}"),
                Ok(_) => continue,
                Err(error) => panic!("founding never answered: {error}"),
            }
        };

        // The joiner: offer a pre-key, get admitted as a writer, dial in.
        let joiner_prekey = prepare_group_identity(&guest, &joiner, binding.moot.0).unwrap();
        host_worker.command(PlaceWorkerCommand::Invite {
            session: host_session,
            generation: 1,
            directory: host.clone(),
            prekey: joiner_prekey,
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: None,
        });
        let invite = expect_invited(&host_updates, "the joiner's invitation");

        let (guest_worker, guest_updates) =
            spawn_place_worker(Arc::new(|| {}), Arc::new(joiner), settings());
        let guest_session = SessionId::new();
        guest_worker.command(PlaceWorkerCommand::Join {
            session: guest_session,
            generation: 1,
            directory: guest.clone(),
            invite,
        });
        match guest_updates.recv_timeout(Duration::from_secs(60)) {
            Ok(Update::PlaceJoined { result: Ok(_), .. }) => {},
            Ok(Update::PlaceJoined {
                result: Err(error), ..
            }) => panic!("join refused: {error}"),
            _ => panic!("join answered with an unrelated update"),
        }

        // Converged BEFORE anything else is authored. The chat channel is the
        // proof the lanes themselves carried something: the invitation's
        // evidence drop covers Gemot only, so a channel can only have come
        // over commons/chat.
        let converged = converge_until(
            &guest_worker,
            &guest_updates,
            guest_session,
            "initial catch-up",
            |s| s.moot.members == 2 && s.chat.channels == 1,
        );
        report_lanes("joiner at initial convergence", &converged);
        assert_eq!(converged.graph.nodes, 0, "nothing is shared yet");
        // The baseline the live-delivery claim is measured against: rounds
        // must not move, counters must.
        let membership_before = lane_counters(&converged, "gemot/membership/v1");
        let delegation_before = lane_counters(&converged, "gemot/delegation/v1");
        let certificates_before = converged.moot.delegated_certificates;

        // Now, with the joiner still connected: admit a THIRD root through the
        // product Invite path. A Reader gets membership only; a Writer gets a
        // delegation on its own lane as well.
        let third_prekey = prepare_group_identity(&newcomer, &third, binding.moot.0).unwrap();
        host_worker.command(PlaceWorkerCommand::Invite {
            session: host_session,
            generation: 1,
            directory: host.clone(),
            prekey: third_prekey,
            access,
            lifetime_ms: None,
        });
        let _third_invite = expect_invited(&host_updates, "the third root's invitation");
        let admitted_at = Instant::now();

        // And author one graph node on the same open, as the comparison.
        host_worker.command(PlaceWorkerCommand::Author {
            session: host_session,
            generation: 1,
            request: 1,
            command: PlaceCommand::ShareNode {
                address: "https://shared.example/page".into(),
            },
        });
        expect_authored(&host_updates, 1);
        let shared_at = Instant::now();
        eprintln!(
            "founder: third member admitted at t+0 ms, shared node authored at t+{} ms",
            shared_at.duration_since(admitted_at).as_millis()
        );

        // Poll the joiner exactly as the app does: a Resync re-folds whatever
        // the lanes have drained into the stores.
        const BUDGET: Duration = Duration::from_secs(10);
        let deadline = admitted_at + BUDGET;
        let mut members_at: Option<Duration> = None;
        let mut node_at: Option<Duration> = None;
        let mut delegated_at: Option<Duration> = None;
        let mut last = converged;
        while Instant::now() < deadline
            && (members_at.is_none() || node_at.is_none() || (writer && delegated_at.is_none()))
        {
            guest_worker.command(PlaceWorkerCommand::Resync {
                session: guest_session,
                generation: 1,
            });
            let inner = Instant::now() + Duration::from_secs(3);
            while Instant::now() < inner {
                match guest_updates.recv_timeout(Duration::from_millis(500)) {
                    Ok(Update::PlaceOpened {
                        result: Ok(snapshot),
                        ..
                    }) => {
                        if members_at.is_none() && snapshot.moot.members >= 3 {
                            members_at = Some(admitted_at.elapsed());
                        }
                        if node_at.is_none() && snapshot.graph.nodes >= 1 {
                            node_at = Some(shared_at.elapsed());
                        }
                        if delegated_at.is_none()
                            && snapshot.moot.delegated_certificates > certificates_before
                        {
                            delegated_at = Some(admitted_at.elapsed());
                        }
                        last = snapshot;
                        break;
                    },
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        match node_at {
            Some(elapsed) => eprintln!(
                "joiner: the founder's shared node arrived {} ms after it was authored",
                elapsed.as_millis()
            ),
            None => eprintln!(
                "joiner: the founder's shared node NEVER arrived within {} s",
                BUDGET.as_secs()
            ),
        }
        match members_at {
            Some(elapsed) => eprintln!(
                "joiner: the third member arrived {} ms after admission",
                elapsed.as_millis()
            ),
            None => eprintln!(
                "joiner: the third member NEVER arrived within {} s (still {} members)",
                BUDGET.as_secs(),
                last.moot.members
            ),
        }
        if writer {
            match delegated_at {
                Some(elapsed) => eprintln!(
                    "joiner: the new writer's delegation arrived {} ms after admission",
                    elapsed.as_millis()
                ),
                None => eprintln!(
                    "joiner: the delegation NEVER arrived within {} s (still {} certificates)",
                    BUDGET.as_secs(),
                    last.moot.delegated_certificates
                ),
            }
        }
        report_lanes("joiner at the end of the window", &last);

        // The control. Without it an absent membership change proves nothing
        // about membership: it could simply be a dead connection.
        assert!(
            node_at.is_some(),
            "the graph control never crossed, so this run says nothing about membership"
        );
        assert!(
            members_at.is_some(),
            "the third member never reached the connected joiner within {} s (still {} members)",
            BUDGET.as_secs(),
            last.moot.members
        );

        // Live delivery, not a fresh round: exactly one operation crossed the
        // membership lane and the lane's round count never moved.
        let membership_after = lane_counters(&last, "gemot/membership/v1");
        assert_eq!(
            membership_after.1,
            membership_before.1 + 1,
            "the membership lane carried {} operations, not the one published",
            membership_after.1 - membership_before.1
        );
        assert_eq!(
            membership_after.0, membership_before.0,
            "the membership lane ran a new sync round, so this was reconciliation"
        );

        if writer {
            assert!(
                delegated_at.is_some(),
                "the new writer's delegation never reached the joiner within {} s",
                BUDGET.as_secs()
            );
            let delegation_after = lane_counters(&last, "gemot/delegation/v1");
            assert_eq!(
                delegation_after.1,
                delegation_before.1 + 1,
                "the delegation lane carried {} operations, not the one published",
                delegation_after.1 - delegation_before.1
            );
            assert_eq!(
                delegation_after.0, delegation_before.0,
                "the delegation lane ran a new sync round, so this was reconciliation"
            );
            // The fold, not just the counter: the joiner now holds the new
            // member's write capability.
            assert_eq!(
                last.moot.delegated_certificates,
                certificates_before + 1,
                "the joiner's delegation fold does not carry the new writer"
            );
        } else {
            assert_eq!(
                lane_counters(&last, "gemot/delegation/v1").1,
                delegation_before.1,
                "a reader admission published a delegation"
            );
            assert_eq!(
                last.moot.delegated_certificates, certificates_before,
                "a reader admission changed the joiner's delegation fold"
            );
        }

        for (worker, updates) in [(&host_worker, &host_updates), (&guest_worker, &guest_updates)] {
            let _ = updates;
            let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
            worker.command(PlaceWorkerCommand::Release(ack_tx));
            let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One authored command's answer, stepping past lane nudges.
    fn authored(
        updates: &std::sync::mpsc::Receiver<Update>,
        request: u64,
    ) -> Result<crate::place::OfflinePlaceSnapshot, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok(Update::PlaceCommandDone {
                request: got,
                result,
                ..
            }) = updates.recv_timeout(Duration::from_secs(10))
                && got == request
            {
                return result;
            }
        }
        panic!("request {request} never answered");
    }

    /// Ask a member's worker to prepare a visit to the founder's document.
    /// The prepared dial is dropped unopened: only the answer matters here.
    fn visit(
        worker: &armillary::ActorHandle<PlaceWorkerCommand>,
        updates: &std::sync::mpsc::Receiver<Update>,
        session: SessionId,
        directory: &Path,
        holder_root: [u8; 32],
        request: u64,
    ) -> Result<(), String> {
        worker.command(PlaceWorkerCommand::VisitDocument {
            session,
            generation: 1,
            directory: directory.to_path_buf(),
            holder_root,
            path: "field.knot".into(),
            request,
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok(Update::PlaceDocumentVisit {
                request: got,
                result,
                ..
            }) = updates.recv_timeout(Duration::from_secs(10))
                && got == request
            {
                return result.map(|_| ());
            }
        }
        panic!("visit {request} never answered");
    }

    fn release(worker: &armillary::ActorHandle<PlaceWorkerCommand>) {
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
    }

    /// Stop a member's worker the way a restart does, then act as a member
    /// that bypasses its own worker: reopen its stores by hand, optionally
    /// author a node its worker would refuse, and dial the founder's door with
    /// the grant it still holds. Returns the bind, held so the node can cross,
    /// and the door's refusal.
    fn bypass_after_restart(
        founder: &ServedFounder,
        worker: &armillary::ActorHandle<PlaceWorkerCommand>,
        member: &InMemoryProvider,
        clock: &crate::place::worker::PlaceWorkerSettings,
        forge: Option<&str>,
        nonce: [u8; 32],
    ) -> (crate::place::worker::OpenPlace, super::LiveLanes, String) {
        release(worker);
        // Release does not wait for the sync actors' store clones the way a
        // process exit would release them, so the reopen waits instead.
        let released = Instant::now();
        let deadline = released + Duration::from_secs(30);
        let (mut open, _) = loop {
            match open_cached_place(&founder.guest, &founder.binding, member, clock) {
                Ok(opened) => {
                    eprintln!("restart: stores reopened {} ms after release", released.elapsed().as_millis());
                    break opened;
                },
                Err(error) if error.contains("already open") && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                },
                Err(error) => panic!("reopen the member's stores: {error}"),
            }
        };
        let lanes = super::join_live(
            &open,
            &founder.binding,
            member,
            std::slice::from_ref(&founder.ticket),
            None,
            None,
        )
        .expect("the member binds its lanes");
        if let Some(address) = forge {
            let address = address.to_string();
            let operation = pollster::block_on(open.graph.edit(move |log| {
                log.insert_node(
                    &Author::new("turnstone"),
                    Container::new(address.clone()).with_address(address),
                );
            }))
            .unwrap();
            lanes.publish_graph(operation).unwrap();
        }
        let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
            .expect("the member still holds its grant");
        let refusal = match super::dial_holder(&lanes, &founder.ticket, &grant, member, nonce) {
            Ok(_) => panic!("the door admitted a withdrawn grant"),
            Err(error) => error,
        };
        (open, lanes, refusal)
    }

    fn drop_bind(open: crate::place::worker::OpenPlace, mut lanes: super::LiveLanes) {
        lanes.leave_and_wait();
        drop(lanes);
        drop(open);
    }

    fn both_permissions(effective: bool) -> Option<crate::place::PlacePermissionSnapshot> {
        Some(crate::place::PlacePermissionSnapshot {
            message_write: effective,
            graph_write: effective,
        })
    }

    fn status_of(
        founder: &ServedFounder,
        snapshot: crate::place::OfflinePlaceSnapshot,
    ) -> Vec<String> {
        crate::place::PlaceState::Offline {
            binding: founder.binding.clone(),
            generation: 1,
            snapshot,
        }
        .status_lines()
    }

    fn joined_snapshot(
        updates: &std::sync::mpsc::Receiver<Update>,
    ) -> crate::place::OfflinePlaceSnapshot {
        match updates.recv_timeout(Duration::from_secs(60)) {
            Ok(Update::PlaceJoined {
                result: Ok((_, snapshot)),
                ..
            }) => snapshot,
            Ok(Update::PlaceJoined {
                result: Err(error), ..
            }) => panic!("join refused: {error}"),
            _ => panic!("join answered with an unrelated update"),
        }
    }

    /// Share one node and send one message; both answers are returned.
    fn try_both_writes(
        worker: &armillary::ActorHandle<PlaceWorkerCommand>,
        updates: &std::sync::mpsc::Receiver<Update>,
        session: SessionId,
        channel: &str,
        first_request: u64,
    ) -> [Result<crate::place::OfflinePlaceSnapshot, String>; 2] {
        let commands = [
            PlaceCommand::ShareNode {
                address: "https://refused.example/page".into(),
            },
            PlaceCommand::SendMessage {
                channel: channel.into(),
                body: "refused".into(),
            },
        ];
        let mut request = first_request;
        commands.map(|command| {
            worker.command(PlaceWorkerCommand::Author {
                session,
                generation: 1,
                request,
                command,
            });
            let answer = authored(updates, request);
            request += 1;
            answer
        })
    }

    /// T1 live: a founder revokes a connected writer. The writer's own status
    /// reads the revocation; its writes and its dial are refused with the
    /// reason; after a restart the founder's door refuses the grant it still
    /// holds, a node it forges is retained and not effective, and Reconnect
    /// is refused by the membership recheck.
    #[test]
    fn a_revoked_writer_sees_it_live_and_cannot_write_dial_or_reconnect() {
        use crate::place::{PlaceMemberAccess, PlaceStanding};
        let joiner = InMemoryProvider::from_seed([0x3b; 32]);
        let joiner_root = joiner.master_public_key().to_bytes();
        let founder_root = InMemoryProvider::from_seed([0x3a; 32])
            .master_public_key()
            .to_bytes();
        let founder = found_and_invite(
            "revoke",
            0x3a,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
        );

        let member = || Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed([0x3b; 32])));
        let (guest_worker, guest_updates) =
            spawn_place_worker(Arc::new(|| {}), member(), settings());
        let session = SessionId::new();
        guest_worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: founder.guest.clone(),
            invite: founder.invite.clone(),
        });
        joined_snapshot(&guest_updates);
        let joined = converge_until(&guest_worker, &guest_updates, session, "writer catch-up", |s| {
            s.moot.members == 2 && s.chat.channels == 1
        });
        assert_eq!(joined.standing, PlaceStanding::Member);
        assert_eq!(joined.permissions, both_permissions(true));

        // The controls: while admitted, the writer shares and may dial.
        const BEFORE: &str = "https://before.example/page";
        guest_worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request: 1,
            command: PlaceCommand::ShareNode {
                address: BEFORE.into(),
            },
        });
        authored(&guest_updates, 1).expect("an admitted writer shares");
        visit(&guest_worker, &guest_updates, session, &founder.guest, founder_root, 1)
            .expect("an admitted writer's dial is prepared");
        let seen = converge_until(
            &founder.worker,
            &founder.updates,
            founder.session,
            "the founder holds the writer's node",
            |s| s.graph.nodes == 1 && s.members.len() == 2,
        );
        assert!(seen.members.iter().any(|entry| {
            entry.root == joiner_root && entry.access == PlaceMemberAccess::Write
        }));

        founder.worker.command(PlaceWorkerCommand::Author {
            session: founder.session,
            generation: 1,
            request: 9,
            command: PlaceCommand::RevokeMember {
                member: joiner_root,
            },
        });
        let revoked = authored(&founder.updates, 9).expect("the founder revokes its writer");
        let revoked_at = Instant::now();
        assert_eq!(revoked.members.len(), 1);
        // Authority is classified by subject at evaluation time, so the node
        // shared while admitted leaves the effective graph too, retained.
        eprintln!(
            "revoke: the founder's pre-revocation writer node reads {} effective, {} revoked",
            revoked.graph.nodes, revoked.graph.revoked_authority
        );
        assert_eq!(
            (revoked.graph.nodes, revoked.graph.revoked_authority),
            (0, 1)
        );

        let withdrawn = converge_until(
            &guest_worker,
            &guest_updates,
            session,
            "the revocation reaches the connected writer",
            |s| {
                s.standing == PlaceStanding::MembershipRevoked
                    && s.permissions == both_permissions(false)
            },
        );
        let arrived = revoked_at.elapsed();
        eprintln!(
            "revoke: the writer read its revocation {} ms after it was authored",
            arrived.as_millis()
        );
        assert!(arrived < Duration::from_secs(10), "took {} ms", arrived.as_millis());
        assert!(withdrawn.sync.is_some(), "still on its live lanes");
        let lines = status_of(&founder, withdrawn);
        for line in [
            "Membership: revoked",
            "Message permission: not effective locally at last refresh",
            "Shared graph permission: not effective locally at last refresh",
        ] {
            assert!(lines.iter().any(|row| row == line), "{line} missing: {lines:?}");
        }

        let refused = "this profile holds no effective capability to author here: its place \
                       membership was revoked";
        for answer in try_both_writes(
            &guest_worker,
            &guest_updates,
            session,
            &founder.binding.default_channel,
            2,
        ) {
            assert_eq!(answer.unwrap_err(), refused);
        }
        assert_eq!(
            visit(&guest_worker, &guest_updates, session, &founder.guest, founder_root, 2)
                .unwrap_err(),
            "this profile cannot visit a held document: its place membership was revoked"
        );

        const FORGED: &str = "https://after.example/page";
        let (open, lanes, door) = bypass_after_restart(
            &founder,
            &guest_worker,
            &joiner,
            &settings(),
            Some(FORGED),
            [41; 32],
        );
        eprintln!("revoke: the founder's door refused the held grant with {door}");
        assert!(door.contains("Revoked"), "{door}");
        let forged_at = Instant::now();
        let retained = converge_until(
            &founder.worker,
            &founder.updates,
            founder.session,
            "the founder retains the forged node",
            |s| s.graph.revoked_authority == 2,
        );
        eprintln!(
            "revoke: the forged node was retained {} ms after it was published",
            forged_at.elapsed().as_millis()
        );
        assert_eq!(retained.graph.nodes, 0, "retained, not effective");
        assert!(!retained.shared.addresses().any(|address| address == FORGED));
        drop_bind(open, lanes);

        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), member(), settings());
        worker.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 1,
            directory: founder.guest.clone(),
            binding: founder.binding.clone(),
        });
        match updates.recv_timeout(Duration::from_secs(30)) {
            Ok(Update::PlaceOpened {
                result: Err(error), ..
            }) => assert_eq!(error, "this identity is no longer a member in retained place state"),
            Ok(Update::PlaceOpened { result: Ok(_), .. }) => panic!("a revoked member reconnected"),
            _ => panic!("reconnect answered with an unrelated update"),
        }
        release(&worker);
        release(&founder.worker);
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// T3 live: a writer invited with a short lifetime, on the system clock,
    /// sees its grant expire while connected. Writes and the dial are refused
    /// as expired, the door refuses the grant, and Reconnect still admits it,
    /// because membership is intact and reading needs no grant.
    #[test]
    fn a_bounded_writer_grant_expires_while_connected_and_reconnect_still_admits() {
        use crate::place::PlaceStanding;
        use crate::place::worker::{AuthorityClock, PlaceWorkerSettings};
        // A fixed clock cannot move inside a running worker, so this runs on
        // the system clock with a lifetime admission fits inside under load.
        const LIFETIME: Duration = Duration::from_secs(6);
        let clocked = PlaceWorkerSettings {
            authority_clock: AuthorityClock::SystemTime,
            ..settings()
        };
        let joiner = InMemoryProvider::from_seed([0x3d; 32]);
        let founder_root = InMemoryProvider::from_seed([0x3c; 32])
            .master_public_key()
            .to_bytes();
        let founder = found_and_invite_with(
            "expire",
            0x3c,
            &joiner,
            crate::place::PlaceInviteAccess::Writer,
            clocked.clone(),
            Some(LIFETIME.as_millis() as u64),
        );
        let grant = crate::place::projection_host::decode_grant(
            founder
                .invite
                .projection_grant
                .as_ref()
                .expect("a writer invitation carries its grant")
                .verified_bytes("projection grant")
                .unwrap(),
        )
        .unwrap();
        let expires_at = grant
            .certificate
            .expires_at_ms
            .expect("the lifetime bounds the grant");
        assert_eq!(
            expires_at - grant.certificate.not_before_ms,
            LIFETIME.as_millis() as u64
        );

        let member = || Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed([0x3d; 32])));
        let (guest_worker, guest_updates) =
            spawn_place_worker(Arc::new(|| {}), member(), clocked.clone());
        let session = SessionId::new();
        guest_worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: founder.guest.clone(),
            invite: founder.invite.clone(),
        });
        let joined = joined_snapshot(&guest_updates);
        eprintln!(
            "expire: the join answered with {} ms of grant left",
            expires_at.saturating_sub(clocked.authority_clock.now_ms())
        );
        // The control: admission folds its snapshot before binding lanes, so
        // this reads the grant before it expired even when the bind is slow.
        assert_eq!(joined.standing, PlaceStanding::Member);
        assert_eq!(joined.permissions, both_permissions(true));

        let expired = converge_until(
            &guest_worker,
            &guest_updates,
            session,
            "the grant expires on the connected writer",
            |s| matches!(s.standing, PlaceStanding::GrantExpired { .. }),
        );
        let late = clocked.authority_clock.now_ms().saturating_sub(expires_at);
        eprintln!("expire: the writer read its expiry {late} ms after the grant expired");
        assert_eq!(expired.standing, PlaceStanding::GrantExpired { at_ms: expires_at });
        assert_eq!(expired.permissions, both_permissions(false));
        assert!(expired.sync.is_some(), "still on its live lanes");
        let expired_line = format!("Grant: expired at {}", crate::place::utc_ms(expires_at));
        let lines = status_of(&founder, expired);
        for line in [
            expired_line.as_str(),
            "Message permission: not effective locally at last refresh",
            "Shared graph permission: not effective locally at last refresh",
        ] {
            assert!(lines.iter().any(|row| row == line), "{line} missing: {lines:?}");
        }

        let reason = format!("its grant expired at {}", crate::place::utc_ms(expires_at));
        for answer in try_both_writes(
            &guest_worker,
            &guest_updates,
            session,
            &founder.binding.default_channel,
            2,
        ) {
            assert_eq!(
                answer.unwrap_err(),
                format!("this profile holds no effective capability to author here: {reason}")
            );
        }
        assert_eq!(
            visit(&guest_worker, &guest_updates, session, &founder.guest, founder_root, 2)
                .unwrap_err(),
            format!("this profile cannot visit a held document: {reason}")
        );

        let (open, lanes, door) =
            bypass_after_restart(&founder, &guest_worker, &joiner, &clocked, None, [42; 32]);
        eprintln!("expire: the founder's door refused the held grant with {door}");
        assert!(door.contains("Expired"), "{door}");
        drop_bind(open, lanes);

        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), member(), clocked.clone());
        worker.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 1,
            directory: founder.guest.clone(),
            binding: founder.binding.clone(),
        });
        let reconnected = loop {
            match updates.recv_timeout(Duration::from_secs(30)) {
                Ok(Update::PlaceOpened {
                    result: Ok(snapshot),
                    ..
                }) => break snapshot,
                Ok(Update::PlaceOpened {
                    result: Err(error), ..
                }) => panic!("an expired writer's reconnect was refused: {error}"),
                Ok(_) => continue,
                Err(error) => panic!("reconnect never answered: {error}"),
            }
        };
        assert!(reconnected.sync.is_some(), "reconnect bound live lanes");
        assert_eq!(
            reconnected.standing,
            PlaceStanding::GrantExpired { at_ms: expires_at }
        );
        release(&worker);
        release(&founder.worker);
        let _ = std::fs::remove_dir_all(&founder.root);
    }

    /// R0 of the co-op lifecycle parity plan: does inviting a third root move
    /// the group epoch past a member who is already joined? The add's control
    /// frame travels only in the invitee's envelope, so if it rotated, the
    /// joined member could not open chat the founder authors afterwards.
    ///
    /// Message 1 (before the third invite) is the positive control for chat,
    /// a shared node authored beside message 2 is the control for the
    /// connection, and a seal/open probe between the two sealed group
    /// sessions separates "never arrived" from "arrived and unreadable",
    /// because the chat lane counts only what its admission accepted.
    #[test]
    fn a_joined_member_reads_chat_authored_after_a_later_invite() {
        let root = std::env::temp_dir()
            .join(format!("turnstone-place-r0-epoch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = root.join("host");
        let guest = root.join("guest");
        let newcomer = root.join("third");
        for directory in [&host, &guest, &newcomer] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let founder = InMemoryProvider::from_seed([0xb4; 32]);
        let joiner = InMemoryProvider::from_seed([0xb5; 32]);
        let third = InMemoryProvider::from_seed([0xb6; 32]);
        let unsealed = |seed: u8| Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed([seed; 32])));

        let (host_worker, host_updates) =
            spawn_place_worker(Arc::new(|| {}), unsealed(0xb4), settings());
        let host_session = SessionId::new();
        host_worker.command(PlaceWorkerCommand::Found {
            session: host_session,
            generation: 1,
            directory: host.clone(),
            name: "Hearth".into(),
        });
        let binding = loop {
            match host_updates.recv_timeout(Duration::from_secs(60)) {
                Ok(Update::PlaceFounded {
                    result: Ok((binding, _)),
                    ..
                }) => break binding,
                Ok(Update::PlaceFounded {
                    result: Err(error), ..
                }) => panic!("founding refused: {error}"),
                Ok(_) => continue,
                Err(error) => panic!("founding never answered: {error}"),
            }
        };
        let moot = binding.moot.0;
        let channel = binding.default_channel.clone();
        // The sealed session on disk, which is what Invite reads and writes.
        let epoch_on_disk = |directory: &Path, who: &InMemoryProvider| {
            let group = load_group_session(directory, who, moot).unwrap();
            (group.current_epoch(), group.epoch_count())
        };
        let head = |epoch: Option<[u8; 32]>| epoch.map_or_else(|| "none".to_string(), |e| hex_head(&e));
        let founded_epoch = epoch_on_disk(&host, &founder);

        // A: invited through the product path, joined live over the ticket.
        let joiner_prekey = prepare_group_identity(&guest, &joiner, moot).unwrap();
        host_worker.command(PlaceWorkerCommand::Invite {
            session: host_session,
            generation: 1,
            directory: host.clone(),
            prekey: joiner_prekey,
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: None,
        });
        let invite = expect_invited(&host_updates, "A's invitation");
        let a_invite_epoch = invite.expected_epoch;
        let after_a_epoch = epoch_on_disk(&host, &founder);
        let (guest_worker, guest_updates) =
            spawn_place_worker(Arc::new(|| {}), unsealed(0xb5), settings());
        let guest_session = SessionId::new();
        guest_worker.command(PlaceWorkerCommand::Join {
            session: guest_session,
            generation: 1,
            directory: guest.clone(),
            invite,
        });
        let joined = joined_snapshot(&guest_updates);
        let joiner_epoch = epoch_on_disk(&guest, &joiner);
        let converged = converge_until(
            &guest_worker,
            &guest_updates,
            guest_session,
            "initial catch-up",
            |s| s.moot.members == 2 && s.chat.channels == 1,
        );
        report_lanes("R0 A at initial convergence", &converged);

        // Positive control: chat before the third invite reaches A and counts.
        host_worker.command(PlaceWorkerCommand::Author {
            session: host_session,
            generation: 1,
            request: 1,
            command: PlaceCommand::SendMessage {
                channel: channel.clone(),
                body: "before the third invite".into(),
            },
        });
        let founder_first = authored(&host_updates, 1).expect("message 1 authored");
        let first_sent = Instant::now();
        let after_first = converge_until(
            &guest_worker,
            &guest_updates,
            guest_session,
            "message 1 reaches A",
            |s| s.chat.messages >= 1,
        );
        let first_elapsed = first_sent.elapsed();
        let chat_before = lane_counters(&after_first, "commons/chat/v1");
        let graph_before = lane_counters(&after_first, "commons/graph/v1");

        // B: a third root invited through the product path. B never joins.
        let third_prekey = prepare_group_identity(&newcomer, &third, moot).unwrap();
        host_worker.command(PlaceWorkerCommand::Invite {
            session: host_session,
            generation: 1,
            directory: host.clone(),
            prekey: third_prekey,
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: None,
        });
        let third_invite = expect_invited(&host_updates, "B's invitation");
        let after_b_epoch = epoch_on_disk(&host, &founder);

        // Message 2, then the connection control on the graph lane.
        host_worker.command(PlaceWorkerCommand::Author {
            session: host_session,
            generation: 1,
            request: 2,
            command: PlaceCommand::SendMessage {
                channel: channel.clone(),
                body: "after the third invite".into(),
            },
        });
        let founder_second = authored(&host_updates, 2).expect("message 2 authored");
        let sent_at = Instant::now();
        host_worker.command(PlaceWorkerCommand::Author {
            session: host_session,
            generation: 1,
            request: 3,
            command: PlaceCommand::ShareNode {
                address: "https://r0.example/control".into(),
            },
        });
        expect_authored(&host_updates, 3);

        const WINDOW: Duration = Duration::from_secs(20);
        let deadline = sent_at + WINDOW;
        let mut message_at: Option<Duration> = None;
        let mut node_at: Option<Duration> = None;
        let mut last = after_first;
        while Instant::now() < deadline && (message_at.is_none() || node_at.is_none()) {
            guest_worker.command(PlaceWorkerCommand::Resync {
                session: guest_session,
                generation: 1,
            });
            let inner = Instant::now() + Duration::from_secs(3);
            while Instant::now() < inner {
                match guest_updates.recv_timeout(Duration::from_millis(500)) {
                    Ok(Update::PlaceOpened {
                        result: Ok(snapshot),
                        ..
                    }) => {
                        if message_at.is_none() && snapshot.chat.messages >= 2 {
                            message_at = Some(sent_at.elapsed());
                        }
                        if node_at.is_none() && snapshot.graph.nodes >= 1 {
                            node_at = Some(sent_at.elapsed());
                        }
                        last = snapshot;
                        break;
                    },
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let chat_after = lane_counters(&last, "commons/chat/v1");
        let graph_after = lane_counters(&last, "commons/graph/v1");

        // Key agreement with no lane involved: the founder's sealed session
        // seals, A's opens.
        let founder_group = load_group_session(&host, &founder, moot).unwrap();
        let joiner_group = load_group_session(&guest, &joiner, moot).unwrap();
        let probe = founder_group.seal_random(b"r0 probe").unwrap();
        let probe_opened = joiner_group.open(&probe);

        eprintln!(
            "R0 epochs on disk: founder founded {} (count {}), after A {} ({}), after B {} ({}); A joined {} ({}, join snapshot epochs {}); A's invite named {}, B's invite named {}",
            head(founded_epoch.0), founded_epoch.1,
            head(after_a_epoch.0), after_a_epoch.1,
            head(after_b_epoch.0), after_b_epoch.1,
            head(joiner_epoch.0), joiner_epoch.1, joined.group.epochs,
            hex_head(&a_invite_epoch),
            hex_head(&third_invite.expected_epoch),
        );
        eprintln!(
            "R0 founder live worker group cache: after message 1 epochs {} members {}; after message 2 epochs {} members {}; founder chat messages {} then {}",
            founder_first.group.epochs, founder_first.group.members,
            founder_second.group.epochs, founder_second.group.members,
            founder_first.chat.messages, founder_second.chat.messages,
        );
        eprintln!(
            "R0 A: message 1 counted by a snapshot taken within {} ms of the founder's answer (poll resolution); chat lane (rounds, ops) {:?} -> {:?}; graph lane {:?} -> {:?}",
            first_elapsed.as_millis(), chat_before, chat_after, graph_before, graph_after,
        );
        eprintln!(
            "R0 A after the window: messages {} pending_causality {} pending_authority {} revoked {} nodes {} group epochs {} members {}; message 2 at {:?}, node at {:?}",
            last.chat.messages, last.chat.pending_causality, last.chat.pending_authority,
            last.chat.revoked_authority, last.graph.nodes, last.group.epochs, last.group.members,
            message_at, node_at,
        );
        eprintln!("R0 probe: founder-sealed opened by A: {:?}", probe_opened.as_ref().map(|bytes| bytes.len()));
        report_lanes("R0 A at the end of the window", &last);

        // A's retained, decrypted bodies, read from its own stores.
        release(&host_worker);
        release(&guest_worker);
        let reopen_deadline = Instant::now() + Duration::from_secs(30);
        let (reopened, _) = loop {
            match open_cached_place(&guest, &binding, &joiner, &settings()) {
                Ok(opened) => break opened,
                Err(error) if error.contains("already open") && Instant::now() < reopen_deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                },
                Err(error) => panic!("reopen A's stores: {error}"),
            }
        };
        let bodies: Vec<String> = pollster::block_on(reopened.chat.projection())
            .unwrap()
            .messages
            .iter()
            .map(|message| message.message.body.clone())
            .collect();
        eprintln!("R0 A's retained chat bodies: {bodies:?}");
        drop(reopened);
        let _ = std::fs::remove_dir_all(&root);

        assert!(
            bodies.iter().any(|body| body == "before the third invite"),
            "positive control: A never decrypted message 1"
        );
        assert!(
            node_at.is_some(),
            "the graph control never crossed within {} s, so this run says nothing about chat",
            WINDOW.as_secs()
        );
        assert!(probe_opened.is_ok(), "A cannot open what the founder's session seals: {probe_opened:?}");
        assert!(
            message_at.is_some(),
            "A never counted message 2 within {} s (still {} messages)",
            WINDOW.as_secs(),
            last.chat.messages
        );
        assert!(
            bodies.iter().any(|body| body == "after the third invite"),
            "A holds no decrypted message 2: {bodies:?}"
        );
    }

    /// Print one snapshot's nine lane counters, plus the two folds this
    /// diagnostic is about.
    fn report_lanes(what: &str, snapshot: &crate::place::OfflinePlaceSnapshot) {
        eprintln!(
            "{what}: members {} nodes {} channels {}",
            snapshot.moot.members, snapshot.graph.nodes, snapshot.chat.channels
        );
        let Some(sync) = &snapshot.sync else {
            eprintln!("{what}: no lanes");
            return;
        };
        for lane in &sync.lanes {
            eprintln!(
                "{what}: {} rounds {} ops {} syncing {}",
                lane.name, lane.sync_rounds, lane.ops_received, lane.syncing
            );
        }
    }
}

/// Domain-separated salt for this place's transport identity. A derived key,
/// never the master: the transport peer id is session machinery, and the
/// worker's projections must stay exactly as valid if it changes.
/// The carrier a visitor mounts a holder's document over: mere's network
/// carrier on this place's own p2panda stream.
pub(crate) type PlaceProjectionCarrier = NetworkCarrier<P2pandaStream>;

/// Dial a place member who holds a document, over this place's own transport.
///
/// The visiting half's one entry point. The returned carrier is what
/// `RetainedEndpointSession::over` mounts; the peer id is returned with it so
/// a caller can name the holder it reached without re-importing the ticket.
///
/// Two things happen here that are not obvious from the call. The ticket is
/// imported first, because a ticket is an address and the address book is
/// what `connect` reads. And the stored grant is not the leaf: the handshake
/// binds the claimed subject to the peer the carrier authenticated, and this
/// place's transport key is derived per place, so the grant's one remaining
/// delegation hop is spent here on a leaf from the member's Personae root to
/// its own place-transport key. That key is then what the hello claims.
///
/// The carrier is not `Send` by mere's deliberate choice, so it belongs to
/// the thread that opened it.
pub(crate) fn dial_holder(
    lanes: &LiveLanes,
    holder_ticket: &str,
    grant: &[u8],
    identity: &dyn IdentityProvider,
    nonce: [u8; 32],
) -> Result<(PlaceProjectionCarrier, PeerID), String> {
    holder_dial(lanes, holder_ticket, grant, identity, nonce)?.open()
}

/// A dial that has been prepared but not yet opened.
///
/// Everything a stream needs, with no borrow of [`LiveLanes`] left in it, so
/// the worker can prepare a dial on its own thread and a visiting thread can
/// open it on its. The carrier is produced by [`Self::open`] and stays on
/// whichever thread called it, which is the whole reason this split exists:
/// the retained session over that carrier is not `Send`, so the thread that
/// opens the stream must be the thread that reads it.
///
/// The runtime handle is a clone of the lane runtime's, and that runtime
/// lives in the worker's `LiveLanes`. It outlives every visit by ordering,
/// not by ownership: the shell drops its visiting hubs before the place is
/// released. If a runtime does go first, the carrier's next request fails and
/// the visited surface reports unavailable, which is the same thing a lost
/// holder looks like and the correct thing to show either way.
pub(crate) struct HolderDial {
    transport: Arc<P2pandaTransport>,
    handle: tokio::runtime::Handle,
    hello: notochord::SessionHello,
    ticket: String,
}

impl HolderDial {
    /// Open the stream and wrap it in the carrier. Blocking, on the lane
    /// runtime, from whatever thread calls it.
    pub(crate) fn open(self) -> Result<(PlaceProjectionCarrier, PeerID), String> {
        let limits = HandshakeLimits::default();
        let transport = Arc::clone(&self.transport);
        let hello = self.hello;
        let ticket = self.ticket;
        let (stream, peer) = self.handle.block_on(async move {
            // Idempotent for a ticket already in the address book, and the
            // only way to learn the peer id a bare ticket names.
            let peer = transport
                .add_peer_ticket(&ticket)
                .await
                .map_err(|error| format!("import holder ticket: {error}"))?;
            let stream = dial_projection_session(&*transport, peer, &hello, &limits)
                .await
                .map_err(|error| format!("dial the holder: {error}"))?
                .map_err(|reason| format!("the holder refused this session: {reason:?}"))?;
            Ok::<_, String>((stream, peer))
        })?;
        let carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(self.handle));
        Ok((carrier, peer))
    }
}

/// Prepare a dial to a holder: spend the grant's hop and sign the hello.
///
/// Everything authority-bearing happens here, on the caller's thread, where
/// the identity provider is. What crosses to a visiting thread afterwards is
/// a signed hello and a transport handle, never a key.
pub(crate) fn holder_dial(
    lanes: &LiveLanes,
    holder_ticket: &str,
    grant: &[u8],
    identity: &dyn IdentityProvider,
    nonce: [u8; 32],
) -> Result<HolderDial, String> {
    let moot = lanes.moot;
    let grant = crate::place::projection_host::decode_grant(grant)?;
    let parent = &grant.certificate;
    if parent.subject != identity.master_public_key().to_bytes() {
        return Err("projection grant names another subject".to_string());
    }

    // The place-transport key this dial will actually arrive as. A provider
    // over the derived seed, because `open_session` takes the subject from
    // whatever provider signs the hello.
    let derived = identity
        .derive_keypair(&transport_salt(moot))
        .map_err(|error| format!("derive place transport identity: {error}"))?;
    let visitor = identity::InMemoryProvider::from_seed(derived.to_seed());
    let subject = visitor.master_public_key().to_bytes();

    // The leaf copies the grant's window, so a bounded grant bounds the leaf.
    let leaf = SignedDelegationCertificate::issue(
        &ProviderRef(identity),
        DelegationCertificate::new(
            DelegationParent::Certificate(parent.id()),
            parent.subject,
            subject,
            projection_scope(moot),
            parent.not_before_ms,
            parent.not_before_ms,
            parent.expires_at_ms,
            0,
            *blake3::hash(&[&parent.nonce[..], &subject[..]].concat()).as_bytes(),
        ),
    )
    .map_err(|error| format!("issue the place-transport leaf: {error}"))?;

    let runtime = lanes._runtime.as_ref().expect("lane runtime is present");
    let transport = Arc::clone(&lanes._transport);
    let local_peer = transport.local_peer_id();
    let hello = open_session(
        &visitor,
        NetworkId(moot),
        projection_profile(),
        TrafficClass::Interactive,
        nonce,
        &projection_binding(local_peer),
        vec![grant, leaf],
    )
    .map_err(|error| format!("sign the projection hello: {error}"))?;

    Ok(HolderDial {
        transport,
        handle: runtime.handle().clone(),
        hello,
        ticket: holder_ticket.to_string(),
    })
}

fn transport_salt(moot: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(61);
    salt.extend_from_slice(b"turnstone.place.transport.v1/");
    salt.extend_from_slice(&moot);
    salt
}

/// Dial the given tickets and join all nine of the place's lanes.
///
/// An EMPTY ticket list is a listen-only bind, not a refusal: a founder who
/// has invited nobody yet still needs an endpoint before it can mint the
/// rendezvous its first invitation carries.
///
/// Takes the already-opened place rather than opening its own, so the lanes
/// drain into the same stores the worker's projections fold from. The
/// runtime is created here and owned by the returned value; the worker
/// thread stays synchronous.
pub(crate) fn join_live(
    open: &OpenPlace,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    tickets: &[String],
    projection: Option<ProjectionSetup>,
    watch: Option<(
        armillary::Emitter<crate::action::Update>,
        crate::panes::SessionId,
        u64,
    )>,
) -> Result<LiveLanes, String> {
    let keypair = identity
        .derive_keypair(&transport_salt(binding.moot.0))
        .map_err(|error| format!("derive transport identity: {error}"))?;
    let overlays = [
        sync_overlay_topic(binding.moot.0),
        sync_overlay_topic(binding.root.0),
        sync_overlay_topic(binding.chat.0),
    ];

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| format!("build lane runtime: {error}"))?;

    let (transport, local_rendezvous, moot_lanes, graph, chat) = runtime.block_on(async {
        // Active mDNS so two peers on one LAN re-find each other by node id
        // after either restarts on a fresh port; a ticket fixes an address,
        // not an identity.
        let transport = P2pandaTransport::builder(&keypair)
            .gossip()
            .mdns(MdnsDiscoveryMode::Active)
            // Beside the sync lanes, on the same endpoint: a place member
            // reaching the document's holder is the same peer relationship,
            // and giving it a second bind would give it a second address.
            .alpns(vec![graphshell::carrier::projection_alpn()])
            .bind()
            .await
            .map_err(|error| format!("bind place transport: {error}"))?;
        let local_rendezvous: Vec<String> = transport
            .ticket()
            .await
            .map(|ticket| vec![ticket])
            .unwrap_or_default();
        for ticket in tickets {
            let peer = transport
                .add_peer_ticket(ticket)
                .await
                .map_err(|error| format!("import rendezvous ticket: {error}"))?;
            transport
                .set_topics(peer, &overlays)
                .await
                .map_err(|error| format!("tag rendezvous overlays: {error}"))?;
        }
        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or_else(|| "place transport has no gossip".to_string())?;
        let moot_lanes = open
            .moot
            .join_lanes(endpoint.clone(), gossip.clone())
            .await
            .map_err(|error| format!("join Gemot lanes: {error}"))?;
        let graph = open
            .graph
            .join(endpoint.clone(), gossip.clone())
            .await
            .map_err(|error| format!("join graph lane: {error}"))?;
        let chat = open
            .chat
            .join(endpoint, gossip)
            .await
            .map_err(|error| format!("join chat lane: {error}"))?;
        Ok::<_, String>((transport, local_rendezvous, moot_lanes, graph, chat))
    })?;

    let mut lanes = LiveLanes {
        watcher: None,
        local_rendezvous,
        dialed_rendezvous: tickets.len(),
        joined: Some(JoinedNine {
            moot: moot_lanes,
            graph,
            chat,
        }),
        moot: binding.moot.0,
        projection: None,
        _transport: Arc::new(transport),
        _runtime: Some(runtime),
    };

    // No vault root means no route and no serving, and the snapshot says so
    // rather than reporting an idle host nobody could have reached.
    if let Some(setup) = projection {
        let handle = lanes
            ._runtime
            .as_ref()
            .expect("lane runtime is present")
            .handle()
            .clone();
        // Grants revoked before this bind are refused from its first accept.
        let withdrawn = crate::place::worker::withdrawn_projection_grants(open, identity)?;
        lanes.projection = Some(ProjectionServing::start(
            Arc::clone(&lanes._transport),
            &handle,
            &setup,
            &withdrawn,
        )?);
    }

    // The watcher turns lane arrivals AND projection session changes into ONE
    // app-visible nudge per settled burst. It reports that something moved; it
    // never folds a projection itself, because the authority filter belongs on
    // the worker thread with the stores, not on a sampling task.
    //
    // Built after the projection host so the sample covers it: without that,
    // the founder's status stays at "0 live sessions" for as long as nothing
    // else on the lanes happens to arrive.
    if let Some((out, session, generation)) = watch {
        let counters = lanes.counter_handles();
        let handle = lanes._runtime.as_ref().expect("lane runtime is present").handle().clone();
        lanes.watcher = Some(handle.spawn(async move {
            let mut settled = counters.total();
            loop {
                tokio::time::sleep(WATCH_TICK).await;
                let now = counters.total();
                if now == settled {
                    continue;
                }
                // Wait for the burst to stop growing before reporting, so a
                // sync round of fifty operations is one re-fold and not fifty.
                tokio::time::sleep(WATCH_TICK).await;
                let after = counters.total();
                if after != now {
                    continue;
                }
                settled = after;
                out.emit(crate::action::Update::PlaceLanesAdvanced {
                    session,
                    generation,
                });
            }
        }));
    }
    Ok(lanes)
}
