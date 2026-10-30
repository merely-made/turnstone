// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A founder's worker, live on a place its product `Found` opened.
pub(super) fn found_live(
    host: &Path,
    seed: u8,
) -> (
    (
        armillary::ActorHandle<PlaceWorkerCommand>,
        std::sync::mpsc::Receiver<Update>,
    ),
    SessionId,
    PlaceBindingV1,
) {
    let identity = RootIdentity::from_seed([seed; 32]);
    let founder = spawn_place_worker(Arc::new(|| {}), Arc::new(identity), settings());
    let session = SessionId::new();
    founder.0.command(PlaceWorkerCommand::Found {
        session,
        generation: 1,
        directory: host.to_path_buf(),
        name: "Hearth".into(),
    });
    loop {
        match founder.1.recv_timeout(Duration::from_secs(60)) {
            Ok(Update::PlaceFounded {
                result: Ok((binding, _)),
                ..
            }) => return (founder, session, binding),
            Ok(Update::PlaceFounded {
                result: Err(error), ..
            }) => panic!("founding refused: {error}"),
            Ok(_) => continue,
            Err(error) => panic!("founding never answered: {error}"),
        }
    }
}
