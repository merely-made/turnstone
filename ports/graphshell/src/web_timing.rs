// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Frame timing, the same measure on both pages, so `GpuPresenter` and the
//! Cambium tree compare frame for frame (the one-tree plan's phase 3).
//!
//! A window opens on one scenario step (`timing start <label>`) and closes on
//! another (`timing stop`). Between them every drawn frame records the time
//! since the last frame began, the CPU time its own work took, and, when the
//! device has timestamp queries, the GPU time between two marker passes around
//! its submissions. WebGPU writes timestamps only at pass boundaries and
//! netrender's passes take none, so the GPU span includes any idle between the
//! frame's submissions. The GPU readback lands a few frames after the window
//! closes, and the window is reported once it has.
//!
//! A hidden page draws no frames, so a window in which the page was ever
//! hidden is marked, and a scenario fails on it.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use wasm_bindgen::{JsCast, closure::Closure};

thread_local! {
    // Frames stop while hidden. Keep transitions independently of the frame
    // loop so a hidden/shown interval cannot disappear from a timing window.
    static HIDDEN_EPOCH: Cell<u64> = const { Cell::new(0) };
    static VISIBILITY_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

fn hidden_epoch() -> u64 {
    HIDDEN_EPOCH.with(Cell::get)
}

fn observe_visibility() -> Result<(), String> {
    if VISIBILITY_INSTALLED.with(Cell::get) {
        return Ok(());
    }
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("no document")?;
    let observed_document = document.clone();
    let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if observed_document.hidden() {
            HIDDEN_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
        }
    });
    document
        .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref())
        .map_err(|_| "could not observe timing visibility")?;
    listener.forget();
    VISIBILITY_INSTALLED.with(|installed| installed.set(true));
    Ok(())
}

/// The most frames a window records GPU time for: two timestamps each, within
/// WebGPU's 4096-query limit on a query set.
const GPU_FRAMES: u32 = 2000;

/// The device a frame's markers are written on.
pub(crate) type Gpu<'a> = Option<(&'a wgpu::Device, &'a wgpu::Queue)>;

/// Milliseconds on the page's clock.
pub(crate) fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or_default()
}

/// Whether the page is hidden, and so drawing no frames.
pub(crate) fn page_hidden() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.hidden())
}

#[derive(Default)]
pub(crate) struct FrameTiming {
    window: Option<Window>,
    closing: Option<Closing>,
    reports: Vec<Report>,
    marks: Option<GpuMarks>,
}

struct Window {
    label: String,
    intervals_ms: Vec<f64>,
    cpu_ms: Vec<f64>,
    producer_render_us: Vec<f64>,
    producer_stage_us: Vec<f64>,
    stages_ms: BTreeMap<&'static str, Vec<f64>>,
    counts: BTreeMap<&'static str, [usize; 2]>,
    last_start: Option<f64>,
    frame_start: Option<f64>,
    hidden: bool,
    hidden_epoch: u64,
    /// Frames whose two markers were both written.
    gpu_frames: u32,
    gpu_open: bool,
}

/// A closed window waiting for its GPU times.
struct Closing {
    window: Window,
    readback: Option<(wgpu::Buffer, Arc<AtomicBool>)>,
}

/// One window's summary, as the receipt reports it.
struct Report {
    label: String,
    frames: usize,
    interval_ms: Summary,
    cpu_ms: Summary,
    gpu_ms: Option<Summary>,
    producer_render_us: Option<Summary>,
    producer_stage_us: Option<Summary>,
    stages_ms: BTreeMap<&'static str, Summary>,
    counts: BTreeMap<&'static str, [usize; 2]>,
    hidden: bool,
}

#[derive(Clone, Copy)]
struct Summary {
    p50: f64,
    p95: f64,
    max: f64,
}

impl Summary {
    fn of(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
        Some(Self {
            p50: at(0.5),
            p95: at(0.95),
            max: sorted[sorted.len() - 1],
        })
    }

    fn json(&self) -> Value {
        json!({ "p50": round(self.p50), "p95": round(self.p95), "max": round(self.max) })
    }

    fn line(&self, name: &str) -> String {
        format!(
            "{name} p50={:.3} p95={:.3} max={:.3}",
            self.p50, self.p95, self.max
        )
    }
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

struct GpuMarks {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    period_ns: f32,
}

impl GpuMarks {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timing"),
            ty: wgpu::QueryType::Timestamp,
            count: GPU_FRAMES * 2,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame timing resolve"),
            size: u64::from(GPU_FRAMES * 2) * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Some(Self {
            set,
            resolve,
            period_ns: queue.get_timestamp_period(),
        })
    }

    /// An empty pass whose only work is writing timestamp `index`.
    fn mark(&self, device: &wgpu::Device, queue: &wgpu::Queue, index: u32) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame timing mark"),
        });
        drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("frame timing mark"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &self.set,
                beginning_of_pass_write_index: Some(index),
                end_of_pass_write_index: None,
            }),
        }));
        queue.submit([encoder.finish()]);
    }

    /// Copy the first `frames` frames' timestamps where they can be mapped.
    fn read(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frames: u32,
    ) -> (wgpu::Buffer, Arc<AtomicBool>) {
        let bytes = u64::from(frames * 2) * 8;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame timing readback"),
            size: bytes.max(8),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame timing readback"),
        });
        if frames > 0 {
            encoder.resolve_query_set(&self.set, 0..frames * 2, &self.resolve, 0);
            encoder.copy_buffer_to_buffer(&self.resolve, 0, &readback, 0, bytes);
        }
        queue.submit([encoder.finish()]);
        // An atomic flag, not `Rc<Cell<bool>>`: with wgpu's
        // `fragile-send-sync-non-atomic-wasm` on (cubecl-wgpu enables it, and
        // features unify) a wasm `map_async` callback must be `Send`.
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |_| {
            flag.store(true, Ordering::Release)
        });
        (readback, done)
    }
}

impl FrameTiming {
    /// Open a window. Fails if one is open or still closing.
    pub(crate) fn start(&mut self, label: &str, gpu: Gpu<'_>) -> Result<(), String> {
        if self.window.is_some() || self.closing.is_some() {
            return Err(format!("timing start {label}: a window is still open"));
        }
        observe_visibility()?;
        if self.marks.is_none()
            && let Some((device, queue)) = gpu
        {
            self.marks = GpuMarks::new(device, queue);
        }
        self.window = Some(Window {
            label: label.to_string(),
            intervals_ms: Vec::new(),
            cpu_ms: Vec::new(),
            producer_render_us: Vec::new(),
            producer_stage_us: Vec::new(),
            stages_ms: BTreeMap::new(),
            counts: BTreeMap::new(),
            last_start: None,
            frame_start: None,
            hidden: page_hidden(),
            hidden_epoch: hidden_epoch(),
            gpu_frames: 0,
            gpu_open: false,
        });
        Ok(())
    }

    /// A frame's work begins: call before the frame builds or submits anything.
    pub(crate) fn frame_begin(&mut self, gpu: Gpu<'_>) {
        let Some(window) = self.window.as_mut() else {
            return;
        };
        let now = now_ms();
        if let Some(last) = window.last_start {
            window.intervals_ms.push(now - last);
        }
        window.last_start = Some(now);
        window.frame_start = Some(now);
        window.hidden |= page_hidden() || window.hidden_epoch != hidden_epoch();
        if let (Some(marks), Some((device, queue))) = (&self.marks, gpu)
            && window.gpu_frames < GPU_FRAMES
        {
            marks.mark(device, queue, window.gpu_frames * 2);
            window.gpu_open = true;
        }
    }

    /// A frame's work ends: call after its last submission.
    pub(crate) fn frame_end(&mut self, gpu: Gpu<'_>) {
        let Some(window) = self.window.as_mut() else {
            return;
        };
        if let Some(start) = window.frame_start.take() {
            window.cpu_ms.push(now_ms() - start);
        }
        if window.gpu_open
            && let (Some(marks), Some((device, queue))) = (&self.marks, gpu)
        {
            marks.mark(device, queue, window.gpu_frames * 2 + 1);
            window.gpu_frames += 1;
        }
        window.gpu_open = false;
    }

    /// The producer's share of the frame just ended, when the page has one.
    pub(crate) fn producer(&mut self, render_us: u64, stage_us: u64) {
        if let Some(window) = self.window.as_mut() {
            window.producer_render_us.push(render_us as f64);
            window.producer_stage_us.push(stage_us as f64);
        }
    }

    /// Whether detailed timings should be sampled for this frame.
    pub(crate) fn active(&self) -> bool {
        self.window.is_some()
    }

    /// Nonoverlapping CPU stages from the scene producer, in milliseconds.
    pub(crate) fn stages(&mut self, stages: impl IntoIterator<Item = (&'static str, f64)>) {
        if let Some(window) = self.window.as_mut() {
            for (name, ms) in stages {
                window.stages_ms.entry(name).or_default().push(ms);
            }
        }
    }

    /// Workload bounds keep timing comparisons honest when nodes leave view.
    pub(crate) fn counts(&mut self, counts: impl IntoIterator<Item = (&'static str, usize)>) {
        if let Some(window) = self.window.as_mut() {
            for (name, count) in counts {
                let range = window.counts.entry(name).or_insert([count, count]);
                range[0] = range[0].min(count);
                range[1] = range[1].max(count);
            }
        }
    }

    /// Close the window; its report lands once the GPU times are read.
    pub(crate) fn stop(&mut self, gpu: Gpu<'_>) -> Result<(), String> {
        let mut window = self
            .window
            .take()
            .ok_or_else(|| "timing stop: no window is open".to_string())?;
        window.hidden |= page_hidden() || window.hidden_epoch != hidden_epoch();
        let readback = match (&self.marks, gpu) {
            (Some(marks), Some((device, queue))) => {
                Some(marks.read(device, queue, window.gpu_frames))
            },
            _ => None,
        };
        self.closing = Some(Closing { window, readback });
        self.poll();
        Ok(())
    }

    /// Finish a closing window whose GPU times have landed. True while one is
    /// still waiting, so a scenario's `wait` holds for it.
    pub(crate) fn poll(&mut self) -> bool {
        let Some(closing) = self.closing.take() else {
            return false;
        };
        let gpu_ms = match &closing.readback {
            Some((_, done)) if !done.load(Ordering::Acquire) => {
                self.closing = Some(closing);
                return true;
            },
            Some((buffer, _)) => {
                let period = self.marks.as_ref().map_or(1.0, |marks| marks.period_ns);
                // A readback that fails to map reports no GPU times rather
                // than holding the scenario forever.
                let times = match buffer.slice(..).get_mapped_range() {
                    Ok(data) => data
                        .chunks_exact(16)
                        .take(closing.window.gpu_frames as usize)
                        .map(|pair| {
                            let begin =
                                u64::from_le_bytes(pair[..8].try_into().unwrap_or_default());
                            let end = u64::from_le_bytes(pair[8..].try_into().unwrap_or_default());
                            end.saturating_sub(begin) as f64 * f64::from(period) / 1.0e6
                        })
                        .collect::<Vec<f64>>(),
                    Err(_) => Vec::new(),
                };
                buffer.unmap();
                Summary::of(&times)
            },
            None => None,
        };
        let window = closing.window;
        self.reports.push(Report {
            label: window.label,
            frames: window.cpu_ms.len(),
            interval_ms: Summary::of(&window.intervals_ms).unwrap_or(Summary {
                p50: 0.0,
                p95: 0.0,
                max: 0.0,
            }),
            cpu_ms: Summary::of(&window.cpu_ms).unwrap_or(Summary {
                p50: 0.0,
                p95: 0.0,
                max: 0.0,
            }),
            gpu_ms,
            producer_render_us: Summary::of(&window.producer_render_us),
            producer_stage_us: Summary::of(&window.producer_stage_us),
            stages_ms: window
                .stages_ms
                .into_iter()
                .filter_map(|(name, values)| Summary::of(&values).map(|summary| (name, summary)))
                .collect(),
            counts: window.counts,
            hidden: window.hidden,
        });
        false
    }

    /// Whether any reported window saw the page hidden.
    pub(crate) fn any_hidden(&self) -> bool {
        self.reports.iter().any(|report| report.hidden)
    }

    /// Whether the device writes timestamps, once a window has asked.
    pub(crate) fn gpu_timed(&self) -> bool {
        self.marks.is_some()
    }

    /// One receipt line per reported window.
    pub(crate) fn receipt_lines(&self) -> Vec<String> {
        self.reports
            .iter()
            .map(|report| {
                let mut parts = vec![
                    format!("timing {} frames={}", report.label, report.frames),
                    report.interval_ms.line("interval_ms"),
                    report.cpu_ms.line("cpu_ms"),
                ];
                match report.gpu_ms {
                    Some(gpu) => parts.push(gpu.line("gpu_ms")),
                    None => parts.push("gpu_ms none".to_string()),
                }
                if let Some(render) = report.producer_render_us {
                    parts.push(render.line("producer_render_us"));
                }
                if let Some(stage) = report.producer_stage_us {
                    parts.push(stage.line("producer_stage_us"));
                }
                for (name, summary) in &report.stages_ms {
                    parts.push(summary.line(name));
                }
                if report.hidden {
                    parts.push("HIDDEN".to_string());
                }
                parts.join(" ")
            })
            .collect()
    }

    /// Every reported window, for the receipt's JSON.
    pub(crate) fn json(&self) -> Value {
        Value::Array(
            self.reports
                .iter()
                .map(|report| {
                    json!({
                        "label": report.label,
                        "frames": report.frames,
                        "interval_ms": report.interval_ms.json(),
                        "cpu_ms": report.cpu_ms.json(),
                        "gpu_ms": report.gpu_ms.map(|gpu| gpu.json()),
                        "producer_render_us": report.producer_render_us.map(|s| s.json()),
                        "producer_stage_us": report.producer_stage_us.map(|s| s.json()),
                        "stages_ms": report.stages_ms.iter().map(|(name, s)| (*name, s.json())).collect::<BTreeMap<_, _>>(),
                        "counts": report.counts.iter().map(|(name, [min,max])| (*name, json!({"min":min,"max":max}))).collect::<BTreeMap<_, _>>(),
                        "hidden": report.hidden,
                    })
                })
                .collect(),
        )
    }
}
