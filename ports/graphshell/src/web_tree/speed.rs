// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The speed dial on the tree page: the `physics_speed` and
//! `physics_budget_ms` page options, the browser clock the budget is measured
//! on, and the window of recent frames the receipts read (did the drawing
//! move, did the simulation step, what did stepping cost).

use std::collections::VecDeque;
use std::time::Duration;

use mere::canvas::{Canvas, Speed, StepBudget};
use mere::kernel::graph::NodeKey;
use taproot::ProbeSnapshot;

use super::Shared;
use crate::web_timing::now_ms;

/// The budget a frame's ticks above real time may spend unless the page asks
/// otherwise: half a 60 Hz frame. Provisional, put to Mark with the control.
const DEFAULT_BUDGET_MS: f64 = 8.0;
/// Frames the receipts' window holds.
const WINDOW: usize = 30;
/// Nodes whose drawn positions stand for the drawing.
const SAMPLE: usize = 8;

#[derive(Clone, Copy, Debug)]
pub(super) struct SpeedOptions {
    pub(super) speed: Speed,
    pub(super) budget: Duration,
    /// The page asked for a speed or a budget: log the pace into the receipt.
    explicit: bool,
}

pub(super) fn options() -> Result<SpeedOptions, String> {
    let search = web_sys::window()
        .ok_or("no window")?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    let speed = match params.get("physics_speed") {
        Some(value) => Speed::from_factor(value.parse().map_err(|_| "invalid physics_speed")?),
        None => Speed::REAL_TIME,
    };
    let budget_ms = match params.get("physics_budget_ms") {
        Some(value) => value.parse().map_err(|_| "invalid physics_budget_ms")?,
        None => DEFAULT_BUDGET_MS,
    };
    Ok(SpeedOptions {
        speed,
        budget: Duration::from_secs_f64(f64::max(budget_ms, 0.0) / 1000.0),
        explicit: params.has("physics_speed") || params.has("physics_budget_ms"),
    })
}

fn clock() -> Duration {
    Duration::from_secs_f64(now_ms().max(0.0) / 1000.0)
}

/// Give the canvas the page's speed and a budget on the browser clock.
pub(super) fn apply(canvas: &mut Canvas, options: SpeedOptions) {
    canvas.set_physics_speed(options.speed);
    canvas.set_physics_step_budget(Some(StepBudget {
        per_frame: options.budget,
        clock,
    }));
}

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

/// Record the frame just drawn; on a dial run, every `WINDOW` moving frames,
/// write the pace into the receipt's physics log.
pub(super) fn record(shared: &Shared, canvas: &Canvas, moving: bool) {
    let mut window = shared.pace.borrow_mut();
    window.record(canvas, moving);
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
    let (frames, moved, stepped, compute) = window.summary();
    shared.physics_log.borrow_mut().push(format!(
        "pace: speed {} effective {} bound {} ticks {} over {frames} frames drawn-moved {moved} \
         stepped {stepped} compute-max {} us budget {} us",
        canvas.physics_speed().factor(),
        pace.effective_speed
            .map_or_else(|| "none".into(), |speed| format!("{speed:.3}")),
        pace.budget_bound,
        pace.ticks,
        compute.as_micros(),
        shared.speed.budget.as_micros(),
    ));
}

#[derive(Clone, Copy)]
struct Frame {
    drawn_moved: bool,
    stepped: bool,
    compute: Duration,
}

impl PaceWindow {
    pub(super) fn record(&mut self, canvas: &Canvas, moving: bool) {
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
        });
    }

    /// Frames held, frames whose drawing moved, frames that stepped, and the
    /// costliest stepping.
    fn summary(&self) -> (usize, usize, usize, Duration) {
        let count = |f: fn(&Frame) -> bool| self.frames.iter().filter(|frame| f(frame)).count();
        let compute = self.frames.iter().map(|frame| frame.compute).max();
        (
            self.frames.len(),
            count(|f| f.drawn_moved),
            count(|f| f.stepped),
            compute.unwrap_or_default(),
        )
    }
}

/// The dial's fields on the lane's snapshot.
pub(super) fn fields(snapshot: ProbeSnapshot, canvas: &Canvas, shared: &Shared) -> ProbeSnapshot {
    let pace = canvas.physics_pace();
    let step = canvas.elapsed_step_report().unwrap_or_default();
    let (frames, moved, stepped, compute_max) = shared.pace.borrow().summary();
    snapshot
        .with_field("physics-speed", canvas.physics_speed().factor().to_string())
        .with_field(
            "physics-effective-speed",
            pace.effective_speed
                .map_or_else(|| "none".into(), |speed| format!("{speed:.3}")),
        )
        .with_field("physics-budget-bound", pace.budget_bound.to_string())
        .with_field(
            "physics-budget-us",
            shared.speed.budget.as_micros().to_string(),
        )
        .with_field("physics-ticks", pace.ticks.to_string())
        .with_field("physics-owed-steps", step.owed_steps.to_string())
        .with_field("pace-frames", frames.to_string())
        .with_field("pace-drawn-moved", moved.to_string())
        .with_field("pace-stepped", stepped.to_string())
        .with_field("pace-compute-max-us", compute_max.as_micros().to_string())
}
