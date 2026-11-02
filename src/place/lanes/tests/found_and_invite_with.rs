// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// [`found_and_invite`] on a chosen clock, with an optional grant lifetime.
pub(super) fn found_and_invite_with(
    tag: &str,
    seed: u8,
    joiner: &InMemoryProvider,
    access: crate::place::PlaceInviteAccess,
    clock: crate::place::worker::PlaceWorkerSettings,
    lifetime_ms: Option<u64>,
) -> ServedFounder {
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
    let document = vault.join("field.knot");
    std::fs::write(&document, "# Field\n").unwrap();

    let founder = RootIdentity::from_seed([seed; 32]);
    let host_settings = crate::place::worker::PlaceWorkerSettings {
        knot_root: Some(vault.clone()),
        ..clock
    };
    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), Arc::new(founder), host_settings);
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Found {
        session,
        generation: 1,
        directory: host.clone(),
        name: "Hearth".into(),
    });
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
                assert_eq!(projection.route, "knot");
                assert_eq!(projection.live_sessions, 0, "nobody has dialed yet");
                let ticket = sync.local_rendezvous[0].clone();
                break (binding, ticket);
            },
            Ok(Update::PlaceFounded {
                result: Err(error), ..
            }) => panic!("founding refused: {error}"),
            Ok(_) => continue,
            Err(error) => panic!("founding never answered: {error}"),
        }
    };

    let prekey = prepare_group_identity(&guest, joiner, binding.moot.0).unwrap();
    worker.command(PlaceWorkerCommand::Invite {
        session,
        generation: 1,
        directory: host.clone(),
        prekey,
        access,
        lifetime_ms,
    });
    let invite = expect_invited(&updates, "the joiner invitation");
    ServedFounder {
        root,
        vault,
        document,
        worker,
        updates,
        session,
        binding,
        ticket,
        invite,
        guest,
    }
}
