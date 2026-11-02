// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn admit_inner(
    directory: &Path,
    invite: &PlaceInviteV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<AdmittedPlace, String> {
    invite
        .validate()
        .map_err(|error| format!("place invitation: {error}"))?;
    let binding = &invite.binding;
    let local_root = identity.master_public_key().to_bytes();

    // 0. The inviter's own time bound, checked before anything is created.
    //    The heads pin below already invalidates an invitation whenever the
    //    roster moves; this additionally stops a forwarded envelope working
    //    forever in a Moot whose membership never changes.
    let now_ms = settings.authority_clock.now_ms();
    if now_ms > invite.not_after_ms {
        return Err(format!(
            "invitation expired at {}",
            crate::place::utc_ms(invite.not_after_ms)
        ));
    }

    // 5. The Commons scopes are distinct governed roots, not aliases of the
    //    Moot or of each other. Checked first because it is free, and because
    //    a collision here would silently point two domains at one store.
    if binding.root.0 == binding.moot.0 || binding.chat.0 == binding.moot.0 {
        return Err("invitation reuses the Moot id as a Commons scope".to_string());
    }
    if binding.root.0 == binding.chat.0 {
        return Err("invitation gives the graph and chat the same scope".to_string());
    }

    // 1. The Gemot evidence addresses this Moot. Imported into the place's own
    //    store so the fold is Gemot's, not a claim the envelope makes.
    let stores = place_store_dir(directory);
    std::fs::create_dir_all(&stores).map_err(|error| format!("create place store: {error}"))?;
    let moot = pollster::block_on(MootFile::open(
        stores.join("gemot"),
        MootId(binding.moot.0),
        invite.founder,
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot store: {error}"))?;
    let evidence = invite
        .governance
        .verified_bytes("governance artifact")
        .map_err(|error| format!("place invitation: {error}"))?;
    let receipt = pollster::block_on(
        moot.import_plain_drop(std::io::Cursor::new(evidence), invite_drop_limits()),
    )
    .map_err(|error| format!("import Gemot evidence: {error}"))?;
    if receipt.snapshot.moot_id != MootId(binding.moot.0) {
        return Err("Gemot evidence addresses another Moot".to_string());
    }

    // 2. The converged membership fold contains the local Personae root, and
    //    the claimed inviter. The envelope names an author; Gemot decides
    //    whether that name is a member.
    let members = &receipt.snapshot.membership.members;
    if !members.iter().any(|member| member.member == local_root) {
        return Err("Gemot membership does not contain this Personae root".to_string());
    }
    if !members.iter().any(|member| member.member == invite.inviter) {
        return Err("invitation names an author outside Gemot membership".to_string());
    }

    // 3. The welcome is bound to this group and addressed to this recipient's
    //    authenticated crypto identity, and it produces a usable epoch.
    let control = GroupControlFrame::from_bytes(
        invite
            .key_welcome
            .verified_bytes("key welcome artifact")
            .map_err(|error| format!("place invitation: {error}"))?,
    )
    .map_err(|error| format!("decode group welcome: {error}"))?;
    let direct = GroupDirectFrame::from_bytes(
        invite
            .key_direct
            .verified_bytes("recipient welcome artifact")
            .map_err(|error| format!("place invitation: {error}"))?,
    )
    .map_err(|error| format!("decode recipient welcome: {error}"))?;
    let group_id = GroupSessionId(binding.moot.0);
    if control.group != group_id || direct.group != group_id {
        return Err("group welcome addresses another group".to_string());
    }
    if direct.control != control.id {
        return Err("recipient welcome belongs to another control frame".to_string());
    }
    // The group identity whose published pre-key this welcome answers. Absent
    // means nobody could have addressed a welcome to this profile yet.
    let mut group = load_group_session(directory, identity, binding.moot.0)
        .map_err(|error| format!("{error}; prepare a group identity before joining"))?;
    if direct.recipient != group.member() {
        return Err("group welcome is addressed to another recipient".to_string());
    }

    // The sender's authenticated pre-key. Its Personae attestation is what
    // makes `invite.inviter` a verified fact rather than an envelope claim, so
    // the two must agree before the bundle is registered.
    let inviter_prekey = GroupPrekeyBundle::from_bytes(
        invite
            .inviter_prekey
            .verified_bytes("inviter pre-key artifact")
            .map_err(|error| format!("place invitation: {error}"))?,
    )
    .map_err(|error| format!("decode inviter pre-key: {error}"))?;
    if inviter_prekey.group != group_id {
        return Err("inviter pre-key belongs to another group".to_string());
    }
    let attested = inviter_prekey
        .personae_root()
        .map_err(|error| format!("verify inviter pre-key: {error}"))?;
    if attested != invite.inviter {
        return Err("inviter pre-key attests a different Personae root".to_string());
    }
    group
        .register_prekey(&inviter_prekey)
        .map_err(|error| format!("register inviter pre-key: {error}"))?;
    group
        .process(invite.inviter, &control, Some(&direct))
        .map_err(|error| format!("process group welcome: {error}"))?;
    // 4. Gemot binds that epoch to the same membership heads.
    //
    // The epoch must be the one the invitation describes, and it must have been
    // minted against the membership state Gemot itself converged to from the
    // imported evidence. Without the second half, a welcome minted before a
    // removal would hand the joiner a key the departed member still holds, and
    // every other check would still pass.
    match group.current_epoch() {
        None => return Err("group welcome produced no current epoch".to_string()),
        Some(epoch) if epoch != invite.expected_epoch => {
            return Err(
                "group welcome installed an epoch the invitation does not name".to_string(),
            );
        },
        Some(_) => {},
    }
    // `auth_heads()` returns sorted heads, and the envelope's are bounded and
    // compared as given: a reordered or padded list is a different claim.
    if receipt.snapshot.membership.auth_heads != invite.membership_heads {
        return Err("invitation pins membership heads that Gemot did not converge to".to_string());
    }

    // Every domain has answered. Only now does anything durable exist, and the
    // binding is written last so the presence of `place.json` implies the whole
    // check list ran, not merely that an envelope parsed.
    save_group_session(directory, identity, &group)?;
    // 6. The projection grant, when the envelope carries one. A reader's does
    //    not, and its absence is not a refusal: it is the reader being refused
    //    at the holder's door later, which is where that decision belongs.
    if let Some(artifact) = &invite.projection_grant {
        let bytes = artifact
            .verified_bytes("projection grant artifact")
            .map_err(|error| format!("place invitation: {error}"))?;
        let grant = crate::place::projection_host::decode_grant(bytes)?;
        if grant.check().is_err() {
            return Err("projection grant does not verify".to_string());
        }
        if grant.certificate.subject != local_root {
            return Err("projection grant names another subject".to_string());
        }
        crate::place::rendezvous::save_projection_grant(directory, bytes)?;
    }
    crate::session::save_place_binding(directory, binding)
        .map_err(|error| format!("persist place binding: {error}"))?;
    Ok(AdmittedPlace {
        binding: binding.clone(),
        moot: MootCache {
            membership_epoch: receipt.snapshot.membership.epoch,
            members: members.len(),
            roster_members: receipt.snapshot.roster.members.len(),
            delegated_certificates: receipt.snapshot.delegated_certificates,
            standing_operations: receipt.snapshot.standing_operations,
        },
        group_members: group
            .members()
            .map_err(|error| format!("materialize group membership: {error}"))?
            .len(),
    })
}
