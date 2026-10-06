// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The step budget both web pages give the canvas and the remote board: a
//! share of the display's frame period (physics catalog plan, ruled
//! 2026-10-04, "The display's frame"). A browser does not say its display's
//! rate, but a page's frames start on the display's refreshes, so the
//! intervals between them are whole numbers of periods ("Infer the period").
//! Read from the intervals, not their length, it does not grow when physics
//! slows the page. Real intervals stray: some frames near a page's start
//! land off any multiple, steady ones scatter by two clock steps, a display
//! can switch or vary its rate. So the period is the largest in
//! [1/360 s, 1/60 s] that three quarters of the last 40 intervals fit within
//! two clock steps and the refresh jitter, taken up to a multiple of itself
//! when that multiple keeps most of the fit, so a fraction of the period is
//! never read, and down to a fraction when that fits many more, so a
//! multiple is not read where the page's frames are mostly even; where no period fits, the 1/60 s cap ("Both machines +
//! planted", 2026-10-05). The page passes its clock, that clock's grain, and
//! the margin its gate keeps ("Gate keeps a forecast margin").

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
/// The intervals kept, for the diagnostics that log them.
const RECENT_INTERVALS: usize = 120;
/// The most recent intervals the period is read from: a quarter of a second
/// at 165 Hz, two thirds at 60 Hz, longer on a page that misses refreshes,
/// short enough that a page's start or a display's switch soon ages out.
const ESTIMATE_INTERVALS: usize = 40;
/// Intervals needed before a period is read at all: with fewer, a page's
/// first frames, which land off any multiple, can carry a candidate past the
/// quorum (every logged window of this machine reads true from 16 on).
const MIN_INTERVALS: usize = 16;
/// The share of intervals a period must fit.
const QUORUM: f64 = 0.75;
/// A multiple of the period found is taken instead when it keeps this much
/// of the fit (and fits half the intervals): every multiple of the true
/// period is a multiple of its half too, so a half that narrowly clears the
/// quorum where the period narrowly misses it is lifted back.
const KEEP: f64 = 0.85;
const KEEP_FLOOR: f64 = 0.5;
/// A half or a third of the period found is taken instead when it fits this
/// much more of the intervals: the period found was then a multiple of the
/// display's, as on a page whose frames mostly take an even number of
/// refreshes. A page whose frames all do gains nothing from the half, and
/// reads twice the period, the most its intervals show.
const DESCEND_GAIN: f64 = 0.15;
/// The period found is first polished to the best-fitting refined candidate
/// within this share of it: a candidate can refine a little short of the
/// period, and a half judged against that weaker fit can look better.
const POLISH: f64 = 0.01;
/// How far an interval may sit from a multiple and still fit: each of its
/// two timestamps within a clock step, plus the refresh's own jitter.
const JITTER_MS: f64 = 0.1;
/// The shortest distinct intervals the candidate periods are drawn from.
const CANDIDATE_INTERVALS: usize = 8;
/// How far past its bounds a fitted period may fall and be taken as the
/// bound: a 60 Hz panel's measured period can read a hair over 1/60 s.
const BOUND_SLACK: f64 = 0.02;

/// Where the display period came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Period {
    /// `fit` of the recent intervals are whole multiples of `ms`.
    Inferred { ms: f64, fit: f64 },
    /// No period fitted the quorum: the 1/60 s cap. `nearest` is the best
    /// share a candidate reached, if any was tried.
    Fallback { ms: f64, nearest: Option<f64> },
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
                nearest: None,
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
                let recent = self.intervals.len().saturating_sub(ESTIMATE_INTERVALS);
                let recent: Vec<f64> = self.intervals.iter().skip(recent).copied().collect();
                self.period = period_of(&recent, self.grain_ms);
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

/// The largest period in bounds that [`QUORUM`] of `intervals` fit, from
/// candidates at the shortest intervals over 1, 2, 3, ..., each refined over
/// the intervals near its multiples, then lifted to a multiple that keeps the
/// fit; else the 1/60 s cap.
fn period_of(intervals: &[f64], grain_ms: f64) -> Period {
    let fallback = |nearest| Period::Fallback {
        ms: MAX_PERIOD_MS,
        nearest,
    };
    if intervals.len() < MIN_INTERVALS {
        return fallback(None);
    }
    let tolerance = 2.0 * grain_ms + JITTER_MS;
    let (low, high) = (
        MIN_PERIOD_MS * (1.0 - BOUND_SLACK),
        MAX_PERIOD_MS * (1.0 + BOUND_SLACK),
    );
    let mut shortest: Vec<f64> = intervals
        .iter()
        .map(|&i| (i / grain_ms).round() * grain_ms)
        .collect();
    shortest.sort_by(f64::total_cmp);
    shortest.dedup();
    shortest.truncate(CANDIDATE_INTERVALS);
    let mut candidates: Vec<f64> = shortest
        .iter()
        .flat_map(|&i| {
            let first = (i / high).ceil().max(1.0) as u32;
            let last = (i / low).floor() as u32;
            (first..=last).map(move |k| i / f64::from(k))
        })
        .collect();
    candidates.sort_by(|a, b| b.total_cmp(a));
    let mut nearest: Option<f64> = None;
    for &guess in &candidates {
        let ms = refined(intervals, guess);
        if !(low..=high).contains(&ms) {
            continue;
        }
        let fit = fit(intervals, ms, tolerance);
        if fit < QUORUM {
            nearest = Some(nearest.map_or(fit, |n: f64| n.max(fit)));
            continue;
        }
        let (ms, fit) = polished(intervals, &candidates, ms, fit, tolerance);
        let (ms, fit) = lifted(intervals, ms, fit, tolerance, high);
        let (ms, fit) = descended(intervals, ms, fit, tolerance, low);
        return Period::Inferred {
            ms: ms.clamp(MIN_PERIOD_MS, MAX_PERIOD_MS),
            fit,
        };
    }
    fallback(nearest)
}

/// The share of `intervals` within `tolerance` of a whole multiple of `ms`.
fn fit(intervals: &[f64], ms: f64, tolerance: f64) -> f64 {
    let fitting = intervals
        .iter()
        .filter(|&&i| {
            let n = (i / ms).round();
            n >= 1.0 && (i - n * ms).abs() <= tolerance
        })
        .count();
    fitting as f64 / intervals.len() as f64
}

/// The period near `guess` by least squares through the intervals within a
/// sixth of a period of its multiples, twice.
fn refined(intervals: &[f64], guess: f64) -> f64 {
    let mut ms = guess;
    for _ in 0..2 {
        let (moment, square) = intervals.iter().fold((0.0, 0.0), |(moment, square), &i| {
            let n = (i / ms).round();
            if n >= 1.0 && (i - n * ms).abs() <= ms / 6.0 {
                (moment + i * n, square + n * n)
            } else {
                (moment, square)
            }
        });
        if square == 0.0 {
            break;
        }
        ms = moment / square;
    }
    ms
}

/// The best fit among the candidates within [`POLISH`] of `ms`, refined.
fn polished(
    intervals: &[f64],
    candidates: &[f64],
    ms: f64,
    fit: f64,
    tolerance: f64,
) -> (f64, f64) {
    candidates
        .iter()
        .filter(|&&guess| (guess / ms - 1.0).abs() <= POLISH)
        .map(|&guess| {
            let near = refined(intervals, guess);
            (near, self::fit(intervals, near, tolerance))
        })
        .fold(
            (ms, fit),
            |best, near| if near.1 > best.1 { near } else { best },
        )
}

/// `ms` taken up to twice or three times itself while the multiple keeps
/// [`KEEP`] of the fit, and at least [`KEEP_FLOOR`] of the intervals.
fn lifted(intervals: &[f64], mut ms: f64, mut fit: f64, tolerance: f64, high: f64) -> (f64, f64) {
    'lift: loop {
        for k in [2.0, 3.0] {
            if ms * k > high {
                continue;
            }
            let multiple = refined(intervals, ms * k);
            let kept = self::fit(intervals, multiple, tolerance);
            if kept >= KEEP * fit && kept >= KEEP_FLOOR {
                (ms, fit) = (multiple, kept);
                continue 'lift;
            }
        }
        return (ms, fit);
    }
}

/// `ms` taken down to a half or a third of itself while that fits
/// [`DESCEND_GAIN`] more of the intervals.
fn descended(intervals: &[f64], mut ms: f64, mut fit: f64, tolerance: f64, low: f64) -> (f64, f64) {
    'descend: loop {
        for k in [2.0, 3.0] {
            if ms / k < low {
                continue;
            }
            let fraction = refined(intervals, ms / k);
            let gained = self::fit(intervals, fraction, tolerance);
            if gained >= fit + DESCEND_GAIN {
                (ms, fit) = (fraction, gained);
                continue 'descend;
            }
        }
        return (ms, fit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAIN: Duration = Duration::from_micros(100);

    /// period-diag-a.log, the 300-node page, the last 40 intervals of its fourth window.
    const WINDOWS_300_LOADED: [f64; 40] = [
        516.6, 639.5, 419.9, 449.9, 589.1, 607.9, 413.3, 200.6, 656.5, 686.7, 474.2, 431.5, 382.9,
        382.9, 383.0, 440.0, 382.9, 547.0, 370.8, 352.6, 401.2, 364.7, 358.5, 376.9, 352.5, 412.2,
        462.1, 440.4, 370.8, 389.0, 346.4, 334.3, 334.3, 492.3, 389.1, 358.6, 346.4, 352.5, 335.5,
        322.0,
    ];
    /// period-diag-a.log, the 24-node page, the last 40 intervals of its fourth window.
    const WINDOWS_24_LOADED: [f64; 40] = [
        109.4, 303.9, 316.1, 115.5, 297.8, 303.9, 103.4, 188.3, 253.0, 91.2, 85.1, 73.0, 60.6,
        243.3, 79.0, 79.0, 72.9, 66.9, 206.6, 72.9, 60.8, 60.9, 60.9, 66.7, 206.7, 66.9, 60.7,
        67.1, 66.6, 60.8, 200.6, 60.7, 60.8, 60.8, 66.8, 66.9, 188.6, 54.6, 60.7, 67.0,
    ];
    /// period-diag-b.log, the 300-node page, the last 40 intervals of its fourth window.
    const WINDOWS_300: [f64; 40] = [
        284.8, 375.8, 272.7, 424.3, 345.4, 321.3, 236.4, 115.1, 303.0, 230.4, 296.9, 218.2, 224.3,
        230.2, 297.0, 206.1, 206.1, 206.0, 290.9, 206.1, 200.0, 206.0, 200.0, 285.0, 199.9, 200.0,
        193.9, 200.0, 194.0, 278.8, 187.8, 188.0, 199.9, 272.8, 194.0, 193.9, 272.7, 200.0, 194.1,
        272.6,
    ];
    /// period-diag-b.log, the 24-node page, the last 40 intervals of its fourth window.
    const WINDOWS_24: [f64; 40] = [
        54.7, 121.5, 42.6, 48.5, 121.6, 109.5, 42.5, 91.2, 36.5, 36.4, 42.5, 30.4, 36.6, 36.4,
        36.5, 103.3, 36.4, 36.6, 36.4, 36.4, 30.5, 36.4, 36.5, 36.5, 36.5, 30.4, 36.4, 36.4, 103.4,
        30.4, 36.4, 42.6, 30.4, 36.5, 36.4, 30.4, 36.5, 30.4, 36.4, 30.5,
    ];

    /// 2026-10-06, the 300-node page, its first 20 intervals: mostly 30 refreshes, an even
    /// number, so twice the period clears the quorum before the period does.
    const WINDOWS_300_EARLY: [f64; 20] = [
        899.6, 85.1, 48.6, 121.6, 230.9, 291.8, 212.8, 285.6, 212.8, 218.8, 303.9, 194.5, 200.6,
        97.3, 182.4, 182.3, 182.3, 188.4, 182.3, 182.4,
    ];

    /// The ThinkPad's 300-node page, calm (2026-10-06, its 60.003 Hz panel), the last 40 intervals.
    const THINKPAD_300: [f64; 40] = [
        500.0, 483.2, 483.4, 316.6, 433.3, 283.4, 316.6, 166.6, 283.4, 300.0, 300.0, 399.9, 283.4,
        283.3, 283.3, 283.4, 283.3, 383.3, 266.6, 266.7, 283.3, 266.6, 266.7, 266.7, 250.0, 383.2,
        266.7, 250.0, 266.6, 266.7, 300.0, 283.4, 283.2, 400.0, 266.7, 266.6, 366.6, 250.1, 249.9,
        250.1,
    ];
    /// The ThinkPad's 24-node page with eight busy processes, the last 40 intervals.
    const THINKPAD_24_LOADED: [f64; 40] = [
        166.7, 350.0, 100.0, 116.7, 316.7, 116.5, 116.8, 216.6, 100.0, 116.7, 100.0, 300.0, 83.2,
        83.4, 100.0, 283.3, 83.3, 83.4, 83.3, 250.0, 100.0, 66.6, 83.4, 83.3, 250.0, 66.6, 83.4,
        83.3, 66.7, 233.3, 83.3, 100.0, 100.0, 250.0, 83.3, 83.4, 66.6, 83.4, 233.2, 66.7,
    ];
    /// The ThinkPad's 300-node page under load, its first 26 intervals: a
    /// candidate there refines short of the period, and stepping down from that
    /// weaker fit read half the period until the fit was polished.
    const THINKPAD_300_LOADED_EARLY: [f64; 26] = [
        101.948, 200.1, 116.6, 283.4, 550.0, 716.5, 400.0, 616.7, 466.6, 450.0, 583.3, 433.3,
        600.0, 216.7, 566.6, 599.9, 416.7, 400.0, 399.9, 400.0, 416.7, 399.9, 333.4, 433.3, 300.0,
        299.9,
    ];

    fn clock() -> Duration {
        Duration::ZERO
    }

    fn fresh() -> FrameBudget {
        FrameBudget::new(0.5, Duration::from_micros(200), clock, GRAIN)
    }

    /// A small deterministic generator for the planted traces.
    struct Lcg(u64);

    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }

        fn pick(&mut self, from: &[u32]) -> u32 {
            from[(self.unit() * from.len() as f64) as usize % from.len()]
        }
    }

    /// Intervals between frames `vsyncs` refreshes of a `hz` display apart,
    /// each true timestamp moved by up to `jitter_ms` and read in `grain_ms`
    /// steps.
    fn planted(hz: f64, vsyncs: &[u32], grain_ms: f64, jitter_ms: f64, rng: &mut Lcg) -> Vec<f64> {
        let period = 1000.0 / hz;
        let read = |t: f64| (t / grain_ms).floor() * grain_ms;
        let mut true_at = 1000.0;
        let mut last = read(true_at);
        vsyncs
            .iter()
            .map(|&n| {
                true_at += f64::from(n) * period;
                let at = read(true_at + (rng.unit() * 2.0 - 1.0) * jitter_ms);
                let interval = at - last;
                last = at;
                interval
            })
            .collect()
    }

    fn vsyncs(rng: &mut Lcg, from: &[u32], count: usize) -> Vec<u32> {
        (0..count).map(|_| rng.pick(from)).collect()
    }

    fn read(intervals: &[f64], grain_ms: f64) -> Period {
        let recent = &intervals[intervals.len().saturating_sub(ESTIMATE_INTERVALS)..];
        period_of(recent, grain_ms)
    }

    fn assert_reads(name: &str, period: Period, ms: f64) {
        match period {
            Period::Inferred { ms: read, .. } => {
                assert!(
                    (read / ms - 1.0).abs() < 0.01,
                    "{name}: read {read} ms, not {ms}"
                );
            },
            other => panic!("{name}: {other:?}, not {ms} ms"),
        }
    }

    /// "Both machines + planted" (ruled 2026-10-05): every planted trace
    /// reads within 1% of the period it holds, or falls back where it holds
    /// none, and none reads a fraction. The positive control is 60 Hz.
    #[test]
    fn planted_traces_read_their_period_or_fall_back() {
        let hz = |hz: f64| 1000.0 / hz;
        let rng = &mut Lcg(7);
        let traces: Vec<(&str, Vec<f64>, f64, Option<f64>)> = vec![
            (
                "60 Hz, the positive control",
                {
                    let v = vsyncs(rng, &[1, 1, 2, 3], 60);
                    planted(60.0, &v, 0.1, 0.0, rng)
                },
                0.1,
                Some(hz(60.0)),
            ),
            (
                "165 Hz, 4 to 17 refreshes",
                {
                    let v = vsyncs(rng, &[4, 5, 6, 7, 9, 14, 17], 60);
                    planted(165.0, &v, 0.1, 0.0, rng)
                },
                0.1,
                Some(hz(165.0)),
            ),
            (
                "144 Hz on a 5 us clock",
                {
                    let v = vsyncs(rng, &[3, 4, 5], 60);
                    planted(144.0, &v, 0.005, 0.0, rng)
                },
                0.005,
                Some(hz(144.0)),
            ),
            (
                "a 30 fps cap at 60 Hz",
                planted(60.0, &[2; 60], 0.1, 0.0, rng),
                0.1,
                Some(hz(60.0)),
            ),
            (
                "a 30 fps cap at 144 Hz",
                {
                    let v = vsyncs(rng, &[4, 5], 60);
                    planted(144.0, &v, 0.1, 0.0, rng)
                },
                0.1,
                Some(hz(144.0)),
            ),
            (
                "variable refresh",
                (0..60)
                    .map(|_| (70.0 + rng.unit() * 330.0).round() / 10.0)
                    .collect(),
                0.1,
                None,
            ),
            (
                "a switch from 60 to 144 Hz",
                {
                    let mut v = planted(60.0, &[1; 40], 0.1, 0.0, rng);
                    let fast = vsyncs(rng, &[2, 3], 40);
                    v.extend(planted(144.0, &fast, 0.1, 0.0, rng));
                    v
                },
                0.1,
                Some(hz(144.0)),
            ),
            (
                "a switch from 144 to 60 Hz",
                {
                    let fast = vsyncs(rng, &[2, 3], 40);
                    let mut v = planted(144.0, &fast, 0.1, 0.0, rng);
                    v.extend(planted(60.0, &[1; 40], 0.1, 0.0, rng));
                    v
                },
                0.1,
                Some(hz(60.0)),
            ),
            (
                "a switch from 120 to 60 Hz",
                {
                    let fast = vsyncs(rng, &[1, 2, 3], 40);
                    let mut v = planted(120.0, &fast, 0.1, 0.0, rng);
                    let slow = vsyncs(rng, &[1, 2], 40);
                    v.extend(planted(60.0, &slow, 0.1, 0.0, rng));
                    v
                },
                0.1,
                Some(hz(60.0)),
            ),
            (
                "165 Hz with 15% outliers",
                {
                    let v = vsyncs(rng, &[5, 6, 7], 60);
                    planted(165.0, &v, 0.1, 0.0, rng)
                        .into_iter()
                        .map(|i| {
                            if rng.unit() < 0.15 {
                                (200.0 + rng.unit() * 1000.0).round() / 10.0
                            } else {
                                i
                            }
                        })
                        .collect()
                },
                0.1,
                Some(hz(165.0)),
            ),
            (
                "165 Hz with 0.1 ms of refresh jitter",
                {
                    let v = vsyncs(rng, &[5, 6, 7], 60);
                    planted(165.0, &v, 0.1, 0.1, rng)
                },
                0.1,
                Some(hz(165.0)),
            ),
            (
                "165 Hz, mostly six refreshes",
                {
                    let v = vsyncs(rng, &[6, 6, 6, 5, 7], 60);
                    planted(165.0, &v, 0.1, 0.0, rng)
                },
                0.1,
                Some(hz(165.0)),
            ),
        ];
        for (name, intervals, grain, period) in traces {
            match period {
                Some(ms) => assert_reads(name, read(&intervals, grain), ms),
                None => assert!(
                    matches!(read(&intervals, grain), Period::Fallback { ms, .. } if ms == MAX_PERIOD_MS),
                    "{name}: {:?}",
                    read(&intervals, grain)
                ),
            }
        }
    }

    /// Both machines' logged windows (2026-10-04 to 06): this machine's 165 Hz
    /// panel, whose period read 6.06 to 6.08 ms, each within 1% of 6.07 ms,
    /// the loaded ones too and the early one where the page's frames mostly
    /// took 30 refreshes; the ThinkPad's 60.003 Hz panel, calm, loaded and
    /// early, each within 1% of 16.666 ms.
    #[test]
    fn logged_windows_read_this_panels_period() {
        // The early window's trap is real: twice the period clears the quorum.
        assert!(fit(&WINDOWS_300_EARLY, 2.0 * 6.078, 0.3) >= QUORUM);
        assert_reads("300 nodes, early", period_of(&WINDOWS_300_EARLY, 0.1), 6.07);
        // The ThinkPad's 60.003 Hz panel; its early loaded window's trap is
        // real too: the half fits as many intervals as the period.
        let early = &THINKPAD_300_LOADED_EARLY;
        assert!(fit(early, 1000.0 / 60.003 / 2.0, 0.3) >= fit(early, 1000.0 / 60.003, 0.3));
        for (name, window) in [
            ("ThinkPad, 300 nodes", &THINKPAD_300[..]),
            ("ThinkPad, 24 nodes, loaded", &THINKPAD_24_LOADED[..]),
            (
                "ThinkPad, 300 nodes, loaded, early",
                &THINKPAD_300_LOADED_EARLY[..],
            ),
        ] {
            assert_reads(name, period_of(window, 0.1), 1000.0 / 60.003);
        }
        for (name, window) in [
            ("300 nodes", WINDOWS_300),
            ("24 nodes", WINDOWS_24),
            ("300 nodes, loaded", WINDOWS_300_LOADED),
            ("24 nodes, loaded", WINDOWS_24_LOADED),
        ] {
            assert_reads(name, period_of(&window, 0.1), 6.07);
        }
    }

    /// A page that only ever takes an even number of refreshes reads twice
    /// the period, the most its intervals show: an overestimate, never a
    /// fraction, and the cap still bounds it.
    #[test]
    fn even_multiples_overestimate_the_period() {
        let rng = &mut Lcg(3);
        let v = vsyncs(rng, &[2, 4, 6], 60);
        assert_reads(
            "even multiples",
            read(&planted(165.0, &v, 0.1, 0.0, rng), 0.1),
            2000.0 / 165.0,
        );
        let doubled = planted(60.0, &[2, 4, 2], 0.1, 0.0, rng);
        assert!(matches!(
            read(&doubled, 0.1),
            Period::Fallback { .. } | Period::Inferred { .. }
        ));
        let mut budget = fresh();
        budget.frame(1000.0);
        for &interval in doubled.iter().cycle().take(40) {
            let at = budget.last_ms.unwrap() + interval;
            budget.frame(at);
        }
        assert_eq!(
            budget.budget().per_frame.as_micros(),
            8_333,
            "two 60 Hz refreshes are past the cap"
        );
    }

    /// The budget: 1/60 s's half before enough intervals; the inferred
    /// period's half once they fit; a hidden page's gap is not an interval;
    /// an old display's intervals age out of the 40 read.
    #[test]
    fn the_budget_follows_the_period_read() {
        let mut budget = fresh();
        let us = |budget: &FrameBudget| budget.budget().per_frame.as_micros();
        assert_eq!(us(&budget), 8_333, "60 Hz before any interval");
        let rng = &mut Lcg(11);
        budget.frame(1000.0);
        let v = vsyncs(rng, &[4, 5, 6], 60);
        for interval in planted(165.0, &v, 0.1, 0.0, rng) {
            let at = budget.last_ms.unwrap() + interval;
            budget.frame(at);
        }
        assert!(us(&budget).abs_diff(3_030) <= 2, "{}", us(&budget));
        let at = budget.last_ms.unwrap() + 1500.0;
        budget.frame(at);
        assert!(
            us(&budget).abs_diff(3_030) <= 2,
            "the gap is not an interval"
        );
        for interval in planted(60.0, &[1; 40], 0.1, 0.0, rng) {
            let at = budget.last_ms.unwrap() + interval;
            budget.frame(at);
        }
        assert!(
            us(&budget).abs_diff(8_333) <= 2,
            "165 Hz's intervals aged out: {}",
            us(&budget)
        );
        assert_eq!(budget.budget().margin, Duration::from_micros(200));
        assert_eq!(
            budget.intervals().count(),
            100,
            "60 + 40 intervals, the gap not one"
        );
    }
}
