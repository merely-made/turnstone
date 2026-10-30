// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// [`author_invitation`] against an already-open Moot and group, for the
/// worker that holds this place's only store handles. Also returns the add
/// frame authored on the group-key lane, for publishing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn author_invitation_with(
    moot: &MootFile,
    group: &mut PlaceGroup,
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    joiner_prekey: &[u8],
    not_after_ms: u64,
    rendezvous: Vec<crate::place::invite::RendezvousV1>,
    projection_grant: Option<SignedDelegationCertificate>,
) -> Result<(PlaceInviteV1, stickleback::Operation<GroupKeyExt>), String> {
    binding
        .validate()
        .map_err(|error| format!("place binding: {error}"))?;
    let prekey = GroupPrekeyBundle::from_bytes(joiner_prekey)
        .map_err(|error| format!("decode joiner pre-key: {error}"))?;
    if prekey.group != GroupSessionId(binding.moot.0) {
        return Err("joiner pre-key belongs to another group".to_string());
    }
    let joiner_root = prekey
        .personae_root()
        .map_err(|error| format!("verify joiner pre-key: {error}"))?;

    let snapshot = pollster::block_on(moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    // The recipient must already be a governed member. Inviting someone the
    // Moot has not admitted would mint an envelope that can only ever be
    // refused, and refusing here names the real reason instead.
    if !snapshot
        .membership
        .members
        .iter()
        .any(|member| member.member == joiner_root)
    {
        return Err("Gemot membership does not contain the invited root".to_string());
    }

    let mut evidence = Vec::new();
    pollster::block_on(moot.export_plain_drop(
        &mut evidence,
        DropExportProfile::default(),
        DropLimits::default(),
    ))
    .map_err(|error| format!("export Gemot evidence: {error}"))?;

    // Welcome the recipient into the crypto group, then persist: the epoch this
    // mints is the one the envelope names, so losing it would strand a welcome
    // this profile can no longer follow. Members already joined process the
    // same add from the group-key lane.
    let mut next = group.copy()?;
    next.register_prekey(&prekey)
        .map_err(|error| format!("register joiner pre-key: {error}"))?;
    let dispatch = next
        .add(prekey.recipient)
        .map_err(|error| format!("welcome the joiner: {error}"))?;
    let direct = dispatch
        .direct_for(prekey.recipient)
        .ok_or_else(|| "welcome carries no frame for the invited recipient".to_string())?;
    let expected_epoch = next
        .current_epoch()
        .ok_or_else(|| "welcoming a member installed no epoch".to_string())?;
    let added = group
        .commit(directory, identity, next, std::slice::from_ref(&dispatch))?
        .pop()
        .expect("one dispatch authors one frame");

    let invite = PlaceInviteV1 {
        version: crate::place::invite::PLACE_INVITE_VERSION,
        binding: binding.clone(),
        founder: snapshot.governance.founder,
        inviter: identity.master_public_key().to_bytes(),
        inviter_prekey: inline_artifact(&sealed_prekey_bytes(directory, identity, binding.moot.0)?),
        governance: inline_artifact(&evidence),
        key_welcome: inline_artifact(
            &dispatch
                .control
                .to_bytes()
                .map_err(|error| format!("encode welcome control: {error}"))?,
        ),
        key_direct: inline_artifact(
            &direct
                .to_bytes()
                .map_err(|error| format!("encode welcome frame: {error}"))?,
        ),
        expected_epoch,
        membership_heads: snapshot.membership.auth_heads,
        not_after_ms,
        rendezvous,
        // A reader's envelope carries none, which is the whole of the
        // difference at the projection door.
        projection_grant: projection_grant
            .as_ref()
            .map(crate::place::projection_host::encode_grant)
            .transpose()?
            .as_deref()
            .map(inline_artifact),
    };
    Ok((invite, added))
}
