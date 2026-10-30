// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The founder path through the worker: `Found` opens a place nobody was
/// invited to, binds listen-only, and reports its own rendezvous without
/// ever reading as connected.
#[test]
fn a_founder_binds_listen_only_and_reports_its_own_rendezvous() {
    let root = std::env::temp_dir().join(format!("turnstone-place-founder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let host = root.join("host");
    std::fs::create_dir_all(&host).unwrap();
    let founder = RootIdentity::from_seed([0xf1; 32]);

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
            assert_eq!(sync.lanes.len(), 10);
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
        host.join(crate::place::rendezvous::RENDEZVOUS_FILE)
            .exists(),
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
    let root = std::env::temp_dir().join(format!("turnstone-place-refound-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let host = root.join("host");
    std::fs::create_dir_all(&host).unwrap();
    let founder = RootIdentity::from_seed([0xf2; 32]);
    let settings = crate::place::worker::PlaceWorkerSettings::default();

    // Exactly what the `Found` command leaves behind on disk, without the
    // live bind: this test is about what a LATER process finds there.
    let binding = crate::place::worker::found_place(&host, &founder, "Hearth", &settings).unwrap();
    crate::session::save_place_binding(&host, &binding).unwrap();
    crate::place::rendezvous::save_founder_rendezvous(&host, &binding).unwrap();

    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), Arc::new(founder), settings);
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Reconnect {
        contact: None,
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
            assert_eq!(sync.lanes.len(), 10);
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
    let root = std::env::temp_dir().join(format!("turnstone-place-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let host = root.join("host");
    let guest = root.join("guest");
    let founder = InMemoryProvider::from_seed([0xd1; 32]);
    let joiner = RootIdentity::from_seed([0xd2; 32]);
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
        Update::PlaceJoined { result: Ok(_), .. } => {},
        Update::PlaceJoined {
            result: Err(error), ..
        } => panic!("join refused: {error}"),
        _ => panic!("join answered with an unrelated update"),
    }
    assert!(
        guest
            .join(crate::place::rendezvous::RENDEZVOUS_FILE)
            .exists(),
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
            },
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
            },
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
    let deadline = Instant::now() + Duration::from_secs(30);
    let authored = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match updates.recv_timeout(left).expect("author answers") {
            // A resync the catch-up asked for can still answer first.
            Update::PlaceLanesAdvanced { .. } | Update::PlaceOpened { .. } => continue,
            authored => break authored,
        }
    };
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
    ack_rx.recv_timeout(ANSWER).unwrap();
    drop(_host_lanes);
    drop(host_open);
    let _ = std::fs::remove_dir_all(&root);
}
