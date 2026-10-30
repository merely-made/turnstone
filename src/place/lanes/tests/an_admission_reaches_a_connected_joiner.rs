// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn an_admission_reaches_a_connected_joiner(access: crate::place::PlaceInviteAccess) {
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

    let founder = RootIdentity::from_seed([0xa1; 32]);
    let joiner = RootIdentity::from_seed([0xa2; 32]);
    let third = RootIdentity::from_seed([0xa3; 32]);

    // The founder: `Found` opens the place and binds its ten lanes
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
                assert!(
                    !sync.local_rendezvous.is_empty(),
                    "the founder has a ticket"
                );
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
        |s| {
            // A quiet lane with zero rounds has not caught up yet. Wait
            // for every initial reconciliation before freezing counters.
            s.moot.members == 2
                    && s.moot.delegated_certificates == 2 // founder and invited writer
                    && s.chat.channels == 1
                    && s.sync.as_ref().is_some_and(|sync| {
                        sync.lanes.len() == 10
                            && sync
                                .lanes
                                .iter()
                                .all(|lane| lane.sync_rounds > 0 && !lane.syncing)
                    })
        },
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

    for (worker, updates) in [
        (&host_worker, &host_updates),
        (&guest_worker, &guest_updates),
    ] {
        let _ = updates;
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(30));
    }
    let _ = std::fs::remove_dir_all(&root);
}
