// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The simulation-speed dial on both web pages (physics catalog plan, ruled
//! 2026-10-04, "Speed select" and "Target + Max, budget as frame share"):
//! presets from 0.2x to 50x and Max in the physics section, 1x by default,
//! applied when chosen, with the speed reached shown while the layout moves.
//! The step budget is [`graphshell::frame_budget`]'s, half the display's
//! frame period as the page's frame intervals show it, at most 1/60 s; here
//! its gate keeps two of the browser clock's 100 us steps past the forecast
//! tick (ruled 2026-10-04, "Gate keeps a forecast margin"). Also the `physics_speed`, `physics_budget_share` and
//! `physics_budget_margin_us` page options and the browser clock the budget
//! is measured on.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub(crate) use graphshell::frame_budget::{FrameBudget, Period};
use mere::canvas::{Canvas, DEFAULT_BUDGET_SHARE, Speed};

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

/// Time the gate keeps past the forecast tick unless the page asks
/// otherwise, in clock steps: two. A tick and its forecast are both read in
/// steps, so a tick the forecast saw at one reading can read two steps
/// dearer (on the 300-node page every admitted tick ran at most 200 us past
/// its forecast, 2026-10-04, Chrome's step being 100 us), and the frame's own
/// reading takes the third, which the receipts' bound allows.
const DEFAULT_MARGIN_STEPS: f64 = 2.0;

/// Where the display period's intervals come from (ruled 2026-10-06, "Worker
/// rAF, main thread fallback"): the worker unless the page turns it off
/// (`physics_period_source=main`) or plants its failure
/// (`physics_plant_worker=fail`), the fallback's control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PeriodSource {
    Worker,
    Main,
    PlantedFailure,
}

/// The browser clock's step, measured: the smallest change of five readings
/// in a row (Chrome resolves 100 us on a page that is not cross-origin
/// isolated, 5 us on one that is; Firefox 1 ms).
pub(crate) fn clock_grain() -> Duration {
    let mut step = f64::INFINITY;
    for _ in 0..5 {
        let start = now_ms();
        let mut next = start;
        for _ in 0..10_000_000 {
            next = now_ms();
            if next != start {
                break;
            }
        }
        if next > start {
            step = step.min(next - start);
        }
    }
    let ms = if step.is_finite() {
        step.clamp(0.001, 100.0)
    } else {
        0.1
    };
    Duration::from_secs_f64(ms / 1000.0)
}
/// How often the reached-speed note may change, in host milliseconds, so a
/// live figure reads rather than flickers.
const NOTE_INTERVAL_MS: f64 = 500.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpeedOptions {
    pub(crate) speed: Speed,
    /// The share of each frame the step budget is, in (0, 1].
    pub(crate) share: f64,
    /// What the gate keeps past the forecast tick.
    pub(crate) margin: Duration,
    /// The browser clock's step, measured.
    pub(crate) grain: Duration,
    pub(crate) period_source: PeriodSource,
    /// A planted stall in the budget's clock, every so many readings: the
    /// receipts' positive control (`physics_plant_stall_ms`,
    /// `physics_plant_every`, 97 by default).
    pub(crate) plant: Option<(Duration, u64)>,
    /// A planted busy-wait in every frame at Max, outside physics, so Max
    /// runs fewer ticks a second than 1x: the control for the fast receipt's
    /// "Relative to the page's 1x" (`physics_plant_max_frame_ms`).
    pub(crate) plant_max_frame: Option<Duration>,
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
    let grain = clock_grain();
    let margin = match params.get("physics_budget_margin_us") {
        Some(value) => Duration::from_micros(
            value
                .parse::<u64>()
                .map_err(|_| "physics_budget_margin_us wants whole microseconds")?,
        ),
        None => grain.mul_f64(DEFAULT_MARGIN_STEPS),
    };
    let period_source = match (
        params.get("physics_period_source").as_deref(),
        params.get("physics_plant_worker").as_deref(),
    ) {
        (_, Some("fail")) => PeriodSource::PlantedFailure,
        (Some("main"), _) => PeriodSource::Main,
        (None | Some("worker"), None) => PeriodSource::Worker,
        _ => {
            return Err(
                "physics_period_source wants worker or main, physics_plant_worker fail".into(),
            );
        },
    };
    let plant = match params.get("physics_plant_stall_ms") {
        Some(value) => {
            let stall = value
                .parse::<u64>()
                .map_err(|_| "physics_plant_stall_ms wants whole milliseconds")?;
            let every = match params.get("physics_plant_every") {
                Some(every) => every
                    .parse::<u64>()
                    .ok()
                    .filter(|every| *every > 0)
                    .ok_or("physics_plant_every wants a count above 0")?,
                None => 97,
            };
            Some((Duration::from_millis(stall), every))
        },
        None => None,
    };
    let plant_max_frame = match params.get("physics_plant_max_frame_ms") {
        Some(value) => {
            Some(Duration::from_millis(value.parse::<u64>().map_err(
                |_| "physics_plant_max_frame_ms wants whole milliseconds",
            )?))
        },
        None => None,
    };
    Ok(SpeedOptions {
        speed,
        share,
        margin,
        grain,
        period_source,
        plant,
        plant_max_frame,
        explicit: [
            "physics_speed",
            "physics_budget_share",
            "physics_budget_margin_us",
        ]
        .iter()
        .any(|name| params.has(name)),
    })
}

pub(crate) fn clock() -> Duration {
    Duration::from_secs_f64(now_ms().max(0.0) / 1000.0)
}

/// Busy-wait `wait` if the canvas runs at Max: the planted frame cost.
pub(crate) fn plant_max_frame(canvas: &Canvas, wait: Option<Duration>) {
    if let Some(wait) = wait
        && canvas.physics_speed() == Speed::UNCAPPED
    {
        let until = now_ms() + wait.as_secs_f64() * 1000.0;
        while now_ms() < until {}
    }
}

/// The page's frame budget: its share and margin, on the browser clock, or
/// on the planted one when the page asks for a stall.
pub(crate) fn frame_budget(options: SpeedOptions) -> FrameBudget {
    let clock = match options.plant {
        Some((stall, every)) => {
            PLANT_STALL_US.store(stall.as_micros() as u64, Ordering::Relaxed);
            PLANT_EVERY.store(every, Ordering::Relaxed);
            planted_clock
        },
        None => clock,
    };
    let mut budget = FrameBudget::new(options.share, options.margin, clock, options.grain);
    if options.period_source == PeriodSource::Main {
        budget.disable_worker();
    }
    budget
}

static PLANT_STALL_US: AtomicU64 = AtomicU64::new(0);
static PLANT_EVERY: AtomicU64 = AtomicU64::new(0);
static PLANT_READINGS: AtomicU64 = AtomicU64::new(0);

/// The browser clock with a planted stall: every `PLANT_EVERY`th reading
/// busy-waits `PLANT_STALL_US` before it reads, so a tick the gate admitted
/// can run past the budget whatever its forecast.
fn planted_clock() -> Duration {
    let every = PLANT_EVERY.load(Ordering::Relaxed);
    if every > 0 && PLANT_READINGS.fetch_add(1, Ordering::Relaxed) % every == every - 1 {
        let until = now_ms() + PLANT_STALL_US.load(Ordering::Relaxed) as f64 / 1000.0;
        while now_ms() < until {}
    }
    clock()
}

/// Where the display period came from, for the receipts: "inferred" or
/// "fallback", and the share of recent intervals the period fits (the best a
/// candidate reached, when none cleared the quorum). Which intervals they
/// were is the budget's source.
pub(crate) fn period_fields(budget: &FrameBudget) -> (&'static str, String) {
    match budget.period() {
        Period::Inferred { fit, .. } => ("inferred", format!("{fit:.2}")),
        Period::Fallback { nearest, .. } => (
            "fallback",
            nearest.map_or_else(|| "none".into(), |share| format!("{share:.2}")),
        ),
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
