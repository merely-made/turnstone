// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The simulation-speed dial on both web pages (physics catalog plan, ruled
//! 2026-10-04, "Speed select" and "Target + Max, budget as frame share"):
//! presets from 0.2x to 50x and Max in the physics section, 1x by default,
//! applied when chosen, with the speed reached shown while the layout moves.
//! The step budget is a share of each frame, measured from the page's own
//! frame timestamps, so every machine gets the same bound. Also the
//! `physics_speed` and `physics_budget_share` page options and the browser
//! clock the budget is measured on.

use std::time::Duration;

use mere::canvas::{Canvas, Speed, StepBudget};

use crate::web_timing::now_ms;

/// The presets, as option value and label; the default is 1x. Max is as fast
/// as the budget allows.
pub(crate) const PRESETS: [(&str, &str); 8] = [
    ("0.2", "0.2x"),
    ("0.5", "0.5x"),
    ("1", "1x"),
    ("2", "2x"),
    ("5", "5x"),
    ("10", "10x"),
    ("50", "50x"),
    (MAX_VALUE, "Max"),
];
pub(crate) const DEFAULT_PRESET: usize = 2;
const MAX_VALUE: &str = "max";

/// The share of each frame a frame's ticks above real time may spend unless
/// the page asks otherwise (ruled 2026-10-04: 50%).
const DEFAULT_BUDGET_SHARE: f64 = 0.5;
/// The frame interval assumed until the page has measured one: 60 Hz.
const FIRST_INTERVAL_MS: f64 = 1000.0 / 60.0;
/// A gap longer than this between frames is a hidden or suspended page, not a
/// frame, and does not move the measured interval.
const GAP_MS: f64 = 1000.0;
/// How much of each new interval the measured interval takes in, so one slow
/// frame does not swing the budget.
const INTERVAL_WEIGHT: f64 = 0.25;
/// How often the reached-speed note may change, in host milliseconds, so a
/// live figure reads rather than flickers.
const NOTE_INTERVAL_MS: f64 = 500.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpeedOptions {
    pub(crate) speed: Speed,
    /// The share of each frame the step budget is, in (0, 1].
    pub(crate) share: f64,
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
    let speed = match params.get("physics_speed").as_deref() {
        Some(MAX_VALUE) => Speed::UNCAPPED,
        Some(value) => Speed::from_factor(value.parse().map_err(|_| "invalid physics_speed")?),
        None => Speed::REAL_TIME,
    };
    let share = match params.get("physics_budget_share") {
        Some(value) => value
            .parse::<f64>()
            .ok()
            .filter(|share| *share > 0.0 && *share <= 1.0)
            .ok_or("physics_budget_share wants a share in (0, 1]")?,
        None => DEFAULT_BUDGET_SHARE,
    };
    Ok(SpeedOptions {
        speed,
        share,
        explicit: params.has("physics_speed") || params.has("physics_budget_share"),
    })
}

fn clock() -> Duration {
    Duration::from_secs_f64(now_ms().max(0.0) / 1000.0)
}

/// The step budget as a share of the page's measured frame interval.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameBudget {
    share: f64,
    interval_ms: Option<f64>,
    last_ms: Option<f64>,
}

impl FrameBudget {
    pub(crate) fn new(share: f64) -> Self {
        Self {
            share,
            interval_ms: None,
            last_ms: None,
        }
    }

    /// Take this frame's timestamp into the measured interval; the budget for
    /// the frame.
    pub(crate) fn frame(&mut self, now_ms: f64) -> StepBudget {
        if let Some(last) = self.last_ms {
            let interval = now_ms - last;
            if interval > 0.0 && interval < GAP_MS {
                self.interval_ms = Some(match self.interval_ms {
                    Some(measured) => measured + (interval - measured) * INTERVAL_WEIGHT,
                    None => interval,
                });
            }
        }
        self.last_ms = Some(now_ms);
        self.budget()
    }

    pub(crate) fn budget(&self) -> StepBudget {
        StepBudget {
            per_frame: Duration::from_secs_f64(self.share * self.interval_ms() / 1000.0),
            clock,
        }
    }

    pub(crate) fn share(&self) -> f64 {
        self.share
    }

    /// The measured frame interval, or 60 Hz's until a frame has been seen.
    pub(crate) fn interval_ms(&self) -> f64 {
        self.interval_ms.unwrap_or(FIRST_INTERVAL_MS)
    }
}

/// Give the canvas the page's speed and its first budget.
pub(crate) fn apply(canvas: &mut Canvas, options: SpeedOptions, budget: &FrameBudget) {
    canvas.set_physics_speed(options.speed);
    canvas.set_physics_step_budget(Some(budget.budget()));
}

/// The preset showing the canvas's speed (a page option may name any speed:
/// the nearest).
pub(crate) fn preset_of(canvas: &Canvas) -> usize {
    let speed = canvas.physics_speed();
    if speed == Speed::UNCAPPED {
        return PRESETS.len() - 1;
    }
    let factor = speed.factor();
    PRESETS[..PRESETS.len() - 1]
        .iter()
        .enumerate()
        .min_by(|(_, (a, _)), (_, (b, _))| {
            let distance =
                |value: &str| (value.parse::<f32>().unwrap_or(1.0).ln() - factor.ln()).abs();
            distance(a).total_cmp(&distance(b))
        })
        .map_or(DEFAULT_PRESET, |(index, _)| index)
}

/// A speed as the pages print it: "2x", "0.2x", "3.7x", "Max".
pub(crate) fn label(speed: Speed) -> String {
    if speed == Speed::UNCAPPED {
        return "Max".into();
    }
    let text = format!("{:.3}", speed.factor());
    format!("{}x", text.trim_end_matches('0').trim_end_matches('.'))
}

/// A speed as the receipts read it: the factor, or "max".
pub(crate) fn field(speed: Speed) -> String {
    if speed == Speed::UNCAPPED {
        MAX_VALUE.into()
    } else {
        speed.factor().to_string()
    }
}

/// The line a remote board shows: its speed, the viewer's own dial.
pub(crate) fn board_line(speed: Speed) -> String {
    format!("Board speed {}, from your Speed setting", label(speed))
}

/// Set preset `index`; the status line the page shows.
pub(crate) fn choose(canvas: &mut Canvas, index: usize) -> String {
    let (value, label) = PRESETS[index.min(PRESETS.len() - 1)];
    canvas.set_physics_speed(if value == MAX_VALUE {
        Speed::UNCAPPED
    } else {
        Speed::from_factor(value.parse().unwrap_or(1.0))
    });
    format!("Speed set to {label}")
}

/// The speed reached while the layout moves, and whether the budget held it.
pub(crate) fn reached(canvas: &Canvas) -> Option<String> {
    let pace = canvas.physics_pace();
    pace.effective_speed.map(|reached| {
        if pace.budget_bound {
            format!("Reached {reached:.1}x: the frame budget is full")
        } else {
            format!("Reached {reached:.1}x")
        }
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
