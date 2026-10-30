// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A member's own worker, joined through the product `Join`.
pub(super) fn join_member(
    directory: &Path,
    seed: u8,
    invite: Box<crate::place::invite::PlaceInviteV1>,
) -> (
    armillary::ActorHandle<PlaceWorkerCommand>,
    std::sync::mpsc::Receiver<Update>,
    SessionId,
) {
    let identity = RootIdentity::from_seed([seed; 32]);
    let (worker, updates) = spawn_place_worker(Arc::new(|| {}), Arc::new(identity), settings());
    let session = SessionId::new();
    worker.command(PlaceWorkerCommand::Join {
        session,
        generation: 1,
        directory: directory.to_path_buf(),
        invite,
    });
    joined_snapshot(&updates);
    (worker, updates, session)
}
