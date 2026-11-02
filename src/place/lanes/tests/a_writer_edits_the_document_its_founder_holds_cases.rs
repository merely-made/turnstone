// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
        super::super::dial_holder(&lanes, &founder.ticket, &grant, &joiner, [21; 32])
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
            ResolvedContent::EditableText(editable) if editable.address.ends_with("field.knot") => {
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
        super::super::dial_holder(&lanes, &founder.ticket, &grant, &joiner, [31; 32])
            .expect("the founder admits this writer");
    let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
        .expect("discover the holder endpoint");
    retained.mount(0).expect("mount the projected vault");

    // Two watch ticks to see the change and two to confirm it settled,
    // plus room for the scheduler: the budget the watcher itself sets.
    let budget = super::super::WATCH_TICK * 8;
    let arrived = next_nudge(&founder, budget)
        .expect("an admitted projection session nudges the app like a lane arrival");
    eprintln!("nudge: arrival reported after {} ms", arrived.as_millis());
    let serving = founder_projection(&founder).expect("the founder still serves");
    assert_eq!(
        serving.live_sessions, 1,
        "and the snapshot says one visitor"
    );
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
    let dial = super::super::holder_dial(&lanes, &founder.ticket, &grant, &joiner, [32; 32])
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
    let dial = super::super::holder_dial(&lanes, &founder.ticket, &grant, &joiner, [33; 32])
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

    let founder = RootIdentity::from_seed([seed; 32]);
    let host_settings = crate::place::worker::PlaceWorkerSettings {
        knot_root: Some(vault.clone()),
        ..clocked.clone()
    };
    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), Arc::new(founder), host_settings);
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
    let lanes = super::super::join_live(
        &open,
        &binding,
        &joiner,
        std::slice::from_ref(&ticket),
        None,
        None,
    )
    .expect("the joiner binds its lanes");
    let grant = crate::place::rendezvous::load_projection_grant(&guest)
        .expect("admission stored the grant beside the rendezvous");

    let dialed = Instant::now();
    let (carrier, _holder) = super::super::dial_holder(&lanes, &ticket, &grant, &joiner, [24; 32])
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
