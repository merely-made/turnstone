// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The speed dial's receipts on the tree page: the window of recent frames
//! they read (did the drawing move, did the simulation step, what did
//! stepping cost against that frame's budget). The options, the clock, the
//! frame-share budget and the presets are [`crate::web_speed`]'s, shared with
//! the main page.

use std::collections::VecDeque;
use std::time::Duration;

use mere::canvas::Canvas;
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
         stepped {} compute-max {} us over-budget-max {} us budget {} us ({} of a {:.1} ms \
         display period; last frame {:.1} ms)",
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
        frame_budget.last_interval_ms(),
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
            format!("{:.1}", frame_budget.display_period_ms()),
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
}
