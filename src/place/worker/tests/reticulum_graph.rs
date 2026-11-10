// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::net::TcpListener;
use std::sync::mpsc;

use super::*;

const ALICE_EDIT: &str = "https://label.example/release";
const BOB_EDIT: &str = "https://archive.example/correction";

#[test]
fn reticulum_graph_automatically_catches_up_on_reconnect_and_after_either_writer_edits() {
    let directory = tempfile::tempdir().unwrap();
    let alice = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x61; 32])));
    let bob = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x62; 32])));
    let alice_dir = directory.path().join("alice");
    let bob_dir = directory.path().join("bob");
    let settings = settings();
    let (binding, invite) = enroll(&alice_dir, &bob_dir, &alice, &bob, &settings);
    let alice_contact = ReticulumGraphContact::for_graph(alice.as_ref(), binding.root).unwrap();
    let bob_contact = ReticulumGraphContact::for_graph(bob.as_ref(), binding.root).unwrap();
    let alice = AutoPeer::new(alice, alice_dir, binding.clone(), settings.clone());
    let bob = AutoPeer::new(bob, bob_dir, binding.clone(), settings);
    alice.open_offline(1);
    bob.worker.command(PlaceWorkerCommand::Join {
        session: bob.session,
        generation: 1,
        directory: bob.directory.clone(),
        invite: Box::new(invite),
    });
    let Update::PlaceJoined { result, .. } = bob.updates.recv_timeout(ANSWER).unwrap() else {
        panic!("the ticketless enrollment did not answer");
    };
    assert!(result.unwrap().1.sync.is_none());
    assert_eq!(alice.share(1, 1, ALICE_EDIT).graph.nodes, 1);
    assert_eq!(bob.share(1, 1, BOB_EDIT).graph.nodes, 1);

    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    alice.configure(ReticulumGraphRoute::Listen(addr), bob_contact);
    bob.configure(ReticulumGraphRoute::Dial(addr), alice_contact);
    alice.reconnect(2);
    bob.reconnect(2);
    let mut expected = vec![ALICE_EDIT, BOB_EDIT];
    assert_eq!(
        alice.await_graph(2, &expected),
        bob.await_graph(2, &expected)
    );

    // Each remote projection advances after the other peer's ordinary
    // authoring command. The test never asks either worker to sync or refresh.
    const LISTENER_EDIT: &str = "https://label.example/listener-edit";
    alice.share(2, 2, LISTENER_EDIT);
    expected.push(LISTENER_EDIT);
    bob.await_graph(2, &expected);
    const DIALER_EDIT: &str = "https://archive.example/dialer-edit";
    bob.share(2, 2, DIALER_EDIT);
    expected.push(DIALER_EDIT);
    alice.await_graph(2, &expected);

    // Drop the listener's carrier by opening it offline. The dialer keeps
    // running while the listener edits, then rediscovers its returning peer.
    alice.open_offline(3);
    const DISCONNECTED_EDIT: &str = "https://label.example/disconnected-edit";
    alice.share(3, 3, DISCONNECTED_EDIT);
    expected.push(DISCONNECTED_EDIT);
    alice.reconnect(4);
    assert_eq!(
        alice.await_graph(4, &expected),
        bob.await_graph(2, &expected)
    );
    alice.release();
    bob.release();
}

struct AutoPeer {
    worker: ActorHandle<PlaceWorkerCommand>,
    updates: mpsc::Receiver<Update>,
    session: SessionId,
    directory: PathBuf,
    binding: PlaceBindingV1,
}

impl AutoPeer {
    fn new(
        identity: Arc<RootIdentity>,
        directory: PathBuf,
        binding: PlaceBindingV1,
        settings: PlaceWorkerSettings,
    ) -> Self {
        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings);
        Self {
            worker,
            updates,
            session: SessionId::new(),
            directory,
            binding,
        }
    }

    fn configure(&self, route: ReticulumGraphRoute, peer: ReticulumGraphContact) {
        let config = ReticulumGraphConfig {
            container: self.binding.root,
            route,
            peer,
            poll_ms: 500,
        };
        std::fs::write(
            self.directory.join(RETICULUM_GRAPH_FILE),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
    }

    fn open_offline(&self, generation: u64) {
        self.worker.command(PlaceWorkerCommand::Open {
            session: self.session,
            generation,
            directory: self.directory.clone(),
            binding: self.binding.clone(),
        });
        let snapshot = expect_open(&self.updates, generation);
        assert!(snapshot.sync.is_none() && !snapshot.reticulum_graph_active);
    }

    fn reconnect(&self, generation: u64) {
        self.worker.command(PlaceWorkerCommand::Reconnect {
            session: self.session,
            generation,
            directory: self.directory.clone(),
            binding: self.binding.clone(),
            contact: None,
        });
    }

    fn share(&self, generation: u64, request: u64, address: &str) -> OfflinePlaceSnapshot {
        self.worker.command(PlaceWorkerCommand::Author {
            session: self.session,
            generation,
            request,
            command: PlaceCommand::ShareNode {
                address: address.into(),
            },
        });
        let deadline = std::time::Instant::now() + ANSWER;
        loop {
            if let Update::PlaceCommandDone {
                request: completed,
                result,
                ..
            } = self
                .updates
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap()
            {
                assert_eq!(completed, request);
                return result.expect("enrolled peer authors locally");
            }
        }
    }

    fn await_graph(&self, generation: u64, addresses: &[&str]) -> String {
        let deadline = std::time::Instant::now() + ANSWER;
        loop {
            if let Update::PlaceOpened {
                generation: current,
                result,
                ..
            } = self
                .updates
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap()
            {
                assert_eq!(current, generation);
                let snapshot = result.expect("Reticulum place snapshot");
                assert!(snapshot.sync.is_none(), "an iroh lane was started");
                assert!(snapshot.reticulum_graph_active);
                let nodes = &snapshot.shared.nodes;
                if nodes.len() == addresses.len()
                    && addresses
                        .iter()
                        .all(|address| nodes.iter().any(|node| node.address == *address))
                {
                    return snapshot.graph_digest;
                }
            }
        }
    }

    fn release(&self) {
        let (reply, released) = mpsc::sync_channel(1);
        self.worker.command(PlaceWorkerCommand::Release(reply));
        released.recv_timeout(ANSWER).unwrap();
    }
}

#[test]
fn place_workers_catch_up_offline_graph_edits_over_reticulum_without_iroh() {
    let directory = tempfile::tempdir().unwrap();
    let alice_dir = directory.path().join("alice");
    let bob_dir = directory.path().join("bob");
    let alice = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x51; 32])));
    let bob = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x52; 32])));
    let settings = settings();
    let (binding, invite) = enroll(&alice_dir, &bob_dir, &alice, &bob, &settings);
    let alice_contact = ReticulumGraphContact::for_graph(alice.as_ref(), binding.root).unwrap();
    let bob_contact = ReticulumGraphContact::for_graph(bob.as_ref(), binding.root).unwrap();
    let (alice_worker, alice_updates) =
        spawn_place_worker(Arc::new(|| {}), alice, settings.clone());
    let (bob_worker, bob_updates) = spawn_place_worker(Arc::new(|| {}), bob, settings);
    let alice_session = SessionId::new();
    let bob_session = SessionId::new();
    alice_worker.command(PlaceWorkerCommand::Open {
        session: alice_session,
        generation: 1,
        directory: alice_dir.clone(),
        binding: binding.clone(),
    });
    bob_worker.command(PlaceWorkerCommand::Join {
        session: bob_session,
        generation: 1,
        directory: bob_dir.clone(),
        invite: Box::new(invite),
    });
    let alice_open = expect_open(&alice_updates, 1);
    let Update::PlaceJoined { result, .. } = bob_updates.recv_timeout(ANSWER).unwrap() else {
        panic!("Bob did not answer the ticketless invitation");
    };
    let (_, bob_open) = result.expect("Bob joins without a live lane");
    assert!(alice_open.sync.is_none() && bob_open.sync.is_none());
    assert_eq!((alice_open.graph.nodes, bob_open.graph.nodes), (0, 0));

    for (worker, updates, session, address) in [
        (&alice_worker, &alice_updates, alice_session, ALICE_EDIT),
        (&bob_worker, &bob_updates, bob_session, BOB_EDIT),
    ] {
        worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request: 1,
            command: PlaceCommand::ShareNode {
                address: address.into(),
            },
        });
        let Update::PlaceCommandDone { result, .. } = updates.recv_timeout(ANSWER).unwrap() else {
            panic!("place worker did not answer an offline edit");
        };
        let snapshot = result.expect("enrolled writer can edit offline");
        assert_eq!(snapshot.shared.nodes.len(), 1);
        assert_eq!(snapshot.shared.nodes[0].address, address);
        assert!(snapshot.sync.is_none());
    }

    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let outsider = InMemoryProvider::from_seed([0x53; 32]);
    let outsider_contact = ReticulumGraphContact::for_graph(&outsider, binding.root).unwrap();
    let refused = send_catch_up(
        &alice_worker,
        alice_session,
        ReticulumGraphRoute::Listen(addr),
        outsider_contact,
    );
    assert!(
        refused
            .recv_timeout(ANSWER)
            .unwrap()
            .unwrap_err()
            .contains("enrolled readers")
    );

    let served = send_catch_up(
        &alice_worker,
        alice_session,
        ReticulumGraphRoute::Listen(addr),
        bob_contact,
    );
    let dialed = send_catch_up(
        &bob_worker,
        bob_session,
        ReticulumGraphRoute::Dial(addr),
        alice_contact,
    );
    served
        .recv_timeout(ANSWER)
        .unwrap()
        .expect("Alice's Reticulum catch-up");
    dialed
        .recv_timeout(ANSWER)
        .unwrap()
        .expect("Bob's Reticulum catch-up");
    let alice_synced = expect_open(&alice_updates, 1);
    let bob_synced = expect_open(&bob_updates, 1);
    assert_eq!(alice_synced.graph_digest, bob_synced.graph_digest);

    for (worker, updates, session, directory) in [
        (&alice_worker, &alice_updates, alice_session, alice_dir),
        (&bob_worker, &bob_updates, bob_session, bob_dir),
    ] {
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 2,
            directory,
            binding: binding.clone(),
        });
        let reopened = expect_open(updates, 2);
        assert_eq!(reopened.graph_digest, alice_synced.graph_digest);
        assert_eq!(
            reopened
                .shared
                .nodes
                .iter()
                .map(|node| node.address.as_str())
                .collect::<Vec<_>>(),
            [BOB_EDIT, ALICE_EDIT]
        );
        assert!(reopened.sync.is_none());
        assert_eq!(reopened.graph.pending_authority, 0);
        let (ack, released) = mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack));
        released.recv_timeout(ANSWER).unwrap();
    }
}

fn enroll(
    alice_dir: &Path,
    bob_dir: &Path,
    alice: &RootIdentity,
    bob: &RootIdentity,
    settings: &PlaceWorkerSettings,
) -> (PlaceBindingV1, PlaceInviteV1) {
    let binding = found_place(alice_dir, alice, "Reticulum Commons", settings).unwrap();
    let prekey = prepare_group_identity(bob_dir, bob, binding.moot.0).unwrap();
    let (mut open, _) = open_cached_place(alice_dir, &binding, alice, settings).unwrap();
    let now = settings.authority_clock.now_ms();
    admit_member(
        &open.moot,
        &binding,
        alice,
        bob.master_public_key().to_bytes(),
        Writer,
        now,
        None,
        false,
    )
    .unwrap();
    let (invite, _) = author_invitation_with(
        &open.moot,
        &mut open.group,
        alice_dir,
        &binding,
        alice,
        &prekey,
        now + 1_000,
        Vec::new(),
        None,
    )
    .unwrap();
    (binding, invite)
}

fn expect_open(updates: &mpsc::Receiver<Update>, expected_generation: u64) -> OfflinePlaceSnapshot {
    let Update::PlaceOpened {
        generation, result, ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("place worker did not emit a graph snapshot");
    };
    assert_eq!(generation, expected_generation);
    result.expect("place snapshot")
}

fn send_catch_up(
    worker: &ActorHandle<PlaceWorkerCommand>,
    session: SessionId,
    route: ReticulumGraphRoute,
    peer: ReticulumGraphContact,
) -> mpsc::Receiver<Result<(), String>> {
    let (reply, result) = mpsc::channel();
    worker.command(PlaceWorkerCommand::CatchUpGraph {
        session,
        generation: 1,
        route,
        peer,
        reply,
    });
    result
}
