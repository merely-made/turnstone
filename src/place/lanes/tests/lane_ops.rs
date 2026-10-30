// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Every lane's name paired with its `ops_received`, for a before/after
/// diff across a replay.
pub(super) fn lane_ops(snapshot: &crate::place::OfflinePlaceSnapshot) -> Vec<(String, u64)> {
    snapshot
        .sync
        .as_ref()
        .expect("a live writer has lanes")
        .lanes
        .iter()
        .map(|lane| (lane.name.to_string(), lane.ops_received))
        .collect()
}
