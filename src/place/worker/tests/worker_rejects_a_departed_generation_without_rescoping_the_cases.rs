// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn worker_rejects_a_departed_generation_without_rescoping_the_open_place() {
    let root = tempfile::tempdir().unwrap();
    let first_directory = root.path().join("first");
    let second_directory = root.path().join("second");
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x94; 32])));
    let first_binding = binding(0x47);
    let second_binding = binding(0x48);
    seed_profile(&first_directory, identity.as_ref(), &first_binding, 0);
    seed_profile(&second_directory, identity.as_ref(), &second_binding, 0);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_place_worker(wake, identity, settings());
    let first_session = SessionId::new();
    let second_session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session: first_session,
        generation: 1,
        directory: first_directory,
        binding: first_binding.clone(),
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceOpened { result: Ok(_), .. }
    ));
    worker.command(PlaceWorkerCommand::Open {
        session: second_session,
        generation: 2,
        directory: second_directory,
        binding: second_binding,
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceOpened { result: Ok(_), .. }
    ));

    worker.command(PlaceWorkerCommand::SetCollection {
        session: first_session,
        generation: 1,
        selection: Some(PlaceCollectionVersion {
            moot: first_binding.moot,
            collection: PlaceCollectionId([0x49; 32]),
            frontier: Vec::new(),
            membership_commitment: [0x4a; 32],
        }),
    });
    assert!(matches!(
        updates
            .recv_timeout(ANSWER)
            .unwrap(),
        Update::PlaceCollectionSet {
            session,
            generation: 1,
            result: Err(ref error),
        } if session == first_session
            && error == "collection selection belongs to a departed place generation"
    ));

    worker.command(PlaceWorkerCommand::Resync {
        session: second_session,
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
                captured_selection: CapturedCollectionSelection::AllEffective,
                ..
            }),
        } if session == second_session
    ));
}

#[test]
fn worker_exact_collection_version_returns_only_its_ready_capture() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let binding = binding(0x44);
    let identity = Arc::new(RootIdentity::Local(founder_for(&binding)));
    let (settings, requested, selected_share) =
        seed_exact_collection(&directory, identity.as_ref(), &binding);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_place_worker(wake, identity, settings);
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
    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 1,
        selection: Some(requested.clone()),
    });
    let Update::PlaceCollectionSet {
        result: Ok(snapshot),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("exact selection did not return a snapshot");
    };
    assert_eq!(snapshot.captured.groups.len(), 1);
    assert!(snapshot.captured.rejected.is_empty());
    assert_eq!(snapshot.captured.groups[0].contributions.len(), 1);
    assert_eq!(
        snapshot.captured.groups[0].contributions[0].share_operation,
        selected_share
    );
    assert_eq!(
        snapshot.captured_selection,
        CapturedCollectionSelection::Collection {
            requested: requested.clone(),
            status: CapturedCollectionSelectionStatus::Ready {
                name: "Field notes".into(),
                effective_contributions: 1,
                pending_facts: 0,
            },
        }
    );
    assert_eq!(
        snapshot.collection_choices,
        vec![PlaceCollectionChoice {
            name: "Field notes".into(),
            version: requested,
        }]
    );
    // Same-handle refresh and a complete release/reopen retain the exact
    // chosen version before exposing the first snapshot.
    worker.command(PlaceWorkerCommand::Resync {
        session,
        generation: 1,
    });
    let Update::PlaceOpened {
        result: Ok(resynced),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("selection did not resync");
    };
    assert_eq!(resynced.captured_selection, snapshot.captured_selection);
    assert_eq!(resynced.captured, snapshot.captured);
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 2,
        directory: directory.clone(),
        binding: binding.clone(),
    });
    let Update::PlaceOpened {
        result: Ok(reopened),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("saved selection did not reopen");
    };
    assert_eq!(reopened.captured_selection, snapshot.captured_selection);
    assert_eq!(reopened.captured, snapshot.captured);
    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 2,
        selection: None,
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceCollectionSet { result: Ok(_), .. }
    ));
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 3,
        directory,
        binding,
    });
    let Update::PlaceOpened {
        result: Ok(cleared),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("cleared selection did not reopen");
    };
    assert_eq!(
        cleared.captured_selection,
        CapturedCollectionSelection::AllEffective
    );
    assert_eq!(cleared.captured.groups.len(), 2);
}

#[test]
fn worker_collection_save_failure_keeps_the_previous_scope() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let binding = binding(0x4b);
    let identity = Arc::new(RootIdentity::Local(founder_for(&binding)));
    let (settings, requested, _) = seed_exact_collection(&directory, identity.as_ref(), &binding);
    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings);
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 1,
        directory: directory.clone(),
        binding,
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceOpened { result: Ok(_), .. }
    ));
    // Replacing a directory with a file must fail on every supported host.
    std::fs::create_dir(directory.join(crate::session::PLACE_COLLECTION_FILE)).unwrap();
    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 1,
        selection: Some(requested),
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceCollectionSet { result: Err(_), .. }
    ));
    worker.command(PlaceWorkerCommand::Resync {
        session,
        generation: 1,
    });
    let Update::PlaceOpened {
        result: Ok(snapshot),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("prior scope did not survive a failed save");
    };
    assert_eq!(
        snapshot.captured_selection,
        CapturedCollectionSelection::AllEffective
    );
    assert_eq!(snapshot.captured.groups.len(), 2);
}

#[test]
fn worker_stale_collection_version_withholds_available_capture() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let binding = binding(0x45);
    let founder = founder_for(&binding);
    let identity = Arc::new(RootIdentity::Local(founder_for(&binding)));
    let (settings, requested, selected_share) =
        seed_exact_collection(&directory, identity.as_ref(), &binding);

    crate::session::save_place_collection(&directory, Some(&requested)).unwrap();

    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(&directory).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .unwrap();
    let current = pollster::block_on(
        moot.authorized_collection(CollectionId(requested.collection.0), AUTHORITY_AT_MS),
    )
    .unwrap()
    .unwrap();
    pollster::block_on(moot.set_collection_membership_for_identity(
        &founder,
        current.collection,
        current.heads,
        CollectionChange::SetMembership {
            contribution: ContributionRef {
                moot_id: binding.moot.0,
                share: selected_share,
            },
            included: false,
        },
        30,
    ))
    .unwrap();
    drop(moot);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_place_worker(wake, identity, settings);
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 1,
        directory,
        binding,
    });
    let Update::PlaceOpened {
        result: Ok(all_effective),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("place did not open");
    };
    assert!(all_effective.captured.groups.is_empty());
    assert!(all_effective.captured.rejected.is_empty());
    assert!(matches!(&all_effective.captured_selection,
            CapturedCollectionSelection::Collection {
                requested: retained, status: CapturedCollectionSelectionStatus::Stale { .. },
            } if retained == &requested));
    assert_eq!(all_effective.collection_choices.len(), 1);
    assert_ne!(all_effective.collection_choices[0].version, requested);

    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 1,
        selection: Some(requested.clone()),
    });
    let Update::PlaceCollectionSet {
        result: Ok(snapshot),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("stale selection did not return a snapshot");
    };
    assert!(snapshot.captured.groups.is_empty());
    assert!(snapshot.captured.rejected.is_empty());
    let CapturedCollectionSelection::Collection {
        requested: retained,
        status: CapturedCollectionSelectionStatus::Stale { current },
    } = snapshot.captured_selection
    else {
        panic!("advanced collection history did not report stale");
    };
    assert_eq!(retained, requested);
    assert_ne!(current, requested);
}

#[test]
fn worker_authority_shrink_keeps_the_exact_version_and_withholds_its_share() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let binding = binding(0x46);
    let identity = Arc::new(RootIdentity::Local(founder_for(&binding)));
    let (settings, requested, selected_share) =
        seed_exact_collection(&directory, identity.as_ref(), &binding);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_place_worker(wake, identity, settings.clone());
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 1,
        directory: directory.clone(),
        binding: binding.clone(),
    });
    let Update::PlaceOpened {
        result: Ok(all_effective),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("place did not open before the withdrawal");
    };
    assert_eq!(
        all_effective.captured.groups.len(),
        2,
        "both effective captures are initially visible"
    );
    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 1,
        selection: Some(requested.clone()),
    });
    assert!(matches!(
        updates.recv_timeout(ANSWER).unwrap(),
        Update::PlaceCollectionSet {
            result: Ok(OfflinePlaceSnapshot {
                captured_selection: CapturedCollectionSelection::Collection {
                    status: CapturedCollectionSelectionStatus::Ready {
                        effective_contributions: 1,
                        ..
                    },
                    ..
                },
                ..
            }),
            ..
        }
    ));

    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();
    let founder = founder_for(&binding);
    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(&directory).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .unwrap();
    pollster::block_on(moot.withdraw_share_for_identity(&founder, selected_share, 30)).unwrap();
    drop(moot);

    worker.command(PlaceWorkerCommand::Open {
        session,
        generation: 2,
        directory,
        binding,
    });
    let Update::PlaceOpened {
        result: Ok(all_effective),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("place did not reopen after the withdrawal");
    };
    assert_eq!(
        all_effective.captured.groups.len(),
        0,
        "the restored selection must not expose the other capture"
    );
    assert!(matches!(&all_effective.captured_selection,
            CapturedCollectionSelection::Collection { requested: retained, .. }
                if retained == &requested));
    worker.command(PlaceWorkerCommand::SetCollection {
        session,
        generation: 2,
        selection: Some(requested.clone()),
    });
    let Update::PlaceCollectionSet {
        result: Ok(snapshot),
        ..
    } = updates.recv_timeout(ANSWER).unwrap()
    else {
        panic!("post-withdrawal selection did not return a snapshot");
    };
    assert!(snapshot.captured.groups.is_empty());
    assert!(snapshot.captured.rejected.is_empty());
    assert_eq!(
        snapshot.captured_selection,
        CapturedCollectionSelection::Collection {
            requested,
            status: CapturedCollectionSelectionStatus::Ready {
                name: "Field notes".into(),
                effective_contributions: 0,
                pending_facts: 1,
            },
        },
        "fauna authority changes effectiveness without retargeting collection history"
    );
}
