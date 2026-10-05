// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Mailbox polling until native producers expose an event-loop wake callback.
//! Early event batches retain the deadline; a late wake polls once rather than
//! replaying missed intervals. Ordinary redraws remain independent of this clock.

use std::time::{Duration, Instant};

pub(super) struct SurfacePollClock {
    interval: Duration,
    next: Option<Instant>,
}

impl SurfacePollClock {
    pub(super) fn new(interval: Duration) -> Self {
        Self { interval, next: None }
    }

    pub(super) fn deadline(&mut self, active: bool, now: Instant) -> Option<Instant> {
        if !active {
            self.next = None;
            return None;
        }
        Some(*self.next.get_or_insert_with(|| now + self.interval))
    }

    pub(super) fn poll_due(&mut self, active: bool, now: Instant) -> bool {
        let Some(deadline) = self.deadline(active, now) else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.next = Some(now + self.interval);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_cancelled_wakes_do_not_postpone_surface_poll() {
        let start = Instant::now();
        let interval = Duration::from_millis(1000);
        let mut clock = SurfacePollClock::new(interval);
        let deadline = start + interval;
        assert_eq!(clock.deadline(true, start), Some(deadline));
        // Each early batch enters new_events and then about_to_wait. None may
        // restart the countdown, even when the batches never leave a quiet gap.
        for millis in 1..1000 {
            let now = start + Duration::from_millis(millis);
            assert!(!clock.poll_due(true, now));
            assert_eq!(clock.deadline(true, now), Some(deadline));
        }
        assert!(clock.poll_due(true, deadline));
        assert_eq!(clock.deadline(true, deadline), Some(deadline + interval));
    }

    #[test]
    fn late_wake_polls_once_without_catch_up_spin() {
        let start = Instant::now();
        let interval = Duration::from_millis(16);
        let mut clock = SurfacePollClock::new(interval);
        clock.deadline(true, start);
        let late = start + Duration::from_millis(130);
        assert!(clock.poll_due(true, late));
        assert!(!clock.poll_due(true, late));
        assert_eq!(clock.deadline(true, late), Some(late + interval));
        assert!(!clock.poll_due(true, late + interval - Duration::from_millis(1)));
        assert!(clock.poll_due(true, late + interval));
    }

    #[test]
    fn inactive_polling_clears_deadline_and_reenable_starts_fresh() {
        let start = Instant::now();
        let interval = Duration::from_millis(16);
        let mut clock = SurfacePollClock::new(interval);
        clock.deadline(true, start);
        let inactive = start + Duration::from_millis(5);
        assert_eq!(clock.deadline(false, inactive), None);
        assert!(!clock.poll_due(false, start + interval));
        let restart = start + Duration::from_secs(1);
        assert!(!clock.poll_due(true, restart));
        assert_eq!(clock.deadline(true, restart), Some(restart + interval));
        assert!(clock.poll_due(true, restart + interval));
    }
}
