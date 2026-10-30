// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Spawn the worker while the profile identity is pending (the vault is
/// locked). Every command that speaks as the profile waits, in order, until
/// [`PlaceWorkerCommand::AdoptIdentity`]; `Release` is still answered at once,
/// so a session switch never waits on the vault, and drops what the released
/// session had waiting.
pub fn spawn_pending_place_worker(
    wake: Wake,
    settings: PlaceWorkerSettings,
) -> (ActorHandle<PlaceWorkerCommand>, Receiver<Update>) {
    spawn_place_worker_with(wake, None, settings)
}
