// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The constitutional root grant's id, derived from the Moot it governs.
/// Deterministic so a later revocation or audit can recompute it.
pub(crate) fn root_grant_id(moot: [u8; 32]) -> [u8; 32] {
    place_tag(b"turnstone.place.root-grant.v1/", moot)
}
