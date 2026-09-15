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

use commons::CommonsExt;
use commons::chat::ChatExt;
use gemot::moot::MootLanes;
use identity::IdentityProvider;
use stickleback::JoinedSpace;
use transport::p2panda_transport::MdnsDiscoveryMode;
use transport::{P2pandaTransport, sync_overlay_topic};

use crate::place::PlaceBindingV1;
use crate::place::worker::OpenPlace;

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
    _transport: P2pandaTransport,
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

/// Shared counter handles across all nine lanes, sampled by the watcher.
struct LaneCounters {
    handles: Vec<std::sync::Arc<std::sync::Mutex<stickleback::SyncStatus>>>,
}

impl LaneCounters {
    /// Accepted operations across every lane. A sum is right HERE and wrong in
    /// the status surface: the watcher only needs to know that something
    /// arrived, while a person needs to know which lane it arrived on.
    fn total(&self) -> u64 {
        self.handles
            .iter()
            .map(|handle| handle.lock().map(|status| status.ops_received).unwrap_or(0))
            .sum()
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

    fn joined(&self) -> &JoinedNine {
        self.joined.as_ref().expect("lanes are taken only in drop")
    }

    fn counter_handles(&self) -> LaneCounters {
        let joined = self.joined();
        let mut handles = joined.moot.status_handles().to_vec();
        handles.push(joined.graph.status_handle());
        handles.push(joined.chat.status_handle());
        LaneCounters { handles }
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
        let mut lanes = super::join_live(&open, &b, &founder, &[], None).unwrap();
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
        _transport: transport,
        _runtime: Some(runtime),
    };

    // The watcher turns lane arrivals into ONE app-visible nudge per settled
    // burst. It reports that something arrived; it never folds a projection
    // itself, because the authority filter belongs on the worker thread with
    // the stores, not on a sampling task.
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
