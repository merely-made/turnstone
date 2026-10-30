// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// R2 of the co-op lifecycle parity plan: the founder revokes B while A and
/// B are live. A applies the removal and rotation from the group-key lane
/// on a nudge; the founder's running worker and A both author to the new
/// epoch and read each other; B, which read chat before the revocation,
/// cannot read anything after it. Then C, invited after the rotation,
/// reads chat authored after its admission, and A reads new chat after a
/// restart.
#[test]
fn a_revoked_member_stops_reading_while_the_others_follow_the_rotation() {
    use crate::place::PlaceStanding;
    let root =
        std::env::temp_dir().join(format!("turnstone-place-r2-rekey-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let [host, a_dir, b_dir, c_dir] = ["host", "a", "b", "c"].map(|name| root.join(name));
    for directory in [&host, &a_dir, &b_dir, &c_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    const F: u8 = 0xc4;
    const A: u8 = 0xc5;
    const B: u8 = 0xc6;
    const C: u8 = 0xc7;
    let root_of = |seed: u8| {
        InMemoryProvider::from_seed([seed; 32])
            .master_public_key()
            .to_bytes()
    };

    let (founder, f_session, binding) = found_live(&host, F);
    let moot = binding.moot.0;
    let channel = binding.default_channel.clone();

    let invite = invite_writer(&founder, f_session, &host, &a_dir, A, moot);
    let (a_worker, a_updates, a_session) = join_member(&a_dir, A, invite);
    let invite = invite_writer(&founder, f_session, &host, &b_dir, B, moot);
    let (b_worker, b_updates, b_session) = join_member(&b_dir, B, invite);
    // A was joined before B's add: three group members means A applied
    // that add from the group-key lane, not from any invitation.
    let a_caught_up = converge_until(&a_worker, &a_updates, a_session, "A sees B's add", |s| {
        s.moot.members == 3 && s.group.members == 3 && s.chat.channels == 1
    });
    report_lanes("R2 A before revocation", &a_caught_up);

    // Positive control: B reads chat before the revocation.
    send_message(
        &founder.0,
        &founder.1,
        f_session,
        1,
        &channel,
        "before revocation",
    );
    converge_until(&a_worker, &a_updates, a_session, "A reads message 0", |s| {
        s.chat.messages == 1
    });
    let b_before = converge_until(&b_worker, &b_updates, b_session, "B reads message 0", |s| {
        s.chat.messages == 1
    });
    let b_epochs = b_before.group.epochs;

    founder.0.command(PlaceWorkerCommand::Author {
        session: f_session,
        generation: 1,
        request: 2,
        command: PlaceCommand::RevokeMember { member: root_of(B) },
    });
    let revoked = authored(&founder.1, 2).expect("the founder revokes B");
    let revoked_at = Instant::now();
    assert_eq!(
        revoked.group.members, 2,
        "B's recipient left the founder's group"
    );

    // A drains the removal and rotation on a watcher nudge alone.
    let (a_rotated, nudges) = until_nudged(
        &a_worker,
        &a_updates,
        a_session,
        Duration::from_secs(10),
        |s| s.group.members == 2 && s.group.epochs == revoked.group.epochs,
    )
    .unwrap_or_else(|| panic!("A never applied the rotation within 10 s"));
    let drain_latency = revoked_at.elapsed();
    eprintln!(
        "R2 drain: A applied removal and rotation {} ms after the revoke answered, over {nudges} nudge(s); founder epochs {}, A epochs {}, A pending {} refused {}",
        drain_latency.as_millis(),
        revoked.group.epochs,
        a_rotated.group.epochs,
        a_rotated.group.pending_frames,
        a_rotated.group.refused_frames,
    );
    assert!(nudges > 0);
    assert_eq!(
        (
            a_rotated.group.pending_frames,
            a_rotated.group.refused_frames
        ),
        (0, 0)
    );

    // The founder's running worker and A both author after the rotation.
    send_message(
        &founder.0,
        &founder.1,
        f_session,
        3,
        &channel,
        "rotated: founder",
    );
    send_message(&a_worker, &a_updates, a_session, 1, &channel, "rotated: A");
    converge_until(
        &founder.0,
        &founder.1,
        f_session,
        "the founder reads A",
        |s| s.chat.messages == 3,
    );
    converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "A reads the founder",
        |s| s.chat.messages == 3,
    );

    // C, invited after the rotation. Its admission is also B's connection
    // control: a plaintext Gemot fact authored after both messages.
    let invite = invite_writer(&founder, f_session, &host, &c_dir, C, moot);
    let (c_worker, c_updates, c_session) = join_member(&c_dir, C, invite);
    let b_control = converge_until(&b_worker, &b_updates, b_session, "B sees C admitted", |s| {
        s.members.iter().any(|member| member.root == root_of(C))
    });
    eprintln!(
        "R2 B after C's admission: standing {:?}, messages {}, group epochs {} (before {b_epochs}), pending {} refused {}",
        b_control.standing,
        b_control.chat.messages,
        b_control.group.epochs,
        b_control.group.pending_frames,
        b_control.group.refused_frames,
    );
    assert_eq!(b_control.standing, PlaceStanding::MembershipRevoked);
    let b_status = crate::place::PlaceState::Offline {
        binding: binding.clone(),
        generation: 1,
        snapshot: b_control.clone(),
    }
    .status_lines();
    assert!(
        b_status.iter().any(|row| row == "Membership: revoked"),
        "{b_status:?}"
    );
    assert!(
        !b_status.iter().any(|row| row.starts_with("Reading:")),
        "{b_status:?}"
    );
    assert_eq!(b_control.chat.messages, 1, "B counted post-rotation chat");
    assert_eq!(
        b_control.group.epochs, b_epochs,
        "B installed a post-removal epoch"
    );

    send_message(
        &founder.0,
        &founder.1,
        f_session,
        4,
        &channel,
        "after C joined",
    );
    let c_read = converge_until(&c_worker, &c_updates, c_session, "C reads new chat", |s| {
        s.chat.messages == 4
    });
    assert_eq!(
        c_read.group.epochs, revoked.group.epochs,
        "C holds every retained epoch"
    );
    converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "A reads the message after C",
        |s| s.group.members == 3 && s.chat.messages == 4,
    );

    // A restarts after the rotation and still reads new chat.
    release(&a_worker);
    drop(reopen_by_hand(
        &a_dir,
        &binding,
        &InMemoryProvider::from_seed([A; 32]),
    ));
    let (a_worker, a_updates) = spawn_place_worker(
        Arc::new(|| {}),
        Arc::new(RootIdentity::from_seed([A; 32])),
        settings(),
    );
    a_worker.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session: a_session,
        generation: 1,
        directory: a_dir.clone(),
        binding: binding.clone(),
    });
    converge_until(&a_worker, &a_updates, a_session, "A reconnects", |s| {
        s.sync.is_some()
    });
    send_message(
        &founder.0,
        &founder.1,
        f_session,
        5,
        &channel,
        "after A restarted",
    );
    converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "restarted A reads new chat",
        |s| s.chat.messages == 5,
    );

    for worker in [&founder.0, &a_worker, &b_worker, &c_worker] {
        release(worker);
    }
    let a_open = reopen_by_hand(&a_dir, &binding, &InMemoryProvider::from_seed([A; 32]));
    let b_open = reopen_by_hand(&b_dir, &binding, &InMemoryProvider::from_seed([B; 32]));
    let a_bodies = chat_bodies(&a_open);
    let b_bodies = chat_bodies(&b_open);
    eprintln!("R2 retained bodies: A {a_bodies:?}; B {b_bodies:?}");
    const AFTER: [&str; 4] = [
        "rotated: founder",
        "rotated: A",
        "after C joined",
        "after A restarted",
    ];
    assert!(
        b_bodies.iter().any(|body| body == "before revocation"),
        "positive control"
    );
    for body in AFTER {
        assert!(a_bodies.iter().any(|held| held == body), "A lacks {body}");
        assert!(!b_bodies.iter().any(|held| held == body), "B read {body}");
    }
    // The founder's post-rotation message, offered to B directly.
    let sealed = pollster::block_on(a_open.chat.projection())
        .unwrap()
        .messages
        .into_iter()
        .find(|message| message.message.body == "rotated: founder")
        .unwrap()
        .operation;
    let operation = pollster::block_on(a_open.chat.sync_store().get_operation(&sealed.into()))
        .unwrap()
        .expect("A retains the founder's message");
    // Parked, not refused: the record is kept whole against the epoch
    // arriving later. B never receives that epoch, so re-admission on B's
    // own handle admits nothing and the projection never shows it.
    assert!(
        !pollster::block_on(b_open.chat.accept(&operation)).unwrap(),
        "a parked record is not accepted",
    );
    // B's live lanes already parked every post-rotation message, this one
    // among them, so the hand-offered copy lands in the slot it holds.
    let parked = pollster::block_on(b_open.chat.parking_status()).unwrap();
    eprintln!("R2 B parks the founder's post-rotation messages: {parked:?}");
    assert_eq!(parked.parked, AFTER.len() as u64);
    let report = pollster::block_on(b_open.chat.readmit_parked()).unwrap();
    assert_eq!(
        (report.admitted, report.still_parked),
        (0, parked.parked),
        "{report:?}"
    );
    assert!(
        !chat_bodies(&b_open)
            .iter()
            .any(|body| body == "rotated: founder")
    );

    // The other half of the same mechanism, on the same parked records:
    // hand B the epoch A holds -- which no revoked member can obtain, so
    // this is the test reaching in -- and they become readable at the next
    // re-admission, not before.
    b_open
        .group
        .keys
        .replace_from_bytes(&a_open.group.session.data_keyring_state().unwrap())
        .unwrap();
    assert!(
        !chat_bodies(&b_open)
            .iter()
            .any(|body| body == "rotated: founder")
    );
    let report = pollster::block_on(b_open.chat.readmit_parked()).unwrap();
    assert_eq!(
        (report.admitted, report.still_parked),
        (parked.parked, 0),
        "{report:?}"
    );
    let readmitted = chat_bodies(&b_open);
    for body in AFTER {
        assert!(
            readmitted.iter().any(|held| held == body),
            "{body} stayed parked"
        );
    }
    drop((a_open, b_open));
    let _ = std::fs::remove_dir_all(&root);
}

/// E2 of the co-op lifecycle parity plan: after the founder revokes B, a
/// node the founder shares is admitted and projected by A and refused by
/// B, which projected the node shared before the revocation. C, invited
/// once both nodes exist, reads the graph history, the pre-rotation node
/// included, through the epochs its welcome carried.
#[test]
fn a_revoked_member_cannot_read_shared_nodes_authored_after_the_rotation() {
    let root =
        std::env::temp_dir().join(format!("turnstone-place-e2-graph-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let [host, a_dir, b_dir, c_dir] = ["host", "a", "b", "c"].map(|name| root.join(name));
    for directory in [&host, &a_dir, &b_dir, &c_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    const F: u8 = 0xd4;
    const A: u8 = 0xd5;
    const B: u8 = 0xd6;
    const C: u8 = 0xd7;
    const BEFORE: &str = "https://before.example/node";
    const AFTER: &str = "https://after.example/node";
    let root_of = |seed: u8| {
        InMemoryProvider::from_seed([seed; 32])
            .master_public_key()
            .to_bytes()
    };

    let (founder, f_session, binding) = found_live(&host, F);
    let moot = binding.moot.0;
    let invite = invite_writer(&founder, f_session, &host, &a_dir, A, moot);
    let (a_worker, a_updates, a_session) = join_member(&a_dir, A, invite);
    let invite = invite_writer(&founder, f_session, &host, &b_dir, B, moot);
    let (b_worker, b_updates, b_session) = join_member(&b_dir, B, invite);

    share_node(&founder, f_session, 1, BEFORE);
    converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "A projects the first node",
        |s| s.graph.nodes == 1,
    );
    // Positive control: B projects a node shared before its revocation.
    let b_before = converge_until(
        &b_worker,
        &b_updates,
        b_session,
        "B projects the first node",
        |s| s.graph.nodes == 1,
    );
    assert_eq!(addresses(&b_before), [BEFORE]);

    founder.0.command(PlaceWorkerCommand::Author {
        session: f_session,
        generation: 1,
        request: 2,
        command: PlaceCommand::RevokeMember { member: root_of(B) },
    });
    let revoked = authored(&founder.1, 2).expect("the founder revokes B");
    converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "A applies the rotation",
        |s| s.group.members == 2 && s.group.epochs == revoked.group.epochs,
    );

    let shared_at = Instant::now();
    share_node(&founder, f_session, 3, AFTER);
    let a_after = converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "A projects the second node",
        |s| s.graph.nodes == 2,
    );
    eprintln!(
        "E2 A projected the post-rotation node {} ms after it was shared: {:?}",
        shared_at.elapsed().as_millis(),
        addresses(&a_after)
    );

    // C arrives after both nodes exist. Its admission is B's connection
    // control, authored after the second node.
    let invite = invite_writer(&founder, f_session, &host, &c_dir, C, moot);
    let (c_worker, c_updates, c_session) = join_member(&c_dir, C, invite);
    let c_read = converge_until(
        &c_worker,
        &c_updates,
        c_session,
        "C reads the graph history",
        |s| s.graph.nodes == 2,
    );
    eprintln!(
        "E2 C's graph after joining: {:?}, group epochs {}",
        addresses(&c_read),
        c_read.group.epochs
    );
    assert_eq!(addresses(&c_read), addresses(&a_after));
    assert!(addresses(&c_read).iter().any(|address| address == BEFORE));
    let b_control = converge_until(&b_worker, &b_updates, b_session, "B sees C admitted", |s| {
        s.members.iter().any(|member| member.root == root_of(C))
    });
    eprintln!("E2 B after C's admission: {:?}", addresses(&b_control));
    assert_eq!(
        addresses(&b_control),
        [BEFORE],
        "B projected a post-rotation node"
    );

    for worker in [&founder.0, &a_worker, &b_worker, &c_worker] {
        release(worker);
    }
    let a_open = reopen_by_hand(&a_dir, &binding, &InMemoryProvider::from_seed([A; 32]));
    let b_open = reopen_by_hand(&b_dir, &binding, &InMemoryProvider::from_seed([B; 32]));
    let b_projection = pollster::block_on(b_open.graph.projection()).unwrap();
    let b_shared = crate::place::projection::SharedGraph::from_projection(&b_projection);
    assert_eq!(b_shared.addresses().collect::<Vec<_>>(), [BEFORE]);
    // The node B never stored, offered to it directly.
    let unseen: Vec<[u8; 32]> =
        pollster::block_on(a_open.graph.projection_with_authority(&EveryOperation))
            .unwrap()
            .revoked
            .into_iter()
            .map(|operation| operation.operation)
            .filter(|id| {
                !pollster::block_on(b_open.graph.sync_store().has_operation(&(*id).into())).unwrap()
            })
            .collect();
    assert_eq!(
        unseen.len(),
        1,
        "A holds exactly one node operation B lacks"
    );
    let operation = pollster::block_on(a_open.graph.sync_store().get_operation(&unseen[0].into()))
        .unwrap()
        .expect("A retains the post-rotation node");
    // Parked, not refused, and B never gets the epoch that would release
    // it: re-admission admits nothing and the projection stays at BEFORE.
    // Sorted: the shared graph is a set, not an insertion order.
    let shared_addresses = |open: &crate::place::worker::OpenPlace| {
        let projection = pollster::block_on(open.graph.projection()).unwrap();
        let mut addresses: Vec<String> =
            crate::place::projection::SharedGraph::from_projection(&projection)
                .addresses()
                .map(str::to_string)
                .collect();
        addresses.sort();
        addresses
    };
    assert!(
        !pollster::block_on(b_open.graph.accept(&operation)).unwrap(),
        "a parked record is not accepted",
    );
    let parked = pollster::block_on(b_open.graph.parking_status()).unwrap();
    eprintln!("E2 B parks the post-rotation node: {parked:?}");
    assert_eq!(parked.parked, 1);
    let report = pollster::block_on(b_open.graph.readmit_parked()).unwrap();
    assert_eq!((report.admitted, report.still_parked), (0, 1), "{report:?}");
    assert_eq!(shared_addresses(&b_open), [BEFORE]);

    // Hand B the epoch A holds -- which no revoked member can obtain, so
    // this is the test reaching in -- and the same parked record projects
    // at the next re-admission, not before.
    b_open
        .group
        .keys
        .replace_from_bytes(&a_open.group.session.data_keyring_state().unwrap())
        .unwrap();
    assert_eq!(shared_addresses(&b_open), [BEFORE]);
    let report = pollster::block_on(b_open.graph.readmit_parked()).unwrap();
    assert_eq!((report.admitted, report.still_parked), (1, 0), "{report:?}");
    assert_eq!(shared_addresses(&b_open), [AFTER, BEFORE]);
    drop((a_open, b_open));
    let _ = std::fs::remove_dir_all(&root);
}

/// R2b: a Gemot member the group session cannot resolve to a recipient is
/// refused by name. Revoking it in Gemot alone would leave it reading,
/// which is the silent half-revoke this refusal replaces.
#[test]
fn a_revoke_with_no_group_recipient_is_refused_and_changes_nothing() {
    let root = std::env::temp_dir().join(format!(
        "turnstone-place-r2b-norecipient-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let host = root.join("host");
    std::fs::create_dir_all(&host).unwrap();
    const F: u8 = 0xe4;
    let stranger = InMemoryProvider::from_seed([0xe5; 32])
        .master_public_key()
        .to_bytes();

    let (founder, f_session, binding) = found_live(&host, F);
    release(&founder.0);

    // A Gemot member the group never welcomed: admitted straight on the
    // membership lane, with no pre-key registered and so no recipient.
    let founder_identity = InMemoryProvider::from_seed([F; 32]);
    // The released worker's store lock outlives its ack under load.
    let deadline = Instant::now() + Duration::from_secs(30);
    let moot_file = loop {
        match pollster::block_on(MootFile::open_existing(
            place_store_dir(&host).join("gemot"),
            MootId(binding.moot.0),
            settings().retention,
        )) {
            Ok(moot_file) => break moot_file,
            Err(error) if Instant::now() < deadline => {
                eprintln!("R2b waiting for the Gemot store: {error}");
                std::thread::sleep(Duration::from_millis(50));
            },
            Err(error) => panic!("open the founder's Gemot: {error}"),
        }
    };
    pollster::block_on(moot_file.membership_store().author_for_identity(
        &founder_identity,
        MootMembershipAction::Add {
            member: stranger,
            access: MootAccessLevel::Write,
        },
    ))
    .unwrap();
    drop(moot_file);

    let (founder, updates) = spawn_place_worker(
        Arc::new(|| {}),
        Arc::new(RootIdentity::from_seed([F; 32])),
        settings(),
    );
    founder.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session: f_session,
        generation: 1,
        directory: host.clone(),
        binding: binding.clone(),
    });
    let before = converge_until(
        &founder,
        &updates,
        f_session,
        "the stranger is a member",
        |s| s.members.iter().any(|member| member.root == stranger),
    );

    founder.command(PlaceWorkerCommand::Author {
        session: f_session,
        generation: 1,
        request: 1,
        command: PlaceCommand::RevokeMember { member: stranger },
    });
    let refusal = authored(&updates, 1).expect_err("a revoke with no recipient must refuse");
    eprintln!("R2b revoke refusal: {refusal}");
    assert_eq!(refusal, crate::place::worker::NO_GROUP_RECIPIENT);

    // Nothing moved: Gemot still holds the member and the group still
    // holds its epoch, so no half-revoke was left behind.
    let after = converge_until(&founder, &updates, f_session, "nothing changed", |s| {
        s.moot.membership_epoch == before.moot.membership_epoch
    });
    assert!(after.members.iter().any(|member| member.root == stranger));
    assert_eq!(after.group.epochs, before.group.epochs);
    release(&founder);
    let _ = std::fs::remove_dir_all(&root);
}
