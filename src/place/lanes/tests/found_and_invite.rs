// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Found a place whose worker serves a one-document vault, then admit one
/// root at `access` through the product `Invite`.
pub(super) fn found_and_invite(
    tag: &str,
    seed: u8,
    joiner: &InMemoryProvider,
    access: crate::place::PlaceInviteAccess,
) -> ServedFounder {
    found_and_invite_with(tag, seed, joiner, access, settings(), None)
}
