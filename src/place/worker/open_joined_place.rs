// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn open_joined_place(
    directory: &Path,
    invite: &PlaceInviteV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
    watch: (Emitter<Update>, SessionId, u64),
) -> Result<(PlaceBindingV1, OpenPlace, OfflinePlaceSnapshot), String> {
    let binding = admit_invitation(directory, invite, identity, settings)?.binding;
    crate::place::rendezvous::save_admitted_rendezvous(directory, invite)?;
    let (mut opened, snapshot) = open_cached_place(directory, &binding, identity, settings)?;
    // A ticketless invitation enrolls the peer and leaves the place offline.
    let tickets: Vec<String> = invite.dialable().map(|entry| entry.hint.clone()).collect();
    if !tickets.is_empty() {
        opened.lanes = Some(crate::place::lanes::join_live(
            &opened,
            &binding,
            identity,
            &tickets,
            settings.projection_setup(&binding, identity),
            Some(watch),
        )?);
    }
    Ok((binding, opened, snapshot))
}
