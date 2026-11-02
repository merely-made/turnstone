// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Spawn the retained-place worker. Each `Open` first releases the prior
/// session's database handles, so switch and trash can establish ordering with
/// the explicit `Release` acknowledgement.
pub fn spawn_place_worker(
    wake: Wake,
    identity: Arc<RootIdentity>,
    settings: PlaceWorkerSettings,
) -> (ActorHandle<PlaceWorkerCommand>, Receiver<Update>) {
    spawn_place_worker_with(wake, Some(identity), settings)
}
