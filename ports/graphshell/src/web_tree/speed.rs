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
    /// Every frame above real time since the page opened in which the gate
    /// admitted ticks past the 1x floor (ruled 2026-10-05, "Only ticks past
    /// the floor"): how many, the most the admitted ticks ran past the
    /// frame's budget (µs), and how many ran past it and the clock's grain.
    above: usize,
    worst_over_budget_us: Option<i64>,
    over_grain: usize,
    /// The worst such frame's own budget, the end of its admitted ticks, its
    /// ticks in all, and which frame above real time it was (from 1).
    worst_frame: Option<(Duration, Duration, u32, usize)>,
    /// Frames above real time whose floor alone ran past the budget and the
    /// grain, the gate admitting nothing: the floor's, not the gate's.
    floor_over: usize,
    /// The effective speed `mark-pace` recorded, to compare a later speed
    /// with on the same page (ruled 2026-10-06, "Relative to the page's 1x").
    marked: Option<f32>,
    /// The page's measured clock step, in us: what "past the grain" means.
    grain_us: i64,
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
         display period, {} at {} from the {}; last frame {:.1} ms) margin {} us; every window: \
         worst {} us, {} frames past the grain",
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
        frame_budget.source().label(),
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
    /// When the ticks the gate admitted past the floor ended, if any.
    admitted_until: Option<Duration>,
    /// The step budget this frame ran under.
    budget: Duration,
}

/// What the window holds, for the snapshot and the log.
struct Summary {
    frames: usize,
    moved: usize,
    stepped: usize,
    compute_max: Duration,
    /// The most the ticks the gate admitted ran past their frame's budget,
    /// in µs (negative when every frame stayed under; floor-only frames are
    /// not the gate's).
    over_budget_us: i64,
}

impl PaceWindow {
    /// A window whose "past the grain" is the page's measured clock step.
    pub(super) fn with_grain(grain_us: i64) -> Self {
        Self {
            grain_us,
            ..Self::default()
        }
    }

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
            let grain = self.grain_us;
            match report.admitted_until {
                Some(until) if report.admitted > 0 => {
                    let over = micros(until) - micros(budget);
                    self.above += 1;
                    if self.worst_over_budget_us.is_none_or(|worst| over > worst) {
                        self.worst_frame = Some((budget, until, report.steps, self.above));
                    }
                    self.worst_over_budget_us =
                        Some(self.worst_over_budget_us.map_or(over, |w| w.max(over)));
                    if over > grain {
                        self.over_grain += 1;
                    }
                },
                _ => {
                    if micros(report.compute.unwrap_or_default()) - micros(budget) > grain {
                        self.floor_over += 1;
                    }
                },
            }
        }
        if self.frames.len() == WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back(Frame {
            drawn_moved: previous.as_ref() != self.last.as_ref(),
            stepped: report.steps > 0,
            compute: report.compute.unwrap_or_default(),
            admitted_until: report.admitted_until.filter(|_| report.admitted > 0),
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
                .filter_map(|frame| {
                    frame
                        .admitted_until
                        .map(|until| micros(until) - micros(frame.budget))
                })
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
    let shown = |speed: Option<f32>| speed.map_or_else(|| "none".into(), |s| format!("{s:.3}"));
    let effective = canvas.physics_pace().effective_speed;
    format!(
        "pace {label}: speed {} effective {} (marked {}, ratio {}) budget {} us margin {} us \
         display period {:.3} ms ({}, fitting {} of the recent intervals from the {}, a {} us \
         clock); every window: {} \
         frames the gate admitted ticks in, worst {} us over budget ({}), {} past the grain; \
         {} floor-only frames past it",
        crate::web_speed::field(canvas.physics_speed()),
        shown(effective),
        shown(window.marked),
        match (effective, window.marked) {
            (Some(now), Some(marked)) if marked > 0.0 => format!("{:.3}", now / marked),
            _ => "none".into(),
        },
        frame_budget.budget().per_frame.as_micros(),
        frame_budget.margin().as_micros(),
        frame_budget.display_period_ms(),
        crate::web_speed::period_fields(&frame_budget).0,
        crate::web_speed::period_fields(&frame_budget).1,
        frame_budget.source().label(),
        window.grain_us,
        window.above,
        window.worst_field(),
        window.worst_frame.map_or_else(
            || "no frame".into(),
            |(budget, until, steps, nth)| format!(
                "frame {nth}: admitted ticks ended at {} us of {steps} ticks against a {} us \
                 budget",
                until.as_micros(),
                budget.as_micros()
            )
        ),
        window.over_grain,
        window.floor_over,
    )
}

/// `mark-pace <label>`: record the effective speed now, for the receipt to
/// compare a later one with, and log it.
pub(super) fn mark_line(label: &str, canvas: &Canvas, shared: &Shared) -> String {
    let effective = canvas.physics_pace().effective_speed;
    shared.pace.borrow_mut().marked = effective;
    format!(
        "pace mark {label}: speed {} effective {}",
        crate::web_speed::field(canvas.physics_speed()),
        effective.map_or_else(|| "none".into(), |speed| format!("{speed:.3}")),
    )
}

/// `log-intervals <label>`: the recent frame intervals the display period is
/// read from, for a diagnostic to fit offline.
pub(super) fn intervals_line(label: &str, shared: &Shared) -> String {
    let frame_budget = shared.frame_budget.borrow();
    let main: Vec<String> = frame_budget
        .intervals()
        .map(|ms| format!("{ms:.3}"))
        .collect();
    let worker: Vec<String> = frame_budget
        .worker_interval_log()
        .map(|ms| format!("{ms:.3}"))
        .collect();
    format!(
        "intervals {label}: {}\nintervals-worker {label}: {}",
        main.join(" "),
        worker.join(" "),
    )
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
        .with_field("display-period-from", frame_budget.source().label())
        .with_field(
            "clock-grain-us",
            format!("{:.0}", frame_budget.grain_ms() * 1000.0),
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
        .with_field("pace-gated-frames", shared.pace.borrow().above.to_string())
        .with_field(
            "physics-effective-marked",
            shared
                .pace
                .borrow()
                .marked
                .map_or_else(|| "none".into(), |speed| format!("{speed:.3}")),
        )
        .with_field(
            "physics-effective-over-marked",
            match (pace.effective_speed, shared.pace.borrow().marked) {
                (Some(now), Some(marked)) if marked > 0.0 => format!("{:.3}", now / marked),
                _ => "none".into(),
            },
        )
        .with_field(
            "pace-floor-over-frames",
            shared.pace.borrow().floor_over.to_string(),
        )
        .with_field(
            "physics-budget-margin-us",
            frame_budget.margin().as_micros().to_string(),
        )
}
