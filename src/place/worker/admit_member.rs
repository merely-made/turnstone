// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Add one invited root to this place's Moot membership at `access`, and for
/// a writer delegate both Commons domains to it. Idempotent in membership: a
/// root already admitted is left at the access it holds.
///
/// A reader gets membership and no delegation. That is the whole difference:
/// reading a place is a membership question answered by the welcome, while
/// authoring into it is a capability question, and a profile nobody delegated
/// to fails its own worker's preflight before anything is authored.
///
/// Takes the already-open Moot rather than opening its own, because the
/// caller is the worker holding this place's only store handle.
///
/// Returns what it authored so the caller can publish it. Storing is what
/// makes an admission survive; publishing is what makes it arrive at a peer
/// that is already connected.
/// Projection authority is included only for a separately offered host.
pub(super) fn admit_member(
    moot: &MootFile,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    joiner_root: [u8; 32],
    access: crate::place::PlaceInviteAccess,
    now_ms: u64,
    expires_at_ms: Option<u64>,
    projection_offered: bool,
) -> Result<AdmittedOps, String> {
    use crate::place::PlaceInviteAccess;
    let moot_id = binding.moot.0;
    let snapshot = pollster::block_on(moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    // A place may offer graph/chat without a document projection host. Do not
    // require that unrelated authority to admit a writer. When offered,
    // prepare its grant before changing membership or delegation, so refusal
    // cannot leave an admission partially authored.
    let projection = if projection_offered && matches!(access, PlaceInviteAccess::Writer) {
        Some(crate::place::projection_host::issue_projection_grant(
            identity,
            moot_id,
            joiner_root,
            now_ms,
            expires_at_ms,
        )?)
    } else {
        None
    };
    let mut authored = AdmittedOps {
        projection,
        ..AdmittedOps::default()
    };
    if !snapshot
        .membership
        .members
        .iter()
        .any(|member| member.member == joiner_root)
    {
        authored.membership = Some(
            pollster::block_on(moot.membership_store().author_for_identity(
                identity,
                MootMembershipAction::Add {
                    member: joiner_root,
                    access: match access {
                        PlaceInviteAccess::Writer => MootAccessLevel::Write,
                        PlaceInviteAccess::Reader => MootAccessLevel::Read,
                    },
                },
            ))
            .map_err(|error| format!("add the invited root to membership: {error}"))?,
        );
    }
    if matches!(access, PlaceInviteAccess::Writer) {
        authored.delegation = Some(
            pollster::block_on(moot.delegation_store().author_issue(
                &founder_signing_key(identity, moot_id)?,
                &snapshot.governance.rules,
                place_delegation(
                    identity,
                    moot_id,
                    joiner_root,
                    now_ms,
                    now_ms,
                    expires_at_ms,
                )?,
            ))
            .map_err(|error| format!("delegate the Commons domains: {error}"))?,
        );
    }
    Ok(authored)
}
