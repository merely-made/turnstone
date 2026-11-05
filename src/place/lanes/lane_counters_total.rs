// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl LaneCounters {
    /// Accepted operations across every lane, plus the projection host's own
    /// session counters. A sum is right HERE and wrong in the status surface:
    /// the watcher only needs to know that SOMETHING moved, while a person
    /// needs to know which lane it arrived on and how many peers are being
    /// served. Folding the projection in is what makes a visitor arriving or
    /// leaving nudge the app exactly like a lane arrival does.
    pub(super) fn total(&self) -> u64 {
        let lanes: u64 = self
            .handles
            .iter()
            .map(|handle| handle.lock().map(|status| status.ops_received).unwrap_or(0))
            .sum();
        // Only active catalogue state needs timed expiry/retry refreshes.
        // An empty place must settle, just as it did before discovery.
        let clock = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let clock = if self
            .catalogue_clock
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            clock
        } else {
            0
        };
        lanes
            + self.catalogue.status().received
            + clock
            + self
                .projection
                .as_ref()
                .map(ProjectionWatchCounters::total)
                .unwrap_or(0)
    }
}
