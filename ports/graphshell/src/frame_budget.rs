// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The step budget both web pages give the canvas and the remote board: a
//! share of the display's frame period (physics catalog plan, ruled
//! 2026-10-04, "The display's frame"), taken as the shortest recent interval
//! between the page's own frame timestamps so physics cannot feed back into
//! the frame it is budgeted from, and never longer than 1/60 s ("Known rate,
//! else capped": a browser does not say its display's rate). The page passes
//! its clock and the margin its gate keeps ("Gate keeps a forecast margin").

use std::collections::VecDeque;
use std::time::Duration;

use mere::canvas::StepBudget;

/// The longest the display's period is taken to be, and the period assumed
/// until the page has measured an interval: 60 Hz's.
pub const MAX_PERIOD_MS: f64 = 1000.0 / 60.0;
/// A gap longer than this between frames is a hidden or suspended page, not a
/// frame, and is not taken in.
const GAP_MS: f64 = 1000.0;
/// The recent intervals the display period is the shortest of: about two
/// seconds at 60 Hz, half a second at 240 Hz.
const RECENT_INTERVALS: usize = 120;

/// The step budget as a share of the display's frame period: the shortest
/// of the last [`RECENT_INTERVALS`] intervals between frames, at most
/// [`MAX_PERIOD_MS`].
#[derive(Clone, Debug)]
pub struct FrameBudget {
    share: f64,
    margin: Duration,
    clock: fn() -> Duration,
    intervals: VecDeque<f64>,
    last_ms: Option<f64>,
}

impl FrameBudget {
    pub fn new(share: f64, margin: Duration, clock: fn() -> Duration) -> Self {
        Self {
            share,
            margin,
            clock,
            intervals: VecDeque::with_capacity(RECENT_INTERVALS),
            last_ms: None,
        }
    }

    /// Take this frame's timestamp in; the budget for the frame.
    pub fn frame(&mut self, now_ms: f64) -> StepBudget {
        if let Some(last) = self.last_ms {
            let interval = now_ms - last;
            if interval > 0.0 && interval < GAP_MS {
                if self.intervals.len() == RECENT_INTERVALS {
                    self.intervals.pop_front();
                }
                self.intervals.push_back(interval);
            }
        }
        self.last_ms = Some(now_ms);
        self.budget()
    }

    pub fn budget(&self) -> StepBudget {
        StepBudget {
            per_frame: Duration::from_secs_f64(self.share * self.display_period_ms() / 1000.0),
            clock: self.clock,
            margin: self.margin,
        }
    }

    pub fn share(&self) -> f64 {
        self.share
    }

    pub fn margin(&self) -> Duration {
        self.margin
    }

    /// The display's frame period: the shortest recent interval, never more
    /// than 60 Hz's, which it is until an interval has been measured.
    pub fn display_period_ms(&self) -> f64 {
        self.intervals.iter().copied().fold(MAX_PERIOD_MS, f64::min)
    }

    /// The last interval between frames: what the page is actually running at.
    pub fn last_interval_ms(&self) -> f64 {
        self.intervals.back().copied().unwrap_or(MAX_PERIOD_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> Duration {
        Duration::ZERO
    }

    /// Frames at `interval_ms` apart, from 1,000 ms.
    fn after(budget: &mut FrameBudget, frames: usize, interval_ms: f64) {
        let start = budget.last_ms.unwrap_or(1000.0);
        for frame in 1..=frames {
            budget.frame(start + frame as f64 * interval_ms);
        }
    }

    fn budget_us(budget: &FrameBudget) -> u128 {
        budget.budget().per_frame.as_micros()
    }

    /// "Known rate, else capped" on the web (ruled 2026-10-04): the shortest
    /// recent interval, never longer than 1/60 s, so the budget is at most
    /// half of 16.7 ms however slow the page, and less on a faster display
    /// the page keeps up with.
    #[test]
    fn the_display_period_is_the_shortest_recent_interval_capped_at_60_hz() {
        let mut budget = FrameBudget::new(0.5, Duration::from_micros(200), clock);
        assert_eq!(budget_us(&budget), 8_333, "60 Hz before any interval");
        budget.frame(1000.0);
        after(&mut budget, 10, 24.2);
        assert_eq!(budget.display_period_ms(), MAX_PERIOD_MS);
        assert_eq!(
            budget_us(&budget),
            8_333,
            "a page slower than 60 Hz is capped"
        );
        assert!((budget.last_interval_ms() - 24.2).abs() < 1e-9);
        // A 165 Hz display the page keeps up with: half of 6.06 ms.
        after(&mut budget, 10, 1000.0 / 165.0);
        assert_eq!(budget_us(&budget), 3_030);
        // A hidden page's gap is not an interval.
        after(&mut budget, 1, 1500.0);
        assert_eq!(budget_us(&budget), 3_030);
        // The fast intervals age out of the window; the cap holds again.
        after(&mut budget, RECENT_INTERVALS, 48.5);
        assert_eq!(budget_us(&budget), 8_333);
        assert_eq!(budget.budget().margin, Duration::from_micros(200));
    }
}
