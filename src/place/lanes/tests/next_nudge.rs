// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Wait for the founder worker's own watch nudge.
pub(super) fn next_nudge(founder: &ServedFounder, budget: Duration) -> Option<Duration> {
    let started = Instant::now();
    let deadline = started + budget;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        match founder.updates.recv_timeout(left) {
            Ok(Update::PlaceLanesAdvanced { .. }) => return Some(started.elapsed()),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}
