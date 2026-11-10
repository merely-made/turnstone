// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn reopen_connected_place(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
    contact: Option<&crate::place::PlaceCardV1>,
    watch: (Emitter<Update>, SessionId, u64),
) -> Result<(OpenPlace, OfflinePlaceSnapshot), String> {
    let (mut opened, _) = reopen_cached_place(directory, binding, identity, settings)?;
    let membership = pollster::block_on(opened.moot.snapshot())
        .map_err(|error| format!("materialize retained Moot: {error}"))?;
    if !membership
        .membership
        .members
        .iter()
        .any(|member| member.member == opened.subject)
    {
        return Err("this identity is no longer a member in retained place state".into());
    }

    #[cfg(feature = "place-reticulum")]
    if let Some(config) = ReticulumGraphConfig::load(directory)? {
        if contact.is_some() {
            return Err("update the saved Reticulum graph route to refresh its contact".into());
        }
        opened.reticulum = Some(reticulum_graph::AutoGraphSync::start(
            &mut opened,
            identity,
            config,
        )?);
        let snapshot = place_snapshot(&opened, binding.moot.0, settings)?;
        return Ok((opened, snapshot));
    }

    let now_ms = settings.authority_clock.now_ms();
    let tickets = match contact {
        Some(card) => crate::place::rendezvous::card_tickets(binding, card)?,
        None => crate::place::rendezvous::load_rendezvous(directory, binding, now_ms)?,
    };
    opened.lanes = Some(crate::place::lanes::join_live(
        &opened,
        binding,
        identity,
        &tickets,
        settings.projection_setup(binding, identity),
        Some(watch),
    )?);
    // Contact metadata is retained only after membership and routing checks.
    if let Some(card) = contact {
        crate::place::rendezvous::save_card_rendezvous(directory, binding, card, now_ms)?;
    }
    let snapshot = place_snapshot(&opened, binding.moot.0, settings)?;
    Ok((opened, snapshot))
}
