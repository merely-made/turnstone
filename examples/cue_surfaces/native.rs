// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use genet_render_host::RenderCore;
use pelt_core::{PeltContent, PeltTileRequest, cue::PeltSharedContent};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use turnstone::servo_host::cue::{ServoCueHost, ServoCueImporter};
use wgpu_cue::webcue::*;
use wgpu_cue::{GpuHost, Imported, NativeImporter, Presenter, Source};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
type R<T> = Result<T>;
struct Clock(Instant);

struct Card {
    content: PeltSharedContent,
    presenter: Presenter,
    events: Vec<serde_json::Value>,
    last_sequence: u64,
    requests: u64,
    completions: Vec<RequestKey>,
    hash: Option<blake3::Hash>,
    stamp: Option<FrameStamp>,
    size: Option<[u32; 2]>,
    background: Option<[u8; 4]>,
    typed_observed: bool,
    navigation_observed: bool,
    navigation_observations: usize,
    retiring: bool,
    pending_readback: Option<(FrameStamp, u8, genet_render_host::PendingRgbaReadback)>,
}

struct Blitter {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

struct App {
    output: PathBuf,
    require_window: bool,
    window: Option<Arc<Window>>,
    core: Option<RenderCore>,
    host: Option<GpuHost>,
    servo: Option<ServoCueHost>,
    importer: Option<ServoCueImporter>,
    cards: Vec<Card>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    blitter: Option<Blitter>,
    viewport: Viewport,
    start: Instant,
    phase: u8,
    phase_start: Instant,
    baseline: Vec<blake3::Hash>,
    navigation_baseline: Vec<usize>,
    presented_frames: u64,
    records: Vec<serde_json::Value>,
    held: Option<NativeFrame>,
    custody_readback: Option<genet_render_host::PendingRgbaReadback>,
    custody_checked: bool,
    old_scopes: Vec<Scope>,
    error: Option<String>,
    passed: bool,
}

#[path = "native/html.rs"]
mod html;
#[path = "native/impl_pelt_core_pelt_clock_for.rs"]
mod impl_pelt_core_pelt_clock_for;
use html::html;
#[path = "native/report.rs"]
mod report;
use report::report;
#[path = "native/app_new.rs"]
mod app_new;
#[path = "native/app_tick.rs"]
mod app_tick;
#[path = "native/blitter_new.rs"]
mod blitter_new;
#[path = "native/card_new.rs"]
mod card_new;
#[path = "native/impl_application_handler_for_app.rs"]
mod impl_application_handler_for_app;
#[path = "native/run.rs"]
mod run;
pub use run::run;
