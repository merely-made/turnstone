// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Author an invitation to this profile's own place for one published pre-key.
///
/// The counterpart to [`admit_invitation`], and deliberately the same shape in
/// reverse: it reads only retained domain state, mints no authority, and every
/// field it fills is one the recipient re-derives and checks independently.
///
/// The caller supplies `joiner_prekey` out of band. Which channel carried it is
/// not this function's concern and must never become evidence: the bundle
/// carries its own Personae attestation, which is what binds it to a person.
#[allow(clippy::too_many_arguments)]
pub fn author_invitation(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    joiner_prekey: &[u8],
    not_after_ms: u64,
    rendezvous: Vec<crate::place::invite::RendezvousV1>,
    projection_grant: Option<SignedDelegationCertificate>,
    settings: &PlaceWorkerSettings,
) -> Result<PlaceInviteV1, String> {
    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(directory).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot store: {error}"))?;
    let mut group = PlaceGroup::open(directory, identity, binding.moot.0)?;
    // Nothing is live here, so the add stays on the lane store until a sync.
    author_invitation_with(
        &moot,
        &mut group,
        directory,
        binding,
        identity,
        joiner_prekey,
        not_after_ms,
        rendezvous,
        projection_grant,
    )
    .map(|(invite, _)| invite)
}
