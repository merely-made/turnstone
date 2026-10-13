// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit, finite two-independent-window shared-device recovery gate.
//!
//! Build only (root owns the compiler cache):
//! `cargo build --locked --offline -p tabard-desktop --example native_shared_recovery -j 2`
//! Run only after the native/GPU handoff and source/binary hashes are sealed:
//! `native_shared_recovery --output EMPTY_DIR --run-id SOURCE_BOUND_RUN --allow-device-destroy`
//! External process deadline: 90 seconds. Internal lifecycle deadline: 60 seconds;
//! presentation waits: 10 seconds; every owned pixel readback: 5 seconds.
//! This is two WinitHosts/WorkshopStates/DOMs, not the shared-forest run_windows API.
//! Output is complete un-matted RGBA plus JSON; no source font/scene blobs.

use std::{
    cell::RefCell,
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use cambium_genet_winit_host::{
    AppCtx, CloseDisposition, CloseRequest, HostHooks, WindowCommand, WinitHost,
};
use cambium_rootstock::{PresentedFrame, ProducerImageIdentity, start_frame_readback};
use genet_render_host::{
    PendingRgbaReadback, RecoveryStatus, RenderCore, RgbaFrame, SharedRenderCore,
};
use netrender::{ColorLoad, NO_CLIP, Scene};
use serde_json::{Value, json};
use tabard_desktop::Logic;
use tabard_workshop::{
    READER_LEAF_KEY, ReaderSpecimen, STYLESHEET_LEAF_KEY, StylesheetSpecimen, WorkshopState,
    WorkshopView,
};
use taproot::Selector;
use winit::{
    application::ApplicationHandler,
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;
const PIXEL_TIMEOUT: Duration = Duration::from_secs(5);
const PRESENT_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_TIMEOUT: Duration = Duration::from_secs(60);
const KEYS: [u64; 2] = [READER_LEAF_KEY, STYLESHEET_LEAF_KEY];
type NativeHost = WinitHost<WorkshopState, Logic, WorkshopView>;
type Context<'a> = AppCtx<'a, WorkshopState, Logic, WorkshopView>;

fn require(ok: bool, message: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}

struct Authority {
    values: Value,
    runner: usize,
    reader: Rc<RefCell<ReaderSpecimen>>,
    stylesheet: Rc<RefCell<StylesheetSpecimen>>,
    images: [ProducerImageIdentity; 2],
    name_point: (f32, f32),
}

fn state_values(state: &WorkshopState) -> Value {
    json!({
        "theme":state.draft_theme(), "mode":state.mode_key(),
        "dirty":state.is_dirty(), "status":state.status(),
        "graph_selected":state.selected_graph_node(),
        "name_text":state.text_field("name").map(|f|f.text()),
        "name_selection":format!("{:?}",state.text_field("name").map(|f|f.caret_selection())),
        "stylesheet_text":state.text_field("mode-sheet").map(|f|f.text()),
    })
}

fn seal(ctx: &Context<'_>) -> Result<Authority, String> {
    let state = ctx.runner.state();
    let dom = ctx.runner.dom();
    let node = taproot::matching(
        &*dom.borrow(),
        &Selector::role("textbox").with_attr("data-field", "name"),
    )
    .into_iter()
    .next()
    .ok_or("production name textbox absent")?;
    let (x, y, w, h) = ctx
        .visible_rect(node)
        .ok_or("production name textbox is not visible")?;
    let [Some(reader_image), Some(stylesheet_image)] =
        KEYS.map(|key| ctx.producers.image_identity(key))
    else {
        return Err("full Workshop must stage both actual preview images".into());
    };
    Ok(Authority {
        values: json!({"state":state_values(state),"focus":format!("{:?}",ctx.runner.focus()),"zoom":ctx.ui_zoom}),
        runner: ctx.runner as *const _ as usize,
        reader: state.reader_preview(),
        stylesheet: state.stylesheet_preview(),
        images: [reader_image, stylesheet_image],
        name_point: (x + w * 0.5, y + h * 0.5),
    })
}

fn image_json(image: &ProducerImageIdentity) -> Value {
    json!({"renderer_key":image.renderer_key,"generation":image.generation,
           "stamp":image.stamp,"size":image.size,"owner":format!("{:?}",image.owner)})
}

struct CopyJob {
    pending: Option<PendingRgbaReadback>,
    started: Instant,
    frame: Option<RgbaFrame>,
}
impl CopyJob {
    fn new(pending: PendingRgbaReadback) -> Self {
        Self {
            pending: Some(pending),
            started: Instant::now(),
            frame: None,
        }
    }
    fn poll(&mut self) -> Result<bool, String> {
        if self.frame.is_some() {
            return Ok(true);
        }
        require(
            self.started.elapsed() < PIXEL_TIMEOUT,
            "owned pixel readback exceeded five seconds",
        )?;
        if let Some(result) = self
            .pending
            .as_mut()
            .ok_or("missing pending readback")?
            .poll()
        {
            require(
                self.started.elapsed() < PIXEL_TIMEOUT,
                "pixel completion arrived after five seconds",
            )?;
            let frame = result.map_err(|e| format!("actual pixel readback failed: {e}"))?;
            valid_frame(&frame)?;
            self.pending = None;
            self.frame = Some(frame);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn valid_frame(frame: &RgbaFrame) -> Result<(), String> {
    let bytes = (frame.width as usize)
        .checked_mul(frame.height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("RGBA extent overflow")?;
    require(
        bytes > 0 && bytes <= MAX_FRAME_BYTES && bytes == frame.rgba.len(),
        "RGBA extent/32MiB bound failed",
    )?;
    require(!frame.is_blank(), "actual production frame is blank")?;
    // Stop collecting after 13 unique colors; do not allocate a set per pixel.
    let mut colors = BTreeSet::new();
    for pixel in frame.rgba.chunks_exact(4) {
        colors.insert([pixel[0], pixel[1], pixel[2], pixel[3]]);
        if colors.len() > 12 {
            return Ok(());
        }
    }
    Err("actual production foreground lacks distinct colors".into())
}

struct Captured {
    presentation: PresentedFrame,
    authority: Authority,
    copies: Vec<CopyJob>, // complete presented source, Reader, Stylesheet
}
struct CaptureSlot {
    seal: Option<Result<Authority, String>>,
    presented: Option<Result<(PresentedFrame, CopyJob), String>>,
}
impl CaptureSlot {
    fn new() -> Self {
        Self {
            seal: None,
            presented: None,
        }
    }
}

fn arm(host: &mut NativeHost, slot: Rc<RefCell<CaptureSlot>>) {
    let sealed = slot.clone();
    host.s.pending_presentation_observer = Some(Box::new(move |ctx, _| {
        sealed.borrow_mut().seal = Some(seal(ctx));
    }));
    host.s.pending_stamped_capture = Some(Box::new(move |result| {
        let result = result
            .map_err(|e| format!("complete presented capture refused: {e}"))
            .and_then(|capture| {
                let p = capture.presentation;
                let bytes = (p.width as usize)
                    .checked_mul(p.height as usize)
                    .and_then(|n| n.checked_mul(4));
                require(
                    bytes.is_some_and(|n| n > 0 && n <= MAX_FRAME_BYTES),
                    "presented source exceeds32MiB",
                )?;
                let copy = start_frame_readback(capture.surface, capture.view, p.width, p.height)?;
                Ok((p, CopyJob::new(copy)))
            });
        slot.borrow_mut().presented = Some(result);
    }));
    host.native_window()
        .expect("opened native window")
        .request_redraw();
}

fn source_copy(
    core: &RenderCore,
    image: ProducerImageIdentity,
    source_key: u64,
) -> Result<CopyJob, String> {
    let [w, h] = image.size;
    let bytes = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4));
    require(
        bytes.is_some_and(|n| n > 0 && n <= MAX_FRAME_BYTES),
        "complete producer exceeds32MiB",
    )?;
    require(
        image.generation == core.generation(),
        "producer belongs to stale generation",
    )?;
    let mut scene = Scene::new(w, h);
    // renderer_key is ALREADY the full physical key; no external_image_key conversion.
    scene.push_image_full(
        0.0,
        0.0,
        w as f32,
        h as f32,
        [0.0, 0.0, 1.0, 1.0],
        [1.0; 4],
        image.renderer_key,
        0,
        NO_CLIP,
    );
    let (texture, _) = core
        .try_rasterize_scaled_for(
            source_key,
            &scene,
            w,
            h,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            1.0,
        )
        .map_err(|e| format!("complete producer raster refused: {e:?}"))?;
    let pending = core.start_rgba8_readback(&texture, w, h)?;
    // No texture/view survives this function; queued readback owns the copy.
    Ok(CopyJob::new(pending))
}

fn collect(
    host: &NativeHost,
    slot: &Rc<RefCell<CaptureSlot>>,
    index: usize,
) -> Result<Option<Captured>, String> {
    let mut slot = slot.borrow_mut();
    if slot.presented.is_none() {
        return Ok(None);
    }
    let (presentation, copy) = slot.presented.take().unwrap()?;
    let authority = slot
        .seal
        .take()
        .ok_or("presentation lacks matching state seal")??;
    let core = host
        .s
        .shared
        .render_core
        .as_ref()
        .ok_or("presented host lacks core")?;
    let mut copies = vec![copy];
    for (kind, image) in authority.images.into_iter().enumerate() {
        // Independent helper surfaces; never reuse a producer's private raster key.
        copies.push(source_copy(
            core,
            image,
            0x6e61_7469_7665_0000 + (index * 2 + kind) as u64,
        )?);
    }
    Ok(Some(Captured {
        presentation,
        authority,
        copies,
    }))
}

fn frame<'a>(capture: &'a Captured, which: usize) -> &'a RgbaFrame {
    capture.copies[which]
        .frame
        .as_ref()
        .expect("completed capture")
}
fn compare(before: &RgbaFrame, after: &RgbaFrame) -> Result<Value, String> {
    require(
        before.width == after.width
            && before.height == after.height
            && before.rgba.len() == after.rgba.len(),
        "recovery pixel extent changed",
    )?;
    let mut max = 0u8;
    let mut alpha = 0u8;
    let mut sum = 0u64;
    for (i, (a, b)) in before.rgba.iter().zip(&after.rgba).enumerate() {
        let d = a.abs_diff(*b);
        max = max.max(d);
        if i % 4 == 3 {
            alpha = alpha.max(d);
        }
        sum += u64::from(d);
    }
    let mean = sum as f64 / before.rgba.len().max(1) as f64;
    // Unchanged stricter native recovery limits, applied to all pixels including edges.
    require(
        max <= 3 && alpha <= 2 && mean <= 1.0,
        format!("recovery delta refused: RGBA{max},alpha{alpha},mean{mean}"),
    )?;
    Ok(
        json!({"max_rgba":max,"max_alpha":alpha,"mean":mean,"exact":before.rgba==after.rgba,
              "limits":{"rgba":3,"alpha":2,"mean":1.0},"matte":false,"crop":false}),
    )
}

fn workshop(index: usize) -> Result<WorkshopState, String> {
    let mut state = WorkshopState::in_memory();
    let mut theme = state.draft_theme().clone();
    theme.seeds.primary.a = 73;
    theme.name = format!(
        "Independent shared native {}",
        if index == 0 { "A" } else { "B" }
    );
    let mode = state
        .available_modes()
        .into_iter()
        .find(|m| m.as_key() == if index == 0 { "light" } else { "dark" })
        .ok_or("required mode absent")?;
    state.edit_definition(&theme, Some(mode))?;
    *state
        .text_field_mut("seed-hex")
        .ok_or("seed field absent")? =
        cambium::TextInput::new(if index == 0 { "#2255AA" } else { "#33AA66" });
    require(state.apply_hex(), "authored seed refused")?;
    let css = if index == 0 {
        "body { background:#f2e7d9; color:#253951; } .toolbar { background:#d7e6f5; } h1 { color:#224477; }"
    } else {
        "body { background:#182e26; color:#edf7e8; } .toolbar { background:#335749; } h1 { color:#bfe6c8; }"
    };
    *state
        .text_field_mut("mode-sheet")
        .ok_or("stylesheet field absent")? = cambium::TextInput::new(css);
    state.apply_stylesheet();
    require(
        state.authored_sheet() == css,
        "actual authored stylesheet not held",
    )?;
    Ok(state)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Opening,
    Baseline,
    AwaitNotice,
    Recovery,
    AfterLoss,
    Input,
    Resize,
    Done,
}
struct Gate {
    hosts: Vec<Box<NativeHost>>,
    owner: Option<Arc<SharedRenderCore>>,
    old_core: Weak<RenderCore>,
    lost_generation: u64,
    factory_attempts: Arc<AtomicUsize>,
    slots: Vec<Rc<RefCell<CaptureSlot>>>,
    collecting: Vec<Option<Captured>>,
    baseline: Vec<Captured>,
    phase: Phase,
    started: Instant,
    phase_started: Instant,
    output: PathBuf,
    proof: Value,
    failure: Option<String>,
    hidden_sequence: u64,
    replacement_generation: u64,
    wake_event_loop: Arc<dyn Fn() + Send + Sync>,
}

fn write_new(path: PathBuf, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())
}

impl Gate {
    fn new(output: PathBuf, run_id: String, wake_event_loop: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            hosts: Vec::new(),
            owner: None,
            old_core: Weak::new(),
            lost_generation: 0,
            factory_attempts: Arc::new(AtomicUsize::new(0)),
            slots: Vec::new(),
            collecting: Vec::new(),
            baseline: Vec::new(),
            phase: Phase::Opening,
            started: Instant::now(),
            phase_started: Instant::now(),
            output,
            proof: json!({"schema":1,"run_id":run_id,"status":"RUNNING","fixture":"two_independent_WinitHosts_full_Workshop",
                "shared_forest_run_windows":false,"local_keys":KEYS,"captures":[],"comparisons":[],
                "limits":{"run_seconds":60,"presentation_seconds":10,"readback_seconds":5,"max_frame_bytes":MAX_FRAME_BYTES,
                          "recovery_rgba":3,"recovery_alpha":2,"recovery_mean":1.0},
                "source_manifest":{"example_fnv1a":fnv(include_str!("native_shared_recovery.rs").as_bytes()),
                    "desktop_manifest_fnv1a":fnv(include_str!("../Cargo.toml").as_bytes()),
                    "workspace_manifest_fnv1a":fnv(include_str!("../../../../Cargo.toml").as_bytes()),
                    "workspace_lock_fnv1a":fnv(include_str!("../../../../Cargo.lock").as_bytes()),
                    "hash_kind":"FNV-1a; runtime owner separately seals source/lock/executable SHA256"}}),
            failure: None,
            hidden_sequence: 0,
            replacement_generation: 0,
            wake_event_loop,
        }
    }

    fn open(
        &mut self,
        event_loop: &ActiveEventLoop,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        require(
            self.hosts.is_empty(),
            "unexpected second native initialization",
        )?;
        let initial = Arc::new(RenderCore::boot(
            (tabard_desktop::host_options().netrender)()
        )?);
        self.old_core = Arc::downgrade(&initial);
        self.lost_generation = initial.generation();
        let old = self.old_core.clone();
        let attempts = self.factory_attempts.clone();
        let owner = SharedRenderCore::with_factory(
            initial.clone(),
            Arc::new(move || {
                let n = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if n != 1 {
                    return Err(
                        "finite fixture refuses a second replacement factory attempt".into(),
                    );
                }
                if old.upgrade().is_some() {
                    return Err("replacement started while old host core remained alive".into());
                }
                RenderCore::boot((tabard_desktop::host_options().netrender)()).map(Arc::new)
            }),
        );
        let info = initial.renderer().wgpu_device.core.adapter.get_info();
        self.proof["adapter"] = json!({"name":info.name,"backend":format!("{:?}",info.backend),"driver":info.driver,"driver_info":info.driver_info});
        self.proof["initial_generation"] = json!(self.lost_generation);
        for i in 0..2 {
            let state = workshop(i)?;
            let mut options = tabard_desktop::host_options();
            options.title = format!(
                "Shared native recovery — {}",
                if i == 0 { "A" } else { "B" }
            );
            options.initial_logical_size = (1180.0, 800.0);
            options.size_env = None;
            let mut hooks: HostHooks<WorkshopState, Logic, WorkshopView> = tabard_desktop::hooks();
            // Actual close policy and native Hide path; no direct hidden-field mutation.
            if i == 1 {
                hooks.close_request = Box::new(|_, _| CloseDisposition::Hide);
            }
            let mut host = Box::new(WinitHost::with_shared_render_core(
                options,
                move |_, commands, _| {
                    let mut init = tabard_desktop::initialize_with_commands(state, commands);
                    init.sheet.push_str(if i == 0 {
                        "\nbody { background:#e8eff7; }"
                    } else {
                        "\nbody { background:#182c21; }"
                    });
                    init
                },
                hooks,
                initial.clone(),
                wake.clone(),
            ));
            host.open(event_loop)?;
            self.hosts.push(host);
        }
        drop(initial);
        self.owner = Some(owner);
        require(
            !Rc::ptr_eq(
                &self.hosts[0].s.runner.as_ref().unwrap().dom(),
                &self.hosts[1].s.runner.as_ref().unwrap().dom(),
            ),
            "independent hosts alias DOM",
        )?;
        Ok(())
    }

    fn begin_capture(&mut self, phase: Phase) {
        self.phase = phase;
        self.phase_started = Instant::now();
        self.slots.clear();
        self.collecting.clear();
        for host in &mut self.hosts {
            let slot = Rc::new(RefCell::new(CaptureSlot::new()));
            arm(host, slot.clone());
            self.slots.push(slot);
            self.collecting.push(None);
        }
    }

    fn captured(&mut self) -> Result<bool, String> {
        require(
            self.phase_started.elapsed() < PRESENT_TIMEOUT,
            format!(
                "native {:?} presentation/capture exceeded ten seconds",
                self.phase
            ),
        )?;
        let mut all = true;
        for i in 0..2 {
            if self.collecting[i].is_none() {
                self.collecting[i] = collect(&self.hosts[i], &self.slots[i], i)?;
            }
            if let Some(capture) = self.collecting[i].as_mut() {
                for job in &mut capture.copies {
                    all &= job.poll()?;
                }
            } else {
                all = false;
            }
        }
        Ok(all)
    }

    fn save_captures(&mut self, label: &str) -> Result<Vec<Captured>, String> {
        let mut captures = Vec::new();
        for i in 0..2 {
            let capture = self.collecting[i]
                .take()
                .ok_or("missing completed window capture")?;
            let mut pixels = Vec::new();
            for (kind, name) in ["complete", "reader", "stylesheet"].into_iter().enumerate() {
                let f = frame(&capture, kind);
                let filename = format!("{label}-{}-{name}.rgba", if i == 0 { "A" } else { "B" });
                write_new(self.output.join(&filename), &f.rgba)?;
                pixels.push(json!({"kind":name,"path":filename,"width":f.width,"height":f.height,
                    "bytes":f.rgba.len(),"fnv1a":format!("{:016x}",f.digest()),"whole_extent":true,"matte":false}));
            }
            self.proof["captures"].as_array_mut().unwrap().push(json!({"phase":label,"window":i,
                "presentation":{"host":capture.presentation.host,"sequence":capture.presentation.sequence,
                    "width":capture.presentation.width,"height":capture.presentation.height,"layout_scale":capture.presentation.layout_scale},
                "authority":capture.authority.values,"runner_address":capture.authority.runner,
                "reader_rc_address":Rc::as_ptr(&capture.authority.reader) as usize,
                "stylesheet_rc_address":Rc::as_ptr(&capture.authority.stylesheet) as usize,
                "images":capture.authority.images.iter().map(image_json).collect::<Vec<_>>(),"pixels":pixels}));
            captures.push(capture);
        }
        Ok(captures)
    }

    fn isolation(&self, captures: &[Captured]) -> Result<(), String> {
        require(
            captures[0].presentation.host != captures[1].presentation.host,
            "independent hosts alias presentation identity",
        )?;
        require(
            captures[0].authority.runner != captures[1].authority.runner,
            "independent hosts alias runner",
        )?;
        require(
            captures[0].authority.values["state"] != captures[1].authority.values["state"],
            "independent authored themes/CSS are identical",
        )?;
        require(
            !Rc::ptr_eq(&captures[0].authority.reader, &captures[1].authority.reader)
                && !Rc::ptr_eq(
                    &captures[0].authority.stylesheet,
                    &captures[1].authority.stylesheet,
                ),
            "independent apps alias sources",
        )?;
        let physical: BTreeSet<_> = captures
            .iter()
            .flat_map(|c| c.authority.images)
            .map(|image| image.renderer_key)
            .collect();
        require(
            physical.len() == 4,
            "same local keys alias physical bindings across independent hosts",
        )?;
        for kind in 0..3 {
            require(
                frame(&captures[0], kind).rgba != frame(&captures[1], kind).rgba,
                "distinct full Workshop sources produced identical pixels",
            )?;
        }
        Ok(())
    }

    fn inject_input(&mut self) -> Result<(), String> {
        let second_before = state_values(self.hosts[1].s.runner.as_ref().unwrap().state());
        for i in 0..2 {
            let point = self.baseline[i].authority.name_point;
            self.hosts[i].pointer_moved(point.0, point.1);
            self.hosts[i].click();
            self.hosts[i].after_dispatch();
            self.hosts[i].handle_window_event(WindowEvent::Ime(Ime::Commit(format!(
                " recovery-input-{}",
                if i == 0 { "A" } else { "B" }
            ))));
            let state = self.hosts[i].s.runner.as_ref().unwrap().state();
            require(
                state.draft_theme().name.contains(if i == 0 {
                    "recovery-input-A"
                } else {
                    "recovery-input-B"
                }),
                "actual focused-text/IME path failed after recovery",
            )?;
            if i == 0 {
                require(
                    state_values(self.hosts[1].s.runner.as_ref().unwrap().state()) == second_before,
                    "A input mutated independent B state",
                )?;
            }
        }
        Ok(())
    }

    fn drive(&mut self) -> Result<bool, String> {
        require(
            self.started.elapsed() < RUN_TIMEOUT,
            "native shared recovery exceeded sixty seconds",
        )?;
        let phase = self.phase;
        match phase {
            Phase::Opening => self.begin_capture(Phase::Baseline),
            Phase::Baseline if self.captured()? => {
                self.baseline = self.save_captures("baseline")?;
                self.isolation(&self.baseline)?;
                require(
                    Arc::ptr_eq(
                        self.hosts[0].s.shared.render_core.as_ref().unwrap(),
                        self.hosts[1].s.shared.render_core.as_ref().unwrap(),
                    ),
                    "hosts do not share actual initial RenderCore",
                )?;
                self.hosts[1].request_close(CloseRequest::Native);
                require(
                    self.hosts[1].s.hidden && !self.hosts[1].s.close_requested,
                    "real B CloseDisposition::Hide did not retain host",
                )?;
                self.hidden_sequence = self.hosts[1].s.presentation_sequence;
                let core = self.hosts[0].s.shared.render_core.as_ref().unwrap();
                core.device().destroy();
                let _ = core.device().poll(wgpu::PollType::Poll);
                self.phase = Phase::AwaitNotice;
                self.phase_started = Instant::now();
            },
            Phase::AwaitNotice => {
                require(
                    self.phase_started.elapsed() < PIXEL_TIMEOUT,
                    "actual destroyed-device callback exceeded five seconds",
                )?;
                let core = self.hosts[0]
                    .s
                    .shared
                    .render_core
                    .as_ref()
                    .ok_or("old core detached before controlled notice")?;
                let _ = core.device().poll(wgpu::PollType::Poll);
                // The detach barrier accepts acknowledgments only after loss
                // invalidation and readback cancellation have completed.
                // device_failure() can expose the earlier pending notice.
                if let Some(notice) = core.loss_notice() {
                    require(
                        notice.generation == self.lost_generation
                            && notice.reason == wgpu::DeviceLostReason::Destroyed,
                        "controlled destroy did not yield actual matching Destroyed notice",
                    )?;
                    self.proof["loss"] = json!({"generation":notice.generation,"reason":format!("{:?}",notice.reason),"message":notice.message});
                    // No event-loop interleave: prove A cannot replace an attached hidden B.
                    self.hosts[0].wake_turn();
                    require(
                        matches!(
                            self.owner.as_ref().unwrap().status(),
                            RecoveryStatus::Waiting {
                                remaining_hosts: 1,
                                ..
                            }
                        ) && self.factory_attempts.load(Ordering::SeqCst) == 0,
                        "A replaced core before hidden B acknowledged loss",
                    )?;
                    self.hosts[1].wake_turn();
                    self.proof["hidden_host_acknowledged_loss"] = json!(true);
                    self.phase = Phase::Recovery;
                    self.phase_started = Instant::now();
                }
            },
            Phase::Recovery => {
                require(
                    self.phase_started.elapsed() < PRESENT_TIMEOUT,
                    "shared replacement exceeded ten seconds",
                )?;
                self.hosts[0].wake_turn();
                self.hosts[1].wake_turn();
                match self.owner.as_ref().unwrap().status() {
                    RecoveryStatus::Ready { generation } if generation != self.lost_generation => {
                        require(
                            self.old_core.upgrade().is_none(),
                            "old RenderCore survived replacement",
                        )?;
                        require(
                            self.factory_attempts.load(Ordering::SeqCst) == 1,
                            "replacement factory did not run exactly once",
                        )?;
                        require(
                            self.hosts[1].s.hidden
                                && self.hosts[1].s.presentation_sequence == self.hidden_sequence,
                            "hidden B presented during recovery",
                        )?;
                        self.replacement_generation = generation;
                        self.hosts[1].commands().show();
                        self.hosts[1].wake_turn();
                        require(
                            !self.hosts[1].s.hidden,
                            "actual WindowCommand::Show did not reveal B",
                        )?;
                        self.begin_capture(Phase::AfterLoss);
                    },
                    RecoveryStatus::Failed { .. } => {
                        return Err(format!(
                            "shared recovery failed: {:?}",
                            self.owner.as_ref().unwrap().status()
                        ));
                    },
                    _ => {},
                }
            },
            Phase::AfterLoss if self.captured()? => {
                let after = self.save_captures("after-loss")?;
                self.isolation(&after)?;
                for i in 0..2 {
                    let b = &self.baseline[i];
                    let a = &after[i];
                    require(
                        a.authority.values == b.authority.values
                            && a.authority.runner == b.authority.runner
                            && Rc::ptr_eq(&a.authority.reader, &b.authority.reader)
                            && Rc::ptr_eq(&a.authority.stylesheet, &b.authority.stylesheet),
                        "authored state/runner/source Rc changed during native recovery",
                    )?;
                    require(
                        a.presentation.host == b.presentation.host
                            && a.presentation.sequence > b.presentation.sequence,
                        "native host lacks fresh presentation sequence",
                    )?;
                    for k in 0..2 {
                        let old = b.authority.images[k];
                        let image = a.authority.images[k];
                        require(
                            image.generation == self.replacement_generation
                                && image.stamp > old.stamp
                                && image.size == old.size
                                && image.renderer_key == old.renderer_key,
                            "producer lacks fresh same-size generation/stamp with preserved physical registration",
                        )?;
                    }
                    for kind in 0..3 {
                        let measurement = compare(frame(b, kind), frame(a, kind))?;
                        self.proof["comparisons"].as_array_mut().unwrap().push(json!({"window":i,"kind":(["complete","reader","stylesheet"][kind]),"measurement":measurement}));
                    }
                }
                require(
                    Arc::ptr_eq(
                        self.hosts[0].s.shared.render_core.as_ref().unwrap(),
                        self.hosts[1].s.shared.render_core.as_ref().unwrap(),
                    ),
                    "hosts split replacement cores",
                )?;
                for host in &self.hosts {
                    let core = host.s.shared.render_core.as_ref().unwrap();
                    require(
                        Arc::ptr_eq(&core.shared_recovery(), self.owner.as_ref().unwrap())
                            && matches!(self.owner.as_ref().unwrap().status(),RecoveryStatus::Ready {generation} if generation==core.generation()),
                        "replacement host does not share the original ready owner",
                    )?;
                }
                self.proof["replacement_generation"] = json!(self.replacement_generation);
                self.inject_input()?;
                self.begin_capture(Phase::Input);
            },
            Phase::Input if self.captured()? => {
                let input = self.save_captures("after-input")?;
                self.isolation(&input)?;
                for i in 0..2 {
                    require(
                        frame(&input[i], 0).rgba != frame(&self.baseline[i], 0).rgba,
                        "post-recovery input failed to change complete native pixels",
                    )?;
                    require(
                        input[i].presentation.host == self.baseline[i].presentation.host
                            && input[i].authority.runner == self.baseline[i].authority.runner,
                        "post-recovery input replaced the native host/runner",
                    )?;
                }
                for host in &mut self.hosts {
                    host.commands().push(WindowCommand::Resize(1040.0, 760.0));
                    host.wake_turn();
                }
                self.phase = Phase::Resize;
                self.phase_started = Instant::now();
                self.slots.clear();
                self.collecting.clear();
            },
            Phase::Resize => {
                if self.slots.is_empty() {
                    require(
                        self.phase_started.elapsed() < PRESENT_TIMEOUT,
                        "real native resize event exceeded ten seconds",
                    )?;
                    let resized = self.hosts.iter().all(|h| {
                        h.native_window().is_some_and(|w| {
                            let logical = w.inner_size().to_logical::<f64>(w.scale_factor());
                            (logical.width - 1040.0).abs() <= 1.0
                                && (logical.height - 760.0).abs() <= 1.0
                        })
                    });
                    if resized {
                        self.begin_capture(Phase::Resize);
                    }
                } else if self.captured()? {
                    let resized = self.save_captures("after-resize")?;
                    self.isolation(&resized)?;
                    for i in 0..2 {
                        require(
                            resized[i].presentation.width != self.baseline[i].presentation.width
                                || resized[i].presentation.height
                                    != self.baseline[i].presentation.height,
                            "post-recovery native resize did not change presented extent",
                        )?;
                        let state = self.hosts[i].s.runner.as_ref().unwrap().state();
                        require(
                            Rc::ptr_eq(&state.reader_preview(), &self.baseline[i].authority.reader)
                                && Rc::ptr_eq(
                                    &state.stylesheet_preview(),
                                    &self.baseline[i].authority.stylesheet,
                                ),
                            "input/resize rebound application source Rc",
                        )?;
                        require(
                            resized[i].presentation.host == self.baseline[i].presentation.host
                                && resized[i].authority.runner == self.baseline[i].authority.runner,
                            "post-recovery resize replaced the native host/runner",
                        )?;
                    }
                    self.proof["status"] = json!("PASS");
                    self.proof["factory_attempts"] =
                        json!(self.factory_attempts.load(Ordering::SeqCst));
                    self.proof["elapsed_seconds"] = json!(self.started.elapsed().as_secs_f64());
                    self.proof["old_core_released"] = json!(true);
                    self.proof["independent_hosts_verified"] = json!(true);
                    self.proof["shared_initial_and_replacement_core_verified"] = json!(true);
                    self.proof["same_local_keys_distinct_physical_images_verified"] = json!(true);
                    self.proof["native_hide_show_input_resize_verified"] = json!(true);
                    self.proof["qualification_scope"] = json!(
                        "controlled Device::destroy through actual native hosts; not spontaneous driver reset or browser recovery"
                    );
                    self.phase = Phase::Done;
                    return Ok(true);
                }
            },
            Phase::Done => return Ok(true),
            _ => {},
        }
        Ok(false)
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        self.proof["status"] = json!("REFUSED");
        self.proof["failure"] = json!(&error);
        self.proof["failed_phase"] = json!(format!("{:?}", self.phase));
        self.failure = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler<()> for Gate {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.open(event_loop, self.wake_event_loop.clone()) {
            self.fail(event_loop, error);
        }
    }
    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        // Both independent subscriptions receive the turn, including the hidden host.
        // AwaitNotice drives the detach-order assertion itself before allowing interleave.
        if self.phase != Phase::AwaitNotice {
            for host in &mut self.hosts {
                host.wake_turn();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.phase == Phase::AwaitNotice {
            return;
        }
        if let Some(host) = self
            .hosts
            .iter_mut()
            .find(|h| h.native_window().is_some_and(|w| w.id() == id))
        {
            host.handle_window_event(event);
            if host.s.close_requested {
                self.fail(
                    event_loop,
                    "native host closed before gate completion".into(),
                );
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.failure.is_some() {
            event_loop.exit();
            return;
        }
        match self.drive() {
            Ok(true) => event_loop.exit(),
            Ok(false) => {
                if self.phase != Phase::AwaitNotice {
                    for host in &mut self.hosts {
                        host.idle_turn();
                    }
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(10),
                ));
            },
            Err(error) => self.fail(event_loop, error),
        }
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.fail(
            event_loop,
            "platform suspended during finite native gate".into(),
        );
    }
}

fn fnv(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = None;
    let mut run_id = None;
    let mut armed = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().map(PathBuf::from),
            "--run-id" => run_id = args.next(),
            "--allow-device-destroy" => armed = true,
            _ => return Err(format!("unknown/missing argument: {arg}").into()),
        }
    }
    let output = output.ok_or("--output EMPTY_DIR is required")?;
    let run_id = run_id
        .filter(|s| !s.trim().is_empty())
        .ok_or("--run-id SOURCE_BOUND_RUN is required")?;
    require(armed, "dedicated --allow-device-destroy opt-in required")?;
    fs::create_dir_all(&output)?;
    require(
        fs::read_dir(&output)?.next().is_none(),
        "output directory must be empty; never overwrite receipts",
    )?;
    let event_loop = EventLoop::<()>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let mut gate = Gate::new(
        output.clone(),
        run_id,
        Arc::new(move || {
            let _ = proxy.send_event(());
        }),
    );
    let result = event_loop.run_app(&mut gate);
    if let Err(error) = &result {
        gate.proof["status"] = json!("REFUSED");
        gate.proof["failure"] = json!(error.to_string());
    }
    write_new(
        output.join("proof.json"),
        &serde_json::to_vec_pretty(&gate.proof)?,
    )?;
    result?;
    if let Some(error) = gate.failure {
        return Err(error.into());
    }
    require(
        gate.phase == Phase::Done,
        "event loop exited without complete independent native proof",
    )?;
    Ok(())
}

#[cfg(test)]
mod controls {
    use super::*;
    fn rgba(bytes: Vec<u8>) -> RgbaFrame {
        RgbaFrame {
            width: 2,
            height: 2,
            rgba: bytes,
        }
    }
    #[test]
    fn recovery_thresholds_preserve_alpha_and_mean_refusals() {
        let before = rgba([20, 30, 40, 100].repeat(4));
        let mut after = rgba(before.rgba.clone());
        after.rgba[0] += 3;
        after.rgba[3] += 2;
        assert!(compare(&before, &after).is_ok());
        after.rgba[3] += 1;
        assert!(compare(&before, &after).is_err());
        let mean = rgba([22, 32, 42, 100].repeat(4));
        assert!(compare(&before, &mean).is_err());
        let mut rgb = rgba(before.rgba.clone());
        rgb.rgba[0] += 4;
        assert!(compare(&before, &rgb).is_err());
    }
    #[test]
    fn complete_extent_comparison_rejects_hidden_tail_difference() {
        let before = rgba([20, 30, 40, 100].repeat(4));
        let mut after = rgba(before.rgba.clone());
        after.rgba[15] += 3;
        assert!(compare(&before, &after).is_err());
    }
}
