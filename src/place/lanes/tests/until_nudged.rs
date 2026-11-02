// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Answer only the lane watcher's nudges until `done`: no polling, so the
/// wait measured is the product path's. Returns the snapshot and the
/// nudges it took.
pub(super) fn until_nudged(
    worker: &armillary::ActorHandle<PlaceWorkerCommand>,
    updates: &std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    budget: Duration,
    done: impl Fn(&crate::place::OfflinePlaceSnapshot) -> bool,
) -> Option<(crate::place::OfflinePlaceSnapshot, usize)> {
    let deadline = Instant::now() + budget;
    let mut nudges = 0;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match updates.recv_timeout(left) {
            Ok(Update::PlaceLanesAdvanced { .. }) => {
                nudges += 1;
                worker.command(PlaceWorkerCommand::Resync {
                    session,
                    generation: 1,
                });
            },
            Ok(Update::PlaceOpened {
                result: Ok(snapshot),
                ..
            }) if done(&snapshot) => return Some((snapshot, nudges)),
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    None
}
