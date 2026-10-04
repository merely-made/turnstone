// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The speed dial's receipts on the tree page: the window of recent frames
//! they read (did the drawing move, did the simulation step, what did
//! stepping cost). The options, the clock and the presets are
//! [`crate::web_speed`]'s, shared with the main page.

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
