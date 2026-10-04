// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The simulation-speed dial on both web pages (physics catalog plan, ruled
//! 2026-10-04, "Speed select"): seven presets in the physics section, 1x by
//! default, applied when chosen, with the speed reached shown while the step
//! budget binds. Also the `physics_speed` and `physics_budget_ms` page
//! options and the browser clock the budget is measured on.

use std::time::Duration;

use mere::canvas::{Canvas, Speed, StepBudget};

use crate::web_timing::now_ms;

/// The presets, as option value and label; the default is 1x.
pub(crate) const PRESETS: [(&str, &str); 7] = [
    ("0.2", "0.2x"),
    ("0.5", "0.5x"),
    ("1", "1x"),
    ("2", "2x"),
    ("5", "5x"),
    ("10", "10x"),
    ("50", "50x"),
];
pub(crate) const DEFAULT_PRESET: usize = 2;

/// The budget a frame's ticks above real time may spend unless the page asks
/// otherwise: half a 60 Hz frame (ruled 2026-10-04, "8 ms budget default").
const DEFAULT_BUDGET_MS: f64 = 8.0;

/// How often the reached-speed note may change, in host milliseconds, so a
/// live figure reads rather than flickers.
const NOTE_INTERVAL_MS: f64 = 500.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpeedOptions {
    pub(crate) speed: Speed,
    pub(crate) budget: Duration,
    /// The page asked for a speed or a budget: receipts log the pace.
    pub(crate) explicit: bool,
}

pub(crate) fn options() -> Result<SpeedOptions, String> {
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
pub(crate) fn apply(canvas: &mut Canvas, options: SpeedOptions) {
    canvas.set_physics_speed(options.speed);
    canvas.set_physics_step_budget(Some(StepBudget {
        per_frame: options.budget,
        clock,
    }));
}

/// The preset nearest the canvas's speed (a page option may name any speed).
pub(crate) fn preset_of(canvas: &Canvas) -> usize {
    let speed = canvas.physics_speed().factor();
    PRESETS
        .iter()
        .enumerate()
        .min_by(|(_, (a, _)), (_, (b, _))| {
            let distance =
                |value: &str| (value.parse::<f32>().unwrap_or(1.0).ln() - speed.ln()).abs();
            distance(a).total_cmp(&distance(b))
        })
        .map_or(DEFAULT_PRESET, |(index, _)| index)
}

/// Set preset `index`; the status line the page shows.
pub(crate) fn choose(canvas: &mut Canvas, index: usize) -> String {
    let (value, label) = PRESETS[index.min(PRESETS.len() - 1)];
    canvas.set_physics_speed(Speed::from_factor(value.parse().unwrap_or(1.0)));
    format!("Speed set to {label}")
}

/// What the page says while the budget holds the speed below the one set.
pub(crate) fn reached(canvas: &Canvas) -> Option<String> {
    let pace = canvas.physics_pace();
    let speed = canvas.physics_speed();
    (pace.budget_bound && speed > Speed::REAL_TIME).then(|| {
        let reached = pace.effective_speed.unwrap_or(0.0);
        format!("Running at {reached:.1}x: the frame budget is full")
    })
}

/// The reached-speed note, refreshed at most every [`NOTE_INTERVAL_MS`].
#[derive(Default)]
pub(crate) struct ReachedNote {
    shown: Option<String>,
    at_ms: f64,
}

impl ReachedNote {
    /// The note to show now, and whether it changed.
    pub(crate) fn update(&mut self, canvas: &Canvas) -> (Option<String>, bool) {
        let now = now_ms();
        let next = reached(canvas);
        let changed = next.is_some() != self.shown.is_some()
            || (next != self.shown && now - self.at_ms >= NOTE_INTERVAL_MS);
        if changed {
            self.shown = next;
            self.at_ms = now;
        }
        (self.shown.clone(), changed)
    }
}
