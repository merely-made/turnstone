// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The bar a capped speed above real time must reach on the tree page's
//! receipts (physics catalog plan, ruled 2026-10-06, "Half of the lesser"):
//! half the lesser of its factor times the same page's 1x and the speed its
//! step budget fits at the run's measured tick cost. Under load the budget,
//! not the dial, limits 50x, so the bar follows what the budget can hold.
//!
//! The budget-fit speed: the frames' stepping time over their ticks is the
//! tick cost; each frame fits its budget less the gate's margin over that
//! cost, never fewer than the floor ticks it ran; those ticks, in simulated
//! seconds, over the frames' wall time.

use std::time::Duration;

use mere::canvas::TICK_DT;

/// The share of the lesser term the bar asks for.
pub const BAR_SHARE: f32 = 0.5;

/// One frame above real time, as the bar reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitFrame {
    /// Ticks the frame ran.
    pub steps: u32,
    /// Of those, the ones the 1x floor ran before the gate.
    pub floor: u32,
    /// Time spent stepping, on the budget's clock.
    pub compute: Duration,
    /// The frame's step budget.
    pub budget: Duration,
    /// The wall time the frame was driven with.
    pub wall: Duration,
}

/// What the budgets fit at the measured tick cost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetFit {
    /// The mean tick's cost.
    pub tick: Duration,
    /// The mean ticks a frame the budgets fit.
    pub ticks: f32,
    /// The speed those ticks make over the frames' wall time.
    pub speed: f32,
}

/// The budget-fit speed over `frames`, the gate keeping `margin`; `None`
/// until a tick has been timed.
pub fn budget_fit(frames: &[FitFrame], margin: Duration) -> Option<BudgetFit> {
    let steps: u64 = frames.iter().map(|frame| u64::from(frame.steps)).sum();
    let compute: Duration = frames.iter().map(|frame| frame.compute).sum();
    let wall: Duration = frames.iter().map(|frame| frame.wall).sum();
    if steps == 0 || compute.is_zero() || wall.is_zero() {
        return None;
    }
    let tick = compute.as_secs_f64() / steps as f64;
    let fits: f64 = frames
        .iter()
        .map(|frame| {
            let room = frame.budget.saturating_sub(margin).as_secs_f64();
            (room / tick).floor().max(f64::from(frame.floor))
        })
        .sum();
    Some(BudgetFit {
        tick: Duration::from_secs_f64(tick),
        ticks: (fits / frames.len() as f64) as f32,
        speed: (fits * f64::from(TICK_DT) / wall.as_secs_f64()) as f32,
    })
}

/// The bar for `factor` times real time, from the same page's `marked` 1x
/// and what the budget fits.
pub fn bar(factor: f32, marked: f32, fit: BudgetFit) -> f32 {
    BAR_SHARE * (factor * marked).min(fit.speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARGIN: Duration = Duration::from_micros(200);

    /// `count` frames of `steps` ticks at `tick_us` each, `floor` of them the
    /// floor's, under a 3 ms budget, `wall_ms` apart.
    fn frames(count: usize, steps: u32, floor: u32, tick_us: u64, wall_ms: u64) -> Vec<FitFrame> {
        vec![
            FitFrame {
                steps,
                floor,
                compute: Duration::from_micros(tick_us * u64::from(steps)),
                budget: Duration::from_millis(3),
                wall: Duration::from_millis(wall_ms),
            };
            count
        ]
    }

    #[test]
    fn the_budget_fits_its_share_less_the_margin_at_the_measured_tick_cost() {
        // 20 us ticks: 2.8 ms of room fits 140 a frame, 140 sixtieths of a
        // second every 100 ms.
        let fit = budget_fit(&frames(10, 50, 3, 20, 100), MARGIN).unwrap();
        assert_eq!(fit.tick, Duration::from_micros(20));
        assert_eq!(fit.ticks, 140.0);
        assert!((fit.speed - 140.0 / 60.0 / 0.1).abs() < 1e-3, "{fit:?}");
        // Control: ticks twice as dear fit half as many.
        let dear = budget_fit(&frames(10, 50, 3, 40, 100), MARGIN).unwrap();
        assert_eq!(dear.ticks, 70.0);
        assert!((dear.speed * 2.0 - fit.speed).abs() < 1e-3);
        // And the margin is the gate's: without it, 150 fit.
        let unmargined = budget_fit(&frames(10, 50, 3, 20, 100), Duration::ZERO).unwrap();
        assert_eq!(unmargined.ticks, 150.0);
    }

    #[test]
    fn a_floor_past_the_budget_counts_the_floor_it_ran() {
        // 1.5 ms ticks fit one in 2.8 ms, but the floor ran three.
        let fit = budget_fit(&frames(4, 3, 3, 1_500, 200), MARGIN).unwrap();
        assert_eq!(fit.ticks, 3.0);
    }

    #[test]
    fn nothing_fits_until_a_tick_is_timed() {
        assert_eq!(budget_fit(&[], MARGIN), None);
        assert_eq!(budget_fit(&frames(4, 0, 0, 20, 100), MARGIN), None);
    }

    #[test]
    fn the_bar_is_half_the_lesser_of_the_dial_and_the_budget() {
        let fit = |speed| BudgetFit {
            tick: Duration::from_micros(60),
            ticks: 47.0,
            speed,
        };
        // Bound, as under load on the 24-node page: the budget is the lesser.
        assert_eq!(bar(50.0, 0.25, fit(5.8)), 2.9);
        // Free, as calm: 50 times the page's 1x is.
        assert_eq!(bar(50.0, 1.0, fit(833.0)), 25.0);
    }
}
