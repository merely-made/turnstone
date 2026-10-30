// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One Gemot lane's counters out of a joiner's snapshot.
pub(super) fn lane_counters(
    snapshot: &crate::place::OfflinePlaceSnapshot,
    name: &str,
) -> (u64, u64) {
    let sync = snapshot.sync.as_ref().expect("a live joiner has lanes");
    let lane = sync
        .lanes
        .iter()
        .find(|lane| lane.name == name)
        .unwrap_or_else(|| panic!("{name} is one of the ten"));
    (lane.sync_rounds, lane.ops_received)
}
