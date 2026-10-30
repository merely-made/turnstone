// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    let root =
        std::env::temp_dir().join(format!("turnstone-place-r0-epoch-{}", std::process::id()));
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
    let fixture_root = |seed: u8| Arc::new(RootIdentity::from_seed([seed; 32]));

    let (host_worker, host_updates) =
        spawn_place_worker(Arc::new(|| {}), fixture_root(0xb4), settings());
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
        spawn_place_worker(Arc::new(|| {}), fixture_root(0xb5), settings());
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
    let graph_before = lane_counters(&after_first, COMMONS_ENCRYPTED_GRAPH_LANE);

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
    let graph_after = lane_counters(&last, COMMONS_ENCRYPTED_GRAPH_LANE);

    // Key agreement with no lane involved: the founder's sealed session
    // seals, A's opens.
    let founder_group = load_group_session(&host, &founder, moot).unwrap();
    let joiner_group = load_group_session(&guest, &joiner, moot).unwrap();
    let probe = founder_group.seal_random(b"r0 probe").unwrap();
    let probe_opened = joiner_group.open(&probe);

    eprintln!(
        "R0 epochs on disk: founder founded {} (count {}), after A {} ({}), after B {} ({}); A joined {} ({}, join snapshot epochs {}); A's invite named {}, B's invite named {}",
        head(founded_epoch.0),
        founded_epoch.1,
        head(after_a_epoch.0),
        after_a_epoch.1,
        head(after_b_epoch.0),
        after_b_epoch.1,
        head(joiner_epoch.0),
        joiner_epoch.1,
        joined.group.epochs,
        hex_head(&a_invite_epoch),
        hex_head(&third_invite.expected_epoch),
    );
    eprintln!(
        "R0 founder live worker group cache: after message 1 epochs {} members {}; after message 2 epochs {} members {}; founder chat messages {} then {}",
        founder_first.group.epochs,
        founder_first.group.members,
        founder_second.group.epochs,
        founder_second.group.members,
        founder_first.chat.messages,
        founder_second.chat.messages,
    );
    eprintln!(
        "R0 A: message 1 counted by a snapshot taken within {} ms of the founder's answer (poll resolution); chat lane (rounds, ops) {:?} -> {:?}; graph lane {:?} -> {:?}",
        first_elapsed.as_millis(),
        chat_before,
        chat_after,
        graph_before,
        graph_after,
    );
    eprintln!(
        "R0 A after the window: messages {} pending_causality {} pending_authority {} revoked {} nodes {} group epochs {} members {}; message 2 at {:?}, node at {:?}",
        last.chat.messages,
        last.chat.pending_causality,
        last.chat.pending_authority,
        last.chat.revoked_authority,
        last.graph.nodes,
        last.group.epochs,
        last.group.members,
        message_at,
        node_at,
    );
    eprintln!(
        "R0 probe: founder-sealed opened by A: {:?}",
        probe_opened.as_ref().map(|bytes| bytes.len())
    );
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
    assert!(
        probe_opened.is_ok(),
        "A cannot open what the founder's session seals: {probe_opened:?}"
    );
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

/// Turnstone no longer derives the topic: the overlay it tags is
/// stickleback's export, and that export is the key the lane files a
/// frame under. If either drifts, members stop exchanging frames silently.
#[test]
fn the_group_key_overlay_names_the_topic_stickleback_stores_under() {
    use muniment::Backend;
    let identity = InMemoryProvider::from_seed([0x5a; 32]);
    let group = stickleback::GroupSessionId([0x5b; 32]);
    let lane =
        stickleback::GroupKeyLane::for_identity(muniment::MemoryBackend::new(), group, &identity)
            .unwrap();
    let (mut session, _) = stickleback::GroupSession::new(group, &identity).unwrap();
    let founded = session.create(&[]).unwrap();
    pollster::block_on(lane.author_dispatch(&founded)).unwrap();
    let exported = stickleback::group_key_sync_topic(group);
    let keys = pollster::block_on(lane.sync_store().backend().list("topic/")).unwrap();
    let expected = format!("topic/{}/", crate::place::hex32(&exported));
    assert!(
        !keys.is_empty() && keys.iter().all(|key| key.starts_with(&expected)),
        "{keys:?}"
    );
    assert_eq!(
        super::super::group_key_overlay(group.0),
        transport::sync_overlay_topic(exported),
        "the dialed overlay is the exported topic's",
    );
}
