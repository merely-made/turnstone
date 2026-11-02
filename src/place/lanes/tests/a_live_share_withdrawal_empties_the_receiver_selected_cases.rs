// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    let joiner = RootIdentity::from_seed([0x9d; 32]);

    // Both profiles retain the same captured text up front. That lets the
    // receiver prove the selection filter, rather than merely showing
    // that a peer without the local document has no search result.
    let (host_settings, requested, selected_share) =
        seed_exact_collection(&host, &RootIdentity::from_seed(founder_seed), &b);
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
    converge_until(
        &worker,
        &updates,
        session,
        "receive host collection",
        |snapshot| {
            snapshot
                .collection_choices
                .iter()
                .any(|choice| choice.version == requested)
        },
    );

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
    ack_rx.recv_timeout(ANSWER).unwrap();
    drop(host_lanes);
    drop(host_open);
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
    let joiner = RootIdentity::from_seed([0xf2; 32]);
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
            rejoined.2.ops_received(),
            rejoined.2.sync_status(),
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
    ack_rx.recv_timeout(ANSWER).unwrap();
    drop(rejoined);
    drop(host_open);
    let _ = std::fs::remove_dir_all(&root);
}
