// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// This profile's standing at `at_ms`, from the retained membership and
/// delegation folds.
///
/// An empty membership fold names nobody, so it withdraws nothing: only a
/// fold that has members and lacks this root reads as a removal.
pub(super) fn local_standing(
    members: &[MootMember],
    delegations: &gemot::moot::MootDelegations,
    rules: &ConstitutionRules,
    moot_id: [u8; 32],
    subject: [u8; 32],
    at_ms: u64,
) -> crate::place::PlaceStanding {
    use crate::place::PlaceStanding;
    if !members.is_empty() && !members.iter().any(|member| member.member == subject) {
        return PlaceStanding::MembershipRevoked;
    }
    let grants: Vec<_> = delegations
        .projections(moot_id, rules, at_ms)
        .into_iter()
        .filter(|grant| grant.subject == subject)
        .collect();
    if grants.iter().any(|grant| grant.active) {
        return PlaceStanding::Member;
    }
    if grants.iter().any(|grant| grant.directly_revoked) {
        return PlaceStanding::GrantRevoked;
    }
    grants
        .iter()
        .filter_map(|grant| grant.expires_at_ms)
        .filter(|expires| *expires < at_ms)
        .max()
        .map_or(PlaceStanding::Member, |at_ms| PlaceStanding::GrantExpired {
            at_ms,
        })
}
