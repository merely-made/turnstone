// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Drive the worker until its snapshot satisfies `done`, answering lane
/// nudges with resyncs. Returns the satisfying snapshot, or says how far
/// it got.
pub(super) fn converge_until(
    worker: &armillary::ActorHandle<PlaceWorkerCommand>,
    updates: &std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    what: &str,
    done: impl Fn(&crate::place::OfflinePlaceSnapshot) -> bool,
) -> crate::place::OfflinePlaceSnapshot {
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut last = None;
    // Ask once up front: what is already retained may satisfy `done`
    // without any lane traffic at all.
    worker.command(PlaceWorkerCommand::Resync {
        session,
        generation: 1,
    });
    loop {
        assert!(
            Instant::now() < deadline,
            "{what}: never happened; {last:?}"
        );
        match updates.recv_timeout(Duration::from_secs(10)) {
            Ok(Update::PlaceLanesAdvanced { .. }) => {
                worker.command(PlaceWorkerCommand::Resync {
                    session,
                    generation: 1,
                });
            },
            Ok(Update::PlaceOpened {
                result: Ok(snapshot),
                ..
            })
            | Ok(Update::PlaceCommandDone {
                result: Ok(snapshot),
                ..
            }) => {
                if done(&snapshot) {
                    return snapshot;
                }
                last = Some(snapshot);
                // No nudge is coming if the lanes are quiet, so keep
                // asking rather than blocking on traffic that may not
                // arrive.
                std::thread::sleep(Duration::from_millis(300));
                worker.command(PlaceWorkerCommand::Resync {
                    session,
                    generation: 1,
                });
            },
            Ok(Update::PlaceCommandDone {
                result: Err(error), ..
            }) => panic!("{what}: command refused: {error}"),
            _ => continue,
        }
    }
}
