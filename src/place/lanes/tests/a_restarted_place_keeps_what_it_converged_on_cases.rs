// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    let root = std::env::temp_dir().join(format!("turnstone-place-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let host = root.join("host");
    let guest = root.join("guest");
    let founder = InMemoryProvider::from_seed([0xa7; 32]);
    let joiner = RootIdentity::from_seed([0xa8; 32]);
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
    ack_rx.recv_timeout(ANSWER).unwrap();
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
            },
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
    assert!(
        reopened.sync.is_none(),
        "cached Open has no live lane status"
    );

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
        contact: None,
        session,
        generation: 3,
        directory: guest.clone(),
        binding: b.clone(),
    });
    let reconnect_deadline = Instant::now() + Duration::from_secs(30);
    let reconnected = loop {
        assert!(
            Instant::now() < reconnect_deadline,
            "reconnect answer deadline"
        );
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
        Some(10),
        "Reconnect reports all seven Gemot lanes plus graph, chat and group keys"
    );
    // Repeating the same lifecycle generation is idempotent and does not
    // tear down or duplicate the already-live lanes.
    restarted.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session,
        generation: 3,
        directory: guest.clone(),
        binding: b.clone(),
    });
    let duplicate_deadline = Instant::now() + Duration::from_secs(30);
    let duplicate = loop {
        assert!(
            Instant::now() < duplicate_deadline,
            "duplicate reconnect deadline"
        );
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
                request: 9,
                result: Ok(snapshot),
                ..
            }) => {
                assert_eq!(snapshot.chat.messages, converged.chat.messages + 2);
                break;
            },
            Ok(Update::PlaceCommandDone {
                request: 9,
                result: Err(error),
                ..
            }) => panic!("reconnected authoring refused: {error}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => panic!("author response channel closed: {error}"),
            _ => continue,
        }
    }

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        assert!(
            Instant::now() < deadline,
            "reconnected author never reached host"
        );
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
    ack_rx.recv_timeout(ANSWER).unwrap();
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
    let joiner = RootIdentity::from_seed([0xe2; 32]);
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
        },
        _ => panic!("author answered with an unrelated update"),
    }

    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();
    let _ = std::fs::remove_dir_all(&root);
}

/// Reconnect-in-place: a live bind closed in a resident process must
/// leave redb's process-wide lock free for the very next reopen. The
/// generation bump in the worker's Reconnect path does exactly this, so
/// any lag here lands the place in Degraded for a person who clicked once.
#[test]
fn a_closed_live_bind_releases_the_store_lock_for_a_reopen() {
    let root = std::env::temp_dir().join(format!("turnstone-place-relock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let host = root.join("host");
    std::fs::create_dir_all(&host).unwrap();
    let founder = RootIdentity::from_seed([0xb7; 32]);
    let settings = settings();

    let b = crate::place::worker::found_place(&host, &founder, "Hearth", &settings).unwrap();
    let (open, _) = open_cached_place(&host, &b, &founder, &settings).unwrap();
    let mut lanes = super::super::join_live(&open, &b, &founder, &[], None, None).unwrap();
    assert_eq!(lanes.ops_received().len(), 10);

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
            },
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
