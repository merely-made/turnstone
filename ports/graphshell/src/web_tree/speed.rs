// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The speed dial's receipts on the tree page: the window of recent frames
//! they read (did the drawing move, did the simulation step, what did
//! stepping cost against that frame's budget), and the worst frame above
//! real time across every window since the page opened. The options, the
//! clock, the frame-share budget and the presets are [`crate::web_speed`]'s,
//! shared with the main page.

use std::collections::VecDeque;
use std::time::Duration;

use mere::canvas::{Canvas, Speed};
use mere::kernel::graph::NodeKey;
use taproot::ProbeSnapshot;

use super::Shared;

/// Frames the receipts' window holds.
const WINDOW: usize = 30;
/// Nodes whose drawn positions stand for the drawing.
const SAMPLE: usize = 8;
/// Pace lines a dial run writes into the receipt, at most.
const LOG_LINES: usize = 40;

/// The last frames while the layout moved.
#[derive(Default)]
pub(super) struct PaceWindow {
    sample: Vec<NodeKey>,
    last: Option<Vec<(f32, f32)>>,
    frames: VecDeque<Frame>,
    since_log: usize,
    logged: usize,
    /// Every frame above real time since the page opened: how many, the
    /// most one ran over its budget (µs), and how many ran past the budget
    /// and the clock's grain.
    above: usize,
    worst_over_budget_us: Option<i64>,
    over_grain: usize,
    /// The worst frame's own budget, stepping time and ticks, and which
    /// frame above real time it was (from 1).
    worst_frame: Option<(Duration, Duration, u32, usize)>,
}

/// Record the frame just drawn under `budget`; on a dial run, every `WINDOW`
/// moving frames, write the pace into the receipt's physics log.
pub(super) fn record(shared: &Shared, canvas: &Canvas, moving: bool, budget: Duration) {
    let mut window = shared.pace.borrow_mut();
    window.record(canvas, moving, budget);
    if !shared.speed.explicit || !moving || window.logged == LOG_LINES {
        return;
    }
    window.since_log += 1;
    if window.since_log < WINDOW {
        return;
    }
    window.since_log = 0;
    window.logged += 1;
    let pace = canvas.physics_pace();
    let summary = window.summary();
    let frame_budget = shared.frame_budget.borrow();
    shared.physics_log.borrow_mut().push(format!(
        "pace: speed {} effective {} bound {} ticks {} over {} frames drawn-moved {} \
         stepped {} compute-max {} us over-budget-max {} us budget {} us ({} of a {:.2} ms \
         display period, {} {}; last frame {:.1} ms) margin {} us; every window: worst {} us, \
         {} frames past the grain",
        crate::web_speed::field(canvas.physics_speed()),
        pace.effective_speed
            .map_or_else(|| "none".into(), |speed| format!("{speed:.3}")),
        pace.budget_bound,
        pace.ticks,
        summary.frames,
        summary.moved,
        summary.stepped,
        summary.compute_max.as_micros(),
        summary.over_budget_us,
        budget.as_micros(),
        frame_budget.share(),
        frame_budget.display_period_ms(),
        crate::web_speed::period_fields(&frame_budget).0,
        crate::web_speed::period_fields(&frame_budget).1,
        frame_budget.last_interval_ms(),
        frame_budget.margin().as_micros(),
        window.worst_field(),
        window.over_grain,
    ));
}

#[derive(Clone, Copy)]
struct Frame {
    drawn_moved: bool,
    stepped: bool,
    compute: Duration,
    /// The step budget this frame ran under.
    budget: Duration,
}

/// What the window holds, for the snapshot and the log.
struct Summary {
    frames: usize,
    moved: usize,
    stepped: usize,
    compute_max: Duration,
    /// The most any frame's stepping ran over its own budget, in µs
    /// (negative when every frame stayed under).
    over_budget_us: i64,
}

impl PaceWindow {
    pub(super) fn record(&mut self, canvas: &Canvas, moving: bool, budget: Duration) {
        if self.sample.is_empty() {
            let mut keys: Vec<NodeKey> = canvas.graph().nodes().map(|(key, _)| key).collect();
            keys.sort_by_key(|key| key.index());
            keys.truncate(SAMPLE);
            self.sample = keys;
        }
        let drawn: Vec<(f32, f32)> = self
            .sample
            .iter()
            .filter_map(|&key| canvas.screen_position_of(key))
            .collect();
        let previous = self.last.replace(drawn);
        if !moving {
            self.frames.clear();
            return;
        }
        let report = canvas.elapsed_step_report().unwrap_or_default();
        if canvas.physics_speed() > Speed::REAL_TIME {
            let micros = |duration: Duration| duration.as_micros().min(i64::MAX as u128) as i64;
            let over = micros(report.compute.unwrap_or_default()) - micros(budget);
            self.above += 1;
            if self.worst_over_budget_us.is_none_or(|worst| over > worst) {
                self.worst_frame = Some((
                    budget,
                    report.compute.unwrap_or_default(),
                    report.steps,
                    self.above,
                ));
            }
            self.worst_over_budget_us =
                Some(self.worst_over_budget_us.map_or(over, |w| w.max(over)));
            if over > crate::web_speed::CLOCK_GRAIN_US as i64 {
                self.over_grain += 1;
            }
        }
        if self.frames.len() == WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back(Frame {
            drawn_moved: previous.as_ref() != self.last.as_ref(),
            stepped: report.steps > 0,
            compute: report.compute.unwrap_or_default(),
            budget,
        });
    }

    /// The worst frame above real time so far, or "none".
    fn worst_field(&self) -> String {
        self.worst_over_budget_us
            .map_or_else(|| "none".into(), |worst| worst.to_string())
    }

    fn summary(&self) -> Summary {
        let count = |f: fn(&Frame) -> bool| self.frames.iter().filter(|frame| f(frame)).count();
        let micros = |duration: Duration| duration.as_micros().min(i64::MAX as u128) as i64;
        Summary {
            frames: self.frames.len(),
            moved: count(|f| f.drawn_moved),
            stepped: count(|f| f.stepped),
            compute_max: self
                .frames
                .iter()
                .map(|frame| frame.compute)
                .max()
                .unwrap_or_default(),
            over_budget_us: self
                .frames
                .iter()
                .map(|frame| micros(frame.compute) - micros(frame.budget))
                .max()
                .unwrap_or(i64::MIN),
        }
    }
}

/// `log-pace <label>`: the budget and every window's worst frame, for the
/// receipt.
pub(super) fn pace_line(label: &str, canvas: &Canvas, shared: &Shared) -> String {
    let window = shared.pace.borrow();
    let frame_budget = shared.frame_budget.borrow();
    format!(
        "pace {label}: speed {} budget {} us margin {} us display period {:.3} ms ({}, worst \
         interval {} ms off its multiple); every window: {} frames above real time, worst {} us \
         over budget ({}), {} past the grain",
        crate::web_speed::field(canvas.physics_speed()),
        frame_budget.budget().per_frame.as_micros(),
        frame_budget.margin().as_micros(),
        frame_budget.display_period_ms(),
        crate::web_speed::period_fields(&frame_budget).0,
        crate::web_speed::period_fields(&frame_budget).1,
        window.above,
        window.worst_field(),
        window.worst_frame.map_or_else(
            || "no frame".into(),
            |(budget, compute, steps, nth)| format!(
                "frame {nth}: {} us stepping {steps} ticks against a {} us budget",
                compute.as_micros(),
                budget.as_micros()
            )
        ),
        window.over_grain,
    )
}

/// `log-intervals <label>`: the recent frame intervals the display period is
/// read from, for a diagnostic to fit offline.
pub(super) fn intervals_line(label: &str, shared: &Shared) -> String {
    let frame_budget = shared.frame_budget.borrow();
    let intervals: Vec<String> = frame_budget
        .intervals()
        .map(|ms| format!("{ms:.3}"))
        .collect();
    format!("intervals {label}: {}", intervals.join(" "))
}

/// The dial's fields on the lane's snapshot.
pub(super) fn fields(snapshot: ProbeSnapshot, canvas: &Canvas, shared: &Shared) -> ProbeSnapshot {
    let pace = canvas.physics_pace();
    let step = canvas.elapsed_step_report().unwrap_or_default();
    let summary = shared.pace.borrow().summary();
    let frame_budget = shared.frame_budget.borrow();
    snapshot
        .with_field(
            "physics-speed",
            crate::web_speed::field(canvas.physics_speed()),
        )
        .with_field(
            "physics-effective-speed",
            pace.effective_speed
                .map_or_else(|| "none".into(), |speed| format!("{speed:.3}")),
        )
        .with_field("physics-budget-bound", pace.budget_bound.to_string())
        .with_field(
            "physics-budget-us",
            frame_budget.budget().per_frame.as_micros().to_string(),
        )
        .with_field("physics-budget-share", frame_budget.share().to_string())
        .with_field(
            "display-period-ms",
            format!("{:.2}", frame_budget.display_period_ms()),
        )
        .with_field(
            "display-period-source",
            crate::web_speed::period_fields(&frame_budget).0,
        )
        .with_field(
            "frame-interval-ms",
            format!("{:.1}", frame_budget.last_interval_ms()),
        )
        .with_field("physics-ticks", pace.ticks.to_string())
        .with_field("physics-owed-steps", step.owed_steps.to_string())
        .with_field("pace-frames", summary.frames.to_string())
        .with_field("pace-drawn-moved", summary.moved.to_string())
        .with_field("pace-stepped", summary.stepped.to_string())
        .with_field(
            "pace-compute-max-us",
            summary.compute_max.as_micros().to_string(),
        )
        .with_field("pace-over-budget-us", summary.over_budget_us.to_string())
        .with_field(
            "pace-over-budget-worst-us",
            shared.pace.borrow().worst_field(),
        )
        .with_field(
            "pace-over-grain-frames",
            shared.pace.borrow().over_grain.to_string(),
        )
        .with_field(
            "physics-budget-margin-us",
            frame_budget.margin().as_micros().to_string(),
        )
}
