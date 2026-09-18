// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Maps Turnstone's own place truth onto `moot_port::coop`'s lifecycle
//! contract (K3 of the 2026-09-16 coop lifecycle parity plan), so status
//! wording, observation, and the conformance harness all read the same view.
//!
//! This module owns nothing: it derives a [`coop::LifecycleReport`] from
//! [`PlaceState`] or an [`OfflinePlaceSnapshot`] the worker already produced.
//! `local_standing` in `worker.rs` stays the one authority on standing; this
//! module only translates its answer.

use moot_port::coop;

use super::{OfflinePlaceSnapshot, PlaceMemberAccess, PlaceStanding, PlaceState};

/// Derive this session's [`coop::LifecycleReport`] at `now_ms`. The one entry
/// point observation and the conformance harness both use.
pub(crate) fn lifecycle_report(state: &PlaceState, now_ms: u64) -> coop::LifecycleReport {
    match state {
        PlaceState::Personal => not_joined("no shared place is joined", now_ms),
        PlaceState::Joining { .. } => not_joined("an invitation is being admitted", now_ms),
        PlaceState::Opening { .. } => {
            not_joined("opening retained state or reconnecting", now_ms)
        },
        // Degraded carries only the error, not the last-retained snapshot, so
        // it reads as not joined with that error as its reason. Chosen over
        // inventing a standing this state does not carry. (K3 brief.)
        PlaceState::Degraded { error, .. } => not_joined(error, now_ms),
        PlaceState::Failed { error } => not_joined(error, now_ms),
        PlaceState::Left { .. } => {
            // Reading is `Unknown`, not `Continues`: this session no longer
            // has the stores open to say whether they can still be read, and
            // "unaffected by the withdrawal" would claim more than a
            // departed, unopened store can back up.
            let mut report = coop::LifecycleReport::new(
                coop::Verdict::Left,
                Some("this session left the place; history retained".into()),
                now_ms,
            );
            report.reading = coop::Reading::Unknown;
            report
        },
        PlaceState::Offline { snapshot, .. } => lifecycle_report_from_snapshot(snapshot, now_ms),
    }
}

/// The same mapping from a retained snapshot alone, for a caller (the
/// conformance harness's driver) that already holds a refreshed
/// [`OfflinePlaceSnapshot`] rather than a whole [`PlaceState`].
pub(crate) fn lifecycle_report_from_snapshot(
    snapshot: &OfflinePlaceSnapshot,
    now_ms: u64,
) -> coop::LifecycleReport {
    let membership = snapshot
        .members
        .iter()
        .find(|member| member.root == snapshot.personae_root)
        .map(|member| coop::Membership {
            members: snapshot.members.len() as u32,
            access: match member.access {
                PlaceMemberAccess::Pull => coop::Access::Pull,
                PlaceMemberAccess::Read => coop::Access::Read,
                PlaceMemberAccess::Write => coop::Access::Write,
                PlaceMemberAccess::Manage => coop::Access::Manage,
            },
        });
    let (verdict, reason, reading) = match snapshot.standing {
        PlaceStanding::Member => (coop::Verdict::Joined, None, coop::Reading::Continues),
        PlaceStanding::MembershipRevoked | PlaceStanding::GrantRevoked => (
            coop::Verdict::Revoked,
            snapshot.standing.refusal(),
            coop::Reading::Cut,
        ),
        PlaceStanding::GrantExpired { .. } => (
            coop::Verdict::Expired,
            snapshot.standing.refusal(),
            coop::Reading::Unknown,
        ),
    };
    // The snapshot carries no delegation expiry bound of its own beyond what
    // `standing` already names on withdrawal, so `expires_at_ms` stays `None`
    // rather than inventing one; `GrantExpired` supplies `expired_at_ms` from
    // the same `at_ms` `local_standing` computed. (K3 brief, documented
    // choice.)
    let grant = coop::Grant {
        expires_at_ms: None,
        expired_at_ms: match snapshot.standing {
            PlaceStanding::GrantExpired { at_ms } => Some(at_ms),
            _ => None,
        },
    };
    // `local_standing` retains neither who revoked nor when: it is a live
    // recomputation over the delegation fold at `at_ms`, not a record of the
    // withdrawal event itself. Both `by` and `at_ms` stay `None` rather than
    // substituting this reading's own clock for a fact the snapshot does not
    // carry — `now_ms` above already says when this report was taken; a
    // fabricated `at_ms` here would look like a second, different fact.
    // (K3 brief, documented choice.)
    let revocation = match snapshot.standing {
        PlaceStanding::MembershipRevoked | PlaceStanding::GrantRevoked => {
            Some(coop::Revocation { by: None, at_ms: None })
        },
        _ => None,
    };
    let mut report = coop::LifecycleReport::new(verdict, reason, now_ms);
    report.membership = membership;
    report.grant = Some(grant);
    report.revocation = revocation;
    report.reading = reading;
    report
}

fn not_joined(reason: impl Into<String>, now_ms: u64) -> coop::LifecycleReport {
    coop::LifecycleReport::new(coop::Verdict::NotJoined, Some(reason.into()), now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::place::{PlaceMember, PlaceMemberAccess};

    fn snapshot_with(standing: PlaceStanding) -> OfflinePlaceSnapshot {
        OfflinePlaceSnapshot {
            personae_root: [9; 32],
            standing,
            members: vec![PlaceMember { root: [9; 32], access: PlaceMemberAccess::Write }],
            ..Default::default()
        }
    }

    #[test]
    fn every_place_state_maps_to_a_report_that_holds_its_invariants() {
        for state in [
            PlaceState::Personal,
            PlaceState::Joining { generation: 1 },
            PlaceState::Failed { error: "no network".into() },
        ] {
            let report = lifecycle_report(&state, 50);
            assert_eq!(report.verdict, coop::Verdict::NotJoined);
            report.check_invariants().expect("not_joined reports invariants hold");
        }
    }

    #[test]
    fn left_maps_to_the_left_verdict_with_reading_unknown_and_no_membership() {
        let binding = crate::place::PlaceBindingV1::new(
            crate::place::PlaceId([4; 32]),
            crate::place::SharedContainerId([5; 32]),
            crate::place::ChatSpaceId([6; 32]),
            "hall",
        )
        .unwrap();
        let state = PlaceState::Left {
            binding,
            retained: crate::place::PlaceLeftSummary {
                graph_nodes: 3,
                chat_messages: 5,
                members: 2,
            },
        };
        let report = lifecycle_report(&state, 50);
        assert_eq!(report.verdict, coop::Verdict::Left);
        assert!(report.reason.is_some());
        assert_eq!(report.reading, coop::Reading::Unknown);
        assert_eq!(report.membership, None);
        report.check_invariants().expect("left report invariants hold");
    }

    #[test]
    fn member_standing_maps_to_joined_with_membership_and_a_grant() {
        let snapshot = snapshot_with(PlaceStanding::Member);
        let report = lifecycle_report_from_snapshot(&snapshot, 50);
        assert_eq!(report.verdict, coop::Verdict::Joined);
        assert_eq!(report.reason, None);
        assert_eq!(report.reading, coop::Reading::Continues);
        assert!(report.membership.is_some());
        assert!(report.grant.is_some());
        report.check_invariants().expect("joined report invariants hold");
    }

    #[test]
    fn membership_revoked_maps_to_revoked_with_reading_cut() {
        let snapshot = snapshot_with(PlaceStanding::MembershipRevoked);
        let report = lifecycle_report_from_snapshot(&snapshot, 50);
        assert_eq!(report.verdict, coop::Verdict::Revoked);
        assert_eq!(report.reading, coop::Reading::Cut);
        assert!(report.revocation.is_some());
        report.check_invariants().expect("revoked report invariants hold");
    }

    #[test]
    fn grant_revoked_maps_to_revoked_with_reading_cut() {
        let snapshot = snapshot_with(PlaceStanding::GrantRevoked);
        let report = lifecycle_report_from_snapshot(&snapshot, 50);
        assert_eq!(report.verdict, coop::Verdict::Revoked);
        assert_eq!(report.reading, coop::Reading::Cut);
        assert!(report.revocation.is_some());
        report.check_invariants().expect("revoked report invariants hold");
    }

    #[test]
    fn grant_expired_maps_to_expired_with_reading_unknown() {
        let snapshot = snapshot_with(PlaceStanding::GrantExpired { at_ms: 1_000 });
        let report = lifecycle_report_from_snapshot(&snapshot, 1_500);
        assert_eq!(report.verdict, coop::Verdict::Expired);
        assert_eq!(report.reading, coop::Reading::Unknown);
        assert_eq!(report.grant.and_then(|grant| grant.expired_at_ms), Some(1_000));
        report.check_invariants().expect("expired report invariants hold");
    }
}
