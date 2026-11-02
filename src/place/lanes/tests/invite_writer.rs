// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Offer a pre-key for `seed` and have the founder's product `Invite`
/// admit it as a writer.
pub(super) fn invite_writer(
    founder: &(
        armillary::ActorHandle<PlaceWorkerCommand>,
        std::sync::mpsc::Receiver<Update>,
    ),
    founder_session: SessionId,
    host: &Path,
    directory: &Path,
    seed: u8,
    moot: [u8; 32],
) -> Box<crate::place::invite::PlaceInviteV1> {
    let prekey =
        prepare_group_identity(directory, &InMemoryProvider::from_seed([seed; 32]), moot).unwrap();
    founder.0.command(PlaceWorkerCommand::Invite {
        session: founder_session,
        generation: 1,
        directory: host.to_path_buf(),
        prekey,
        access: crate::place::PlaceInviteAccess::Writer,
        lifetime_ms: None,
    });
    expect_invited(&founder.1, "a writer invitation")
}
