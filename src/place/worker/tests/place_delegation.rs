// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The founder's signed delegation admitting one profile root to both
/// Commons domains, at the pinned window. Deterministic, so a later test
/// can recompute its id to revoke it.
pub(crate) fn place_delegation(
    founder: &InMemoryProvider,
    moot: [u8; 32],
    subject: [u8; 32],
) -> SignedDelegationCertificate {
    super::super::place_delegation(founder, moot, subject, 15, 20, Some(900)).unwrap()
}
