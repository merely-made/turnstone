// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn a_revoked_member_reaches_no_projected_place_state() {
    let root = std::env::temp_dir().join(format!("turnstone-place-revoked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let directory = root.join("profile");
    let identity = RootIdentity::Local(InMemoryProvider::from_seed([0xa1; 32]));
    let binding = binding(0x51);
    seed_profile(&directory, &identity, &binding, 2);

    let (_, admitted) = open_cached_place(&directory, &binding, &identity, &settings()).unwrap();
    assert_eq!((admitted.graph.nodes, admitted.chat.messages), (2, 2));
    assert_eq!(
        admitted.permissions,
        Some(crate::place::PlacePermissionSnapshot {
            message_write: true,
            graph_write: true,
        })
    );
    assert_eq!(admitted.chat.channels, 1);
    assert_eq!(
        (
            admitted.graph.pending_authority,
            admitted.graph.revoked_authority,
            admitted.chat.pending_authority,
            admitted.chat.revoked_authority,
        ),
        (0, 0, 0, 0)
    );

    revoke_place_delegation(
        &directory,
        &binding,
        identity.master_public_key().to_bytes(),
    );

    let (mut withdrawn_open, withdrawn) =
        open_cached_place(&directory, &binding, &identity, &settings()).unwrap();
    assert_eq!(
        withdrawn.permissions,
        Some(crate::place::PlacePermissionSnapshot {
            message_write: false,
            graph_write: false,
        })
    );
    assert!(
        author_into_place(
            &mut withdrawn_open,
            &binding,
            &identity,
            &PlaceCommand::SendMessage {
                channel: "hall".into(),
                body: "refused after revocation".into()
            },
            &settings(),
        )
        .unwrap_err()
        .contains("no effective capability to author here: its grant was revoked")
    );
    assert_eq!(
        withdrawn.standing,
        crate::place::PlaceStanding::GrantRevoked
    );
    drop(withdrawn_open);
    assert_eq!(
        (withdrawn.graph.nodes, withdrawn.graph.edges),
        (0, 0),
        "a withdrawn member's graph facts must not reach PlaceState"
    );
    assert_eq!(
        (
            withdrawn.chat.messages,
            withdrawn.chat.channels,
            withdrawn.chat.deleted_messages
        ),
        (0, 0, 0),
        "a withdrawn member's chat facts must not reach PlaceState"
    );
    assert!(withdrawn.graph.pending_authority == 0);
    assert!(withdrawn.chat.pending_authority == 0);
    // The facts stay retained and attributable: revocation withholds them
    // from the projection, it does not erase them.
    assert_eq!(withdrawn.graph.revoked_authority, 2);
    assert_eq!(withdrawn.chat.revoked_authority, 3);
    assert_eq!(withdrawn.moot.delegated_certificates, 1);
    let _ = std::fs::remove_dir_all(&root);
}

/// Vault lock ruling 35: a worker started while the identity is pending
/// holds what speaks as the profile until the root arrives, then runs it
/// in order. A release is answered at once and drops what was waiting.
#[test]
fn a_pending_worker_holds_commands_until_the_root_arrives() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x95; 32])));
    let binding = binding(0x45);
    seed_profile(&directory, identity.as_ref(), &binding, 1);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_pending_place_worker(wake, settings());
    let open = |session| PlaceWorkerCommand::Open {
        session,
        generation: 1,
        directory: directory.clone(),
        binding: binding.clone(),
    };
    let wait = std::time::Duration::from_millis(300);

    let released = SessionId::new();
    worker.command(open(released));
    let (ack, acked) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack));
    acked
        .recv_timeout(ANSWER)
        .expect("a release never waits on the vault");

    let session = SessionId::new();
    worker.command(open(session));
    assert!(
        updates.recv_timeout(wait).is_err(),
        "nothing opens while the identity is pending"
    );

    worker.command(PlaceWorkerCommand::AdoptIdentity(identity));
    match updates.recv_timeout(ANSWER).unwrap() {
        Update::PlaceOpened {
            session: opened,
            result: Ok(_),
            ..
        } => assert_eq!(opened, session, "the released session's open was dropped"),
        _ => panic!("expected the held open"),
    }
    assert!(updates.recv_timeout(wait).is_err(), "and it ran once");
}

#[test]
fn worker_sets_and_clears_an_exact_collection_selection() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x93; 32])));
    let binding = binding(0x43);
    seed_profile(&directory, identity.as_ref(), &binding, 1);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_place_worker(wake, identity, settings());
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 1,
        directory,
        binding,
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceOpened { result: Ok(_), .. }
    ));

    let requested = PlaceCollectionVersion {
        moot: PlaceId([0xfe; 32]),
        collection: PlaceCollectionId([7; 32]),
        frontier: Vec::new(),
        membership_commitment: [0; 32],
    };
    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 1,
        selection: Some(requested.clone()),
    });
    let selected = updates.recv_timeout(ANSWER).unwrap();
    let Update::PlaceCollectionSet {
        result: Ok(snapshot),
        ..
    } = selected
    else {
        panic!("collection selection did not return a snapshot");
    };
    assert!(snapshot.captured.groups.is_empty());
    assert!(snapshot.captured.rejected.is_empty());
    assert_eq!(
        snapshot.captured_selection,
        CapturedCollectionSelection::Collection {
            requested,
            status: CapturedCollectionSelectionStatus::ForeignMoot,
        }
    );

    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 1,
        selection: None,
    });
    let cleared = updates.recv_timeout(ANSWER).unwrap();
    assert!(matches!(
        cleared,
        Update::PlaceCollectionSet {
            result: Ok(OfflinePlaceSnapshot {
                captured_selection: CapturedCollectionSelection::AllEffective,
                ..
            }),
            ..
        }
    ));
}

#[test]
fn reconnect_refuses_removed_local_membership_before_dialing_and_keeps_cache() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let binding = binding(0x4c);
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x95; 32])));
    seed_profile(&directory, identity.as_ref(), &binding, 2);

    // This is retained membership state, not a claim about a remote
    // revocation that has not arrived. The founder removes this profile
    // before the worker tries to turn saved contact data into a live lane.
    let founder = founder_for(&binding);
    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(&directory).join("gemot"),
        MootId(binding.moot.0),
        settings().retention,
    ))
    .unwrap();
    pollster::block_on(moot.membership_store().author_for_identity(
        &founder,
        MootMembershipAction::Create {
            initial_members: vec![
                MootMember {
                    member: founder.master_public_key().to_bytes(),
                    access: MootAccessLevel::Manage,
                },
                MootMember {
                    member: identity.master_public_key().to_bytes(),
                    access: MootAccessLevel::Write,
                },
            ],
        },
    ))
    .unwrap();
    pollster::block_on(moot.membership_store().author_for_identity(
        &founder,
        MootMembershipAction::Remove {
            member: identity.master_public_key().to_bytes(),
        },
    ))
    .unwrap();
    drop(moot);

    let session = SessionId::new();
    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity.clone(), settings());
    worker.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session,
        generation: 1,
        directory: directory.clone(),
        binding: binding.clone(),
    });
    assert!(matches!(
        updates
            .recv_timeout(ANSWER)
            .unwrap(),
        Update::PlaceOpened {
            session: received_session,
            generation: 1,
            result: Err(ref error),
        } if received_session == session
            && error == "this identity is no longer a member in retained place state"
    ));

    let (_, cached) =
        open_cached_place(&directory, &binding, identity.as_ref(), &settings()).unwrap();
    assert_eq!((cached.graph.nodes, cached.chat.messages), (2, 2));
}

/// Reconnect pressed while the place is already live. The app bumps the
/// generation, so the worker closes the bind and reopens the same stores
/// inside one process: the reopen must not land in Degraded on a lock the
/// departed lanes are still letting go of.
#[test]
fn a_live_place_reconnects_into_a_new_generation_in_one_process() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("host");
    std::fs::create_dir_all(&directory).unwrap();
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0xc4; 32])));

    let (worker, updates) =
        spawn_place_worker(Arc::new(|| {}), identity, PlaceWorkerSettings::default());
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Found {
        session,
        generation: 1,
        directory: directory.clone(),
        name: "Hearth".into(),
    });
    let binding = match updates.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(Update::PlaceFounded {
            result: Ok((binding, snapshot)),
            ..
        }) => {
            assert!(snapshot.sync.is_some(), "founding binds lanes");
            binding
        },
        Ok(Update::PlaceFounded {
            result: Err(error), ..
        }) => panic!("founding refused: {error}"),
        _ => panic!("founding answered with an unrelated update"),
    };

    worker.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session,
        generation: 2,
        directory,
        binding,
    });
    match updates.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(Update::PlaceOpened {
            result: Ok(snapshot),
            generation: 2,
            ..
        }) => {
            let sync = snapshot.sync.expect("the reconnected place binds lanes");
            assert_eq!(sync.lanes.len(), 10);
        },
        Ok(Update::PlaceOpened {
            result: Err(error), ..
        }) => panic!("same-process reconnect refused: {error}"),
        _ => panic!("reconnect answered with an unrelated update"),
    }

    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .unwrap();
}

#[test]
fn stale_reconnect_cannot_disturb_a_newer_open_scope() {
    let root = tempfile::tempdir().unwrap();
    let stale_directory = root.path().join("stale");
    let current_directory = root.path().join("current");
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x96; 32])));
    let stale_binding = binding(0x4d);
    let current_binding = binding(0x4e);
    seed_profile(&stale_directory, identity.as_ref(), &stale_binding, 0);
    seed_profile(&current_directory, identity.as_ref(), &current_binding, 1);

    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings());
    let current_session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session: current_session,
        generation: 2,
        directory: current_directory,
        binding: current_binding,
    });
    assert!(matches!(
        updates
            .recv_timeout(ANSWER)
            .unwrap(),
        Update::PlaceOpened {
            session,
            generation: 2,
            result: Ok(_),
        } if session == current_session
    ));

    let stale_session = SessionId::new();
    worker.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session: stale_session,
        generation: 1,
        directory: stale_directory,
        binding: stale_binding,
    });
    assert!(matches!(
        updates
            .recv_timeout(ANSWER)
            .unwrap(),
        Update::PlaceOpened {
            session,
            generation: 1,
            result: Err(ref error),
        } if session == stale_session && error == "reconnect generation is stale"
    ));

    worker.command(PlaceWorkerCommand::Resync {
        session: current_session,
        generation: 2,
    });
    assert!(matches!(
        updates
            .recv_timeout(ANSWER)
            .unwrap(),
        Update::PlaceOpened {
            session,
            generation: 2,
            result: Ok(OfflinePlaceSnapshot {
                graph: GraphCache { nodes: 1, .. },
                ..
            }),
        } if session == current_session
    ));
}

#[test]
fn reconnect_for_a_released_generation_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let binding = binding(0x4f);
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x97; 32])));
    seed_profile(&directory, identity.as_ref(), &binding, 0);

    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings());
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 1,
        directory: directory.clone(),
        binding: binding.clone(),
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceOpened { result: Ok(_), .. }
    ));
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();

    worker.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session,
        generation: 1,
        directory,
        binding,
    });
    assert!(matches!(
        updates
            .recv_timeout(ANSWER)
            .unwrap(),
        Update::PlaceOpened {
            session: received_session,
            generation: 1,
            result: Err(ref error),
        } if received_session == session && error == "reconnect generation was already released"
    ));
}
