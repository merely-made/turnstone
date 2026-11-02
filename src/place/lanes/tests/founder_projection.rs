// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The founder's current projection observation, through a product Resync.
pub(super) fn founder_projection(
    founder: &ServedFounder,
) -> Option<crate::place::PlaceProjectionSnapshot> {
    founder.worker.command(PlaceWorkerCommand::Resync {
        session: founder.session,
        generation: 1,
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(Instant::now() < deadline, "the founder never re-folded");
        match founder.updates.recv_timeout(Duration::from_secs(5)) {
            Ok(Update::PlaceOpened {
                result: Ok(snapshot),
                ..
            }) => return snapshot.sync.and_then(|sync| sync.projection),
            Ok(_) => continue,
            Err(error) => panic!("the founder never answered a resync: {error}"),
        }
    }
}
