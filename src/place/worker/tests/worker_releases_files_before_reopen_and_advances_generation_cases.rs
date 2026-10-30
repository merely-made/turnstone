// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn worker_releases_files_before_reopen_and_advances_generation() {
    let root = std::env::temp_dir().join(format!("turnstone-place-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let original = root.join("original");
    let moved = root.join("moved");
    let identity = Arc::new(RootIdentity::Local(InMemoryProvider::from_seed([0x91; 32])));
    let binding = binding(0x41);
    seed_profile(&original, identity.as_ref(), &binding, 1);

    let wake: Wake = Arc::new(|| {});
    let (worker, updates) = spawn_place_worker(wake, identity, settings());
    worker.command(PlaceWorkerCommand::Open {
        session: SessionId::new(),
        generation: 1,
        directory: original.clone(),
        binding: binding.clone(),
    });
    let first = updates.recv_timeout(ANSWER).unwrap();
    assert!(matches!(
        first,
        Update::PlaceOpened {
            generation: 1,
            result: Ok(_),
            ..
        }
    ));

    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();
    std::fs::rename(&original, &moved)
        .expect("release acknowledgement means the session directory can move");

    worker.command(PlaceWorkerCommand::Open {
        session: SessionId::new(),
        generation: 2,
        directory: moved,
        binding,
    });
    let second = updates.recv_timeout(ANSWER).unwrap();
    assert!(matches!(
        second,
        Update::PlaceOpened {
            generation: 2,
            result: Ok(_),
            ..
        }
    ));
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    ack_rx.recv_timeout(ANSWER).unwrap();
    let _ = std::fs::remove_dir_all(&root);
}
