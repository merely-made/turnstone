// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Prepare the dial to one holder, or say why this profile cannot.
///
/// Three things have to be true and each failure reads differently: the place
/// is live (there is a transport at all), admission stored a projection grant
/// (this profile was admitted as a writer), and the holder's ticket is known.
/// For this slice the only ticket a member has is the founder's saved
/// rendezvous, so a holder who is not the founder is unreachable rather than
/// searched for.
pub(super) fn visit_dial(
    open: &OpenPlace,
    directory: &Path,
    identity: &dyn IdentityProvider,
    holder_root: [u8; 32],
    settings: &PlaceWorkerSettings,
) -> Result<crate::place::lanes::HolderDial, String> {
    // A withdrawn profile is refused here with the reason, not at the door
    // with a handshake fault.
    let at_ms = settings.authority_clock.now_ms();
    let snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    if let Some(reason) = local_standing(
        &snapshot.membership.members,
        &delegations,
        &snapshot.governance.rules,
        open.binding.moot.0,
        open.subject,
        at_ms,
    )
    .refusal()
    {
        return Err(format!(
            "this profile cannot visit a held document: {reason}"
        ));
    }
    let lanes = open
        .lanes
        .as_ref()
        .ok_or_else(|| "this place is not live, so no holder can be reached".to_string())?;
    let grant = crate::place::rendezvous::load_projection_grant(directory).ok_or_else(|| {
        "this profile holds no projection grant for this place; a reader is not admitted \
         to a held document"
            .to_string()
    })?;
    let ticket = crate::place::rendezvous::load_rendezvous(
        directory,
        &open.binding,
        settings.authority_clock.now_ms(),
    )?
    .into_iter()
    .next()
    .ok_or_else(|| "no rendezvous for the mere holding this document".to_string())?;
    // Domain-separated per holder and per open, so two visits in one session
    // never present the same handshake nonce.
    let nonce = place_tag(
        b"turnstone.place.visit-nonce.v1/",
        *blake3::hash(
            &[
                &holder_root[..],
                &open.binding.moot.0[..],
                &settings.authority_clock.now_ms().to_le_bytes()[..],
            ]
            .concat(),
        )
        .as_bytes(),
    );
    crate::place::lanes::holder_dial(lanes, &ticket, &grant, identity, nonce)
}
