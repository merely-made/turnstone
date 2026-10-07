// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The step budget both web pages give the canvas and the remote board: a
//! share of the display's frame period (physics catalog plan, ruled
//! 2026-10-04, "The display's frame"). A browser does not say its display's
//! rate, but a page's frames start on the display's refreshes, so every
//! interval between them is a whole number of periods: the period is the
//! largest between 1/360 s and 1/60 s that every recent interval is a whole
//! multiple of, within the clock's grain ("Infer the period"). Taken from the
//! intervals, not their length, it does not grow when physics slows the
//! page. When nothing fits, the shortest recent interval stands in, never
//! longer than 1/60 s ("Known rate, else capped"). The page passes its clock,
//! that clock's grain, and the margin its gate keeps ("Gate keeps a forecast
//! margin").

use std::collections::VecDeque;
use std::time::Duration;

use mere::canvas::{FALLBACK_DISPLAY_PERIOD, StepBudget};

/// The longest the display's period is taken to be, and the period assumed
/// until the page has measured an interval: 60 Hz's.
pub const MAX_PERIOD_MS: f64 = FALLBACK_DISPLAY_PERIOD.as_nanos() as f64 / 1e6;
/// The shortest the display's period is taken to be: 360 Hz's.
pub const MIN_PERIOD_MS: f64 = 1000.0 / 360.0;
/// A gap longer than this between frames is a hidden or suspended page, not a
/// frame, and is not taken in.
const GAP_MS: f64 = 1000.0;
/// The recent intervals the period is read from: about two seconds at 60 Hz,
/// half a second at 240 Hz.
const RECENT_INTERVALS: usize = 120;
/// How far past its bounds a fitted period may fall and be taken as the
/// bound: a 60 Hz panel's measured period can read a hair over 1/60 s.
const BOUND_SLACK: f64 = 0.01;

/// Where the display period came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Period {
    /// Every recent interval is a whole multiple of `ms`, the worst within
    /// `residual_ms` of its multiple.
    Inferred { ms: f64, residual_ms: f64 },
    /// No period fitted: the shortest recent interval at most 1/60 s, or 1/60 s
    /// before any interval. `nearest_ms` is how close the best candidate came.
    Fallback { ms: f64, nearest_ms: Option<f64> },
}

impl Period {
    pub fn ms(self) -> f64 {
        match self {
            Period::Inferred { ms, .. } | Period::Fallback { ms, .. } => ms,
        }
    }
}

/// The step budget as a share of the display's frame period, read from the
/// last [`RECENT_INTERVALS`] intervals between frames.
#[derive(Clone, Debug)]
pub struct FrameBudget {
    share: f64,
    margin: Duration,
    clock: fn() -> Duration,
    grain_ms: f64,
    intervals: VecDeque<f64>,
    last_ms: Option<f64>,
    period: Period,
}

impl FrameBudget {
    /// `share` of the period, measured on `clock`, whose readings come in
    /// steps of `grain`, the gate keeping `margin` past the forecast tick.
    pub fn new(share: f64, margin: Duration, clock: fn() -> Duration, grain: Duration) -> Self {
        Self {
            share,
            margin,
            clock,
            grain_ms: grain.as_secs_f64() * 1000.0,
            intervals: VecDeque::with_capacity(RECENT_INTERVALS),
            last_ms: None,
            period: Period::Fallback {
                ms: MAX_PERIOD_MS,
                nearest_ms: None,
            },
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
                self.period = period_of(&self.intervals, self.grain_ms);
            }
        }
        self.last_ms = Some(now_ms);
        self.budget()
    }

    pub fn budget(&self) -> StepBudget {
        StepBudget::of_period(
            Duration::from_secs_f64(self.period.ms() / 1000.0),
            self.share,
            self.clock,
            self.margin,
        )
    }

    pub fn share(&self) -> f64 {
        self.share
    }

    pub fn margin(&self) -> Duration {
        self.margin
    }

    /// The display's frame period and where it came from.
    pub fn period(&self) -> Period {
        self.period
    }

    pub fn display_period_ms(&self) -> f64 {
        self.period.ms()
    }

    /// The recent intervals the period is read from, oldest first, in ms.
    pub fn intervals(&self) -> impl Iterator<Item = f64> + '_ {
        self.intervals.iter().copied()
    }

    /// The last interval between frames: what the page is actually running at.
    pub fn last_interval_ms(&self) -> f64 {
        self.intervals.back().copied().unwrap_or(MAX_PERIOD_MS)
    }
}

/// The largest period in bounds that every interval is a whole multiple of
/// within `grain_ms`, tried at the shortest interval divided by 1, 2, 3, ...
/// and refined over all of them; else the fallback.
fn period_of(intervals: &VecDeque<f64>, grain_ms: f64) -> Period {
    let Some(shortest) = intervals.iter().copied().reduce(f64::min) else {
        return Period::Fallback {
            ms: MAX_PERIOD_MS,
            nearest_ms: None,
        };
    };
    let first = (shortest / (MAX_PERIOD_MS * (1.0 + BOUND_SLACK)))
        .ceil()
        .max(1.0) as u32;
    let last = (shortest / (MIN_PERIOD_MS * (1.0 - BOUND_SLACK))).floor() as u32;
    let mut nearest: Option<f64> = None;
    for k in first..=last {
        let (ms, residual_ms) = fitted(intervals, shortest / f64::from(k));
        if !(MIN_PERIOD_MS * (1.0 - BOUND_SLACK)..=MAX_PERIOD_MS * (1.0 + BOUND_SLACK))
            .contains(&ms)
        {
            continue;
        }
        if residual_ms <= grain_ms {
            return Period::Inferred {
                ms: ms.clamp(MIN_PERIOD_MS, MAX_PERIOD_MS),
                residual_ms,
            };
        }
        nearest = Some(nearest.map_or(residual_ms, |n: f64| n.min(residual_ms)));
    }
    Period::Fallback {
        ms: shortest.clamp(MIN_PERIOD_MS, MAX_PERIOD_MS),
        nearest_ms: nearest,
    }
}

/// The period near `guess` that the intervals are multiples of (their total
/// over their multiples), and the worst interval's distance from its multiple.
fn fitted(intervals: &VecDeque<f64>, guess: f64) -> (f64, f64) {
    let (total, multiples) = intervals.iter().fold((0.0, 0.0), |(total, multiples), &i| {
        (total + i, multiples + (i / guess).round().max(1.0))
    });
    let ms = total / multiples;
    let residual = intervals
        .iter()
        .map(|&i| (i - (i / ms).round().max(1.0) * ms).abs())
        .fold(0.0, f64::max);
    (ms, residual)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAIN: Duration = Duration::from_micros(100);

    fn clock() -> Duration {
        Duration::ZERO
    }

    fn fresh() -> FrameBudget {
        FrameBudget::new(0.5, Duration::from_micros(200), clock, GRAIN)
    }

    /// Frames `vsyncs` refreshes of a `hz` display apart, each timestamp read
    /// in the clock's 100 us steps, from 1,000 ms.
    fn refreshes(budget: &mut FrameBudget, hz: f64, vsyncs: &[u32], repeat: usize) {
        let period = 1000.0 / hz;
        let mut at = budget.last_ms.unwrap_or(1000.0);
        let mut true_at = at;
        if budget.last_ms.is_none() {
            budget.frame(at);
        }
        for &n in vsyncs.iter().cycle().take(vsyncs.len() * repeat) {
            true_at += f64::from(n) * period;
            at = (true_at * 10.0).floor() / 10.0;
            budget.frame(at);
        }
    }

    fn budget_us(budget: &FrameBudget) -> u128 {
        budget.budget().per_frame.as_micros()
    }

    fn inferred(budget: &FrameBudget) -> f64 {
        match budget.period() {
            Period::Inferred { ms, residual_ms } => {
                assert!(residual_ms <= 0.1, "residual {residual_ms}");
                ms
            },
            other => panic!("not inferred: {other:?}"),
        }
    }

    /// "Infer the period" (ruled 2026-10-04): a page that misses refreshes
    /// still reads its display's period, at 60, 120, 144 and 165 Hz, from
    /// intervals of mixed whole multiples read in 100 us steps.
    #[test]
    fn the_period_is_inferred_from_whole_multiples_of_the_refresh() {
        for (hz, vsyncs, us) in [
            (60.0, &[1, 1, 2, 1, 3][..], 8_333),
            (120.0, &[2, 3, 2, 4, 5][..], 4_166),
            (144.0, &[3, 4, 5, 3, 7][..], 3_472),
            (165.0, &[4, 5, 6, 7, 8, 9, 10, 14, 17][..], 3_030),
        ] {
            let mut budget = fresh();
            refreshes(&mut budget, hz, vsyncs, 15);
            let ms = inferred(&budget);
            assert!((ms - 1000.0 / hz).abs() < 0.01, "{hz} Hz read {ms} ms");
            // Timestamps read in steps put the read period within a
            // microsecond of the true one over these intervals.
            assert!(
                budget_us(&budget).abs_diff(us) <= 1,
                "{hz} Hz: {}",
                budget_us(&budget)
            );
        }
    }

    /// A page that only ever takes an even number of refreshes reads twice
    /// the period, an overestimate: the budget twice half the display's
    /// frame. The cap still bounds it: at 60 Hz two refreshes are past it, so
    /// the period read is the true one.
    #[test]
    fn even_multiples_overestimate_the_period() {
        let mut budget = fresh();
        refreshes(&mut budget, 165.0, &[2, 4, 6, 4, 8], 20);
        assert!((inferred(&budget) - 2000.0 / 165.0).abs() < 0.01);
        assert_eq!(budget_us(&budget), 6_060);
        let mut budget = fresh();
        refreshes(&mut budget, 60.0, &[2, 4, 2], 20);
        assert_eq!(
            budget_us(&budget),
            8_333,
            "two 60 Hz refreshes are past the cap"
        );
    }

    /// The fallback ("Known rate, else capped"): 1/60 s before any interval;
    /// intervals that share no period leave the shortest, at most 1/60 s; a
    /// hidden page's gap is not an interval; old intervals age out.
    #[test]
    fn without_a_common_period_the_shortest_interval_is_capped_at_60_hz() {
        let mut budget = fresh();
        assert_eq!(budget_us(&budget), 8_333, "60 Hz before any interval");
        budget.frame(1000.0);
        for interval in [23.7, 31.9, 27.3, 41.1, 25.6] {
            let at = budget.last_ms.unwrap() + interval;
            budget.frame(at);
        }
        assert!(matches!(budget.period(), Period::Fallback { .. }));
        assert_eq!(budget_us(&budget), 8_333, "a slow page is capped");
        let mut budget = fresh();
        budget.frame(1000.0);
        for interval in [7.25, 9.125, 8.5, 7.875] {
            let at = budget.last_ms.unwrap() + interval;
            budget.frame(at);
        }
        assert!(matches!(budget.period(), Period::Fallback { ms, .. } if ms == 7.25));
        assert_eq!(budget_us(&budget), 3_625);

        let mut budget = fresh();
        refreshes(&mut budget, 165.0, &[4, 5], 10);
        assert_eq!(budget_us(&budget), 3_030);
        let at = budget.last_ms.unwrap() + 1500.0;
        budget.frame(at);
        assert_eq!(budget_us(&budget), 3_030, "the gap is not an interval");
        refreshes(&mut budget, 60.0, &[1], RECENT_INTERVALS);
        assert_eq!(budget_us(&budget), 8_333, "165 Hz's intervals aged out");
        assert_eq!(budget.budget().margin, Duration::from_micros(200));
    }
}
