// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    let dial = super::super::holder_dial(&lanes, &founder.ticket, &grant, &joiner, [23; 32])
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
        probe.disclosed_address().ends_with("field.knot") && probe.disclosed_address() != address,
        "the holder discloses its own local address, not the shared one"
    );
    assert_eq!(probe.source(), "# Field\n");

    let authored = "# Field\n\nThe visitor was here.\n";
    assert_eq!(
        probe
            .save(authored)
            .expect("the holder took the visited edit"),
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

    let member = || Arc::new(RootIdentity::from_seed([0x3b; 32]));
    let (guest_worker, guest_updates) = spawn_place_worker(Arc::new(|| {}), member(), settings());
    let session = SessionId::new();
    guest_worker.command(PlaceWorkerCommand::Join {
        session,
        generation: 1,
        directory: founder.guest.clone(),
        invite: founder.invite.clone(),
    });
    joined_snapshot(&guest_updates);
    let joined = converge_until(
        &guest_worker,
        &guest_updates,
        session,
        "writer catch-up",
        |s| s.moot.members == 2 && s.chat.channels == 1,
    );
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
    visit(
        &guest_worker,
        &guest_updates,
        session,
        &founder.guest,
        founder_root,
        1,
    )
    .expect("an admitted writer's dial is prepared");
    let seen = converge_until(
        &founder.worker,
        &founder.updates,
        founder.session,
        "the founder holds the writer's node",
        |s| s.graph.nodes == 1 && s.members.len() == 2,
    );
    assert!(
        seen.members
            .iter()
            .any(|entry| { entry.root == joiner_root && entry.access == PlaceMemberAccess::Write })
    );

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
    assert!(
        arrived < Duration::from_secs(10),
        "took {} ms",
        arrived.as_millis()
    );
    assert!(withdrawn.sync.is_some(), "still on its live lanes");
    let lines = status_of(&founder, withdrawn);
    for line in [
        "Membership: revoked",
        "Message permission: not effective locally at last refresh",
        "Shared graph permission: not effective locally at last refresh",
    ] {
        assert!(
            lines.iter().any(|row| row == line),
            "{line} missing: {lines:?}"
        );
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
        visit(
            &guest_worker,
            &guest_updates,
            session,
            &founder.guest,
            founder_root,
            2
        )
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
        contact: None,
        session,
        generation: 1,
        directory: founder.guest.clone(),
        binding: founder.binding.clone(),
    });
    match updates.recv_timeout(Duration::from_secs(30)) {
        Ok(Update::PlaceOpened {
            result: Err(error), ..
        }) => assert_eq!(
            error,
            "this identity is no longer a member in retained place state"
        ),
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
    const LIFETIME: Duration = Duration::from_secs(20);
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

    let member = || Arc::new(RootIdentity::from_seed([0x3d; 32]));
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
    assert_eq!(
        expired.standing,
        PlaceStanding::GrantExpired { at_ms: expires_at }
    );
    assert_eq!(expired.permissions, both_permissions(false));
    assert!(expired.sync.is_some(), "still on its live lanes");
    let expired_line = format!("Grant: expired at {}", crate::place::utc_ms(expires_at));
    let lines = status_of(&founder, expired);
    for line in [
        expired_line.as_str(),
        "Message permission: not effective locally at last refresh",
        "Shared graph permission: not effective locally at last refresh",
    ] {
        assert!(
            lines.iter().any(|row| row == line),
            "{line} missing: {lines:?}"
        );
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
        visit(
            &guest_worker,
            &guest_updates,
            session,
            &founder.guest,
            founder_root,
            2
        )
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
        contact: None,
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
