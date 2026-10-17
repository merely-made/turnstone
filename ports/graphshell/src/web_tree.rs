// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The one-tree page: Graphshell's graph canvas inside a Cambium tree.
//!
//! Phase 3 of the one-tree plan proves the canvas can live in a mounted
//! Cambium tree before any control moves there. Pictograph's canvas renders
//! through a [`TextureProducer`] into a custom-leaf slot, rasterized with the
//! web host's own renderer under a key of its own. A click on the slot picks
//! the node under it, the wheel zooms by default (configurable in `commands`),
//! two fingers pan/pinch (`gestures`), and the arrow keys pan and plus and
//! minus zoom. `tree.html` mounts it with [`mount_tree`]; `nodes` and `seed`
//! in the URL swap the fixture graph for a generated one, and a host history
//! (`data-dataset`) replaces both, stepped by checkpoint (`history`).
//!
//! The page drives itself through Mesquite's scenario lane with the web
//! host's [`WebCapture`], and times its frames with the same
//! [`FrameTiming`] as the main page, so the two are measured alike.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cambium::{
    AnyView, GenetCtx, GenetElement, Key, NamedKey, custom_leaf, el, on_key, on_pointer,
};
use cambium_genet_web_host::{MountLifetime, WebCapture, WebWindow, mount_with_lifetime};
use cambium_rootstock::{
    AppCtx, CloseDisposition, Frame, HostFont, HostHooks, HostOptions, HostPointer, HostWindow,
    Init, ProducedTexture, ProducerContext, SourceAlpha, SourceEncoding, TextureProducer,
};
use graphshell::app::GraphshellApp;
use graphshell::mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef};
use mere::canvas::Canvas;
use mere::canvas::controls::CanvasCommand;
use mere::kernel::graph::Graph;
use mesquite::{CaptureRecord, Lane, Product};
use muniment::MemoryBackend;
use netrender::{ColorLoad, NetrenderOptions};
use serde_json::json;
use taproot::ProbeSnapshot;
use taproot::Selector;
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlCanvasElement};

// The retained canvas's clear colour, independent of any presentation adapter.
const SHELL_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.027,
    g: 0.047,
    b: 0.059,
    a: 1.0,
};
use super::web_graphs;
use super::web_scenario;
use super::web_timing::{FrameTiming, now_ms};

/// The canvas's custom-leaf key, shared by the view and its producer.
const CANVAS_KEY: u64 = 1;
/// The tile cache the canvas's scene rasterizes under, apart from the
/// document's own.
const CANVAS_RASTER: u64 = 0x7472_6565;
/// How far an arrow key pans and plus or minus zooms, as the main page does.
const PAN_STEP: f32 = 42.0;
const ZOOM_STEP: f32 = 40.0;
/// The docked "Graph tools" region's width, logical px. The canvas takes the
/// rest of the row.
const TOOLS_WIDTH: u32 = 300;
/// Below this tree width the region collapses to a "Graph tools" toggle that
/// opens it over the canvas. Docked, the canvas keeps at least 600 px: a
/// centred node dragged 220 px needs 476, and the fitted fixture spans
/// about 320.
const TOOLS_DOCK_MIN_WIDTH: u32 = TOOLS_WIDTH + 600;

const SHEET: &str = "\
    :root { font-family: IBM Plex Sans, sans-serif; font-size: 14px; color: var(--tabard-color-text, #dce3e8); background: var(--tabard-color-bg, #070c0f); } \
    main { display: flex; flex-direction: column; } \
    h1 { font-size: 16px; margin: 8px 12px 0; } \
    .tree-status { margin: 4px 12px 8px; } \
    .tree-controls { display:flex; gap:6px; padding:6px 12px; flex-wrap:wrap; } \
    .tree-controls button { background:var(--tabard-color-surface-2, #263640); color:var(--tabard-color-text, #dce3e8); padding:5px 10px; border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-controls button[disabled] { opacity:.45; } \
    .tree-grouping { max-height:180px; overflow-y:auto; padding:4px 12px; border-bottom:1px solid var(--tabard-color-border, #2c3b44); } \
    .tree-grouping h2 { font-size:14px; margin:2px 0; } \
    .tree-grouping p { font-size:12px; margin:3px 0; overflow-wrap:anywhere; } \
    .tree-grouping ul { margin:3px 0; padding-left:18px; } \
    .tree-grouping button { background:var(--tabard-color-surface-2, #263640); color:var(--tabard-color-text, #dce3e8); padding:4px 8px; margin:2px 4px; border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-grouped .tree-relations { max-height:110px; overflow-y:auto; padding:0 12px; overflow-wrap:anywhere; } \
    .tree-body { display:flex; flex-direction:row; flex:1 1 auto; min-height:0; position:relative; }     .tools-overlay { position:absolute; top:0; right:0; bottom:0; z-index:10; }     .tools-storage { margin:2px 0 6px; color:var(--tabard-color-text-dim, #9fb0bb); font-size:12px; } \
    .tree-graph { display:flex; flex-direction:column; flex:1 1 auto; min-width:0; position:relative; } \
    .forme-drop-cue { position:absolute; pointer-events:none; z-index:6; box-sizing:border-box; border:2px dashed currentColor; } \
    .forme-drop-stack { border:5px double currentColor; } \
    .forme-drop-label { display:block; width:230px; padding:4px 6px; color:var(--tabard-color-text, #dce3e8); background:var(--tabard-color-surface, #17232b); border:1px solid currentColor; font-size:12px; } \
    .tree-canvas { display: block; flex: 1 1 auto; min-height: 0; } \
    .tree-tools { flex:0 0 auto; width:279px; padding:4px 10px; background:var(--tabard-color-surface, #0d161b); border-left:1px solid var(--tabard-color-border, #2c3b44); overflow-y:auto; min-height:0; } \
    .tree-tools .disclosure-trigger { display:block; width:100%; text-align:left; font-size:14px; background:transparent; border:none; padding:2px 0 4px; margin:0; } \
    .tools-cards { margin:2px 0 4px; padding:0 0 0 16px; font-size:12px; color:var(--tabard-color-text, #dce3e8); } \
    .tree-tools h2 { font-size:14px; margin:2px 0 4px; } \
    .tree-tools h3 { font-size:13px; margin:4px 0 2px; } \
    .tree-rules [data-cambium-text-value] { display:block; width:260px; min-height:26px; color:inherit; background:var(--tabard-color-surface-2, #263640); border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-rules pre, .tree-dynamics pre, .moot-opened-content { white-space:pre-wrap; overflow-wrap:anywhere; font-size:12px; } \
    .tree-coupling [data-cambium-text-value] { display:block; width:260px; min-height:26px; color:inherit; background:var(--tabard-color-surface-2, #263640); border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-query [data-cambium-text-value] { display:block; width:260px; min-height:26px; color:inherit; background:var(--tabard-color-surface-2, #263640); border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-inspection { overflow-wrap:anywhere; } \
    .tree-inspection p, .tree-inspection li { font-size:12px; } \
    .tree-tools .detail-section-title { font-weight:bold; margin:6px 0 4px; } \
    .tree-tools .detail-key { display:block; font-size:12px; color:var(--tabard-color-text-dim, #9fb0bb); } \
    .tree-tools .detail-value { display:block; margin:2px 0 8px; overflow-wrap:anywhere; } \
    .tools-section + .tools-section { margin-top:10px; border-top:1px solid var(--tabard-color-border, #2c3b44); padding-top:4px; } \
    .tools-switch button[aria-pressed=true] { background:var(--tabard-color-tertiary, #2d5a63); color:var(--tabard-color-on-tertiary, #dce3e8); border-color:var(--tabard-color-primary, #79a9be); } \
    .tools-active { margin:4px 0; color:var(--tabard-color-text, #f0dfb8); font-size:12px; } \
    .tree-tools label { display:block; margin:3px 0; } \
    .tree-tools .tools-caption { display:block; color:var(--tabard-color-text-dim, #9fb0bb); font-size:12px; } \
    .tree-tools button { background:var(--tabard-color-surface-2, #263640); color:var(--tabard-color-text, #dce3e8); padding:3px 10px; margin:2px 0 4px; border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tools-overlays { display:flex; flex-wrap:wrap; margin:2px 0; } \
    .tools-overlays label { width:136px; margin:1px 0; } \
    .tools-overlays label.disabled { opacity:.45; } \
    .tools-note { margin:2px 0 4px; color:var(--tabard-color-text-dim, #c9b27c); font-size:12px; } \
    .tools-status { margin:4px 0; color:var(--tabard-color-text-dim, #9fb0bb); font-size:12px; } \
    .select-box { background:var(--tabard-color-surface-2, #263640); border:1px solid var(--tabard-color-border-strong, #637581); padding:2px 8px; } \
    .select-list { background:var(--tabard-color-surface-2, #17232b); border:1px solid var(--tabard-color-border-strong, #637581); z-index:20; width:262px; } \
    .select-option { padding:1px 8px; } \
    .tree-product { position:absolute;top:8px;left:12px;max-width:360px;max-height:90%;overflow-y:auto;z-index:5; } \
    .tree-product-workbench { position:static;flex:0 0 auto;max-width:none;max-height:140px;padding:4px 12px; } \
    .tree-product p { margin:4px 0; } \
    .tree-projection { position:absolute;top:8px;left:12px;right:310px;max-height:90%;overflow:auto;z-index:6;background:var(--tabard-color-surface, #17232b);padding:12px; } \
    .tree-practice { background:var(--tabard-color-surface, #1a211b);color:var(--tabard-color-text, #e4eadc); } \
    .practice-control { box-sizing:border-box; padding:8px; font-size:14px; } \
    .practice-button,.practice-card { background:var(--tabard-color-surface-2, #29352a);border:1px solid var(--tabard-color-border-strong, #627255); } \
    .practice-selected { background:var(--tabard-color-surface-hover, #3a4c30);border:2px solid var(--tabard-color-border-strong, #bdd59e); } \
    .practice-label,.practice-detail { display:block; } \
    .practice-heading { font-size:24px; } \
    .practice-detail { font-size:12px; } \
    .tree-detail { background:var(--tabard-color-surface-2, #17232b);border:1px solid var(--tabard-color-border, #637581);padding:12px; } \
    .tree-detail label { display:block;margin:8px 0; } \
    .tree-detail .detail-key { display:block; } \
    .tree-detail .detail-value { display:block;margin:2px 0 8px;overflow-wrap:anywhere; } \
    .tree-product [data-cambium-text-value] { display:block;width:280px;min-height:1.2em;height:26px;color:var(--tabard-color-text, #dce3e8);background:var(--tabard-color-surface-2, #263640);border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-tools [data-cambium-text-value], .tree-projection [data-cambium-text-value], .tree-command-menu [data-cambium-text-value] { display:block;box-sizing:border-box;width:100%;min-height:1.2em;height:26px;color:var(--tabard-color-text, #dce3e8);background:var(--tabard-color-surface-2, #263640);border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-product button { background:var(--tabard-color-surface-2, #263640);color:var(--tabard-color-text, #dce3e8);padding:5px 10px;border:1px solid var(--tabard-color-border-strong, #637581); } \
    .tree-relations { margin:0 12px 6px; font-size:12px; } \
    .tree-relations h2 { font-size:13px; margin:0 0 2px; } \
    .tree-relations ul { margin:0; padding:0 0 0 16px; max-height:96px; overflow-y:auto; } \
    .tree-refusal { margin:4px 12px 8px; padding:8px; color:var(--tabard-color-text, #f3d2c6); background:var(--tabard-color-surface-2, #3a1f1a); border:1px solid var(--tabard-color-danger, #a4574a); } \
    .tree-relations ul, .history-changes { max-height:72px; } \
    .tree-history { margin:0 12px 6px; font-size:12px; } \
    .history-control { display:flex; flex-direction:row; align-items:center; gap:8px; } \
    .history-control button { background:var(--tabard-color-surface-2, #263640); color:var(--tabard-color-text, #dce3e8); padding:3px 10px; border:1px solid var(--tabard-color-border-strong, #637581); } \
    .history-control button[aria-disabled=true] { opacity:.45; } \
    .history-slider { position:relative; width:240px; height:18px; } \
    .history-track { position:absolute; left:0; right:0; top:8px; height:2px; background:var(--tabard-color-border-strong, #637581); } \
    .history-thumb { position:absolute; top:2px; width:10px; height:14px; margin-left:-5px; background:var(--tabard-color-primary, #79a9be); } \
    .history-value { margin:4px 0 0; overflow-wrap:anywhere; } \
    .history-summary { margin:2px 0; color:var(--tabard-color-text, #f0dfb8); } \
    .history-changes { margin:0; padding:0 0 0 16px; overflow-y:auto; }";

// Applets retain their host stylesheet; application appearance enters last.
fn tree_stylesheet(appearance: &str) -> String {
    #[cfg(feature = "product")]
    let workspace_sheet = format!(
        "{}\n{}\n{}",
        cambium::TITLE_BAR_CSS,
        cambium::INSTRUMENT_CSS,
        instrument::SHEET
    );
    #[cfg(not(feature = "product"))]
    let workspace_sheet = "";
    #[cfg(feature = "applets")]
    let applet_sheet = applet::SHEET;
    #[cfg(not(feature = "applets"))]
    let applet_sheet = "";
    #[cfg(feature = "remote")]
    let reservoir_sheet = mere_view::MERE_VIEW_CSS;
    #[cfg(not(feature = "remote"))]
    let reservoir_sheet = "";
    format!(
        "{SHEET}\n{}\n{}\n{workspace_sheet}\n{applet_sheet}\n{reservoir_sheet}\n{}\n{appearance}",
        format!("{}\n{}", commands::SHEET, presenter::SHEET),
        forme_sheet(),
        cambium::BACKDROP_SCENE_CSS
    )
}

/// What the page, its producer and its hooks share.
struct Shared {
    adapter_info: RefCell<Option<serde_json::Value>>,
    events: RefCell<Vec<String>>,
    #[cfg(feature = "product")]
    reading: RefCell<reading::Reading>,
    #[cfg(feature = "moot-resources")]
    moot_resources: RefCell<graphshell::moot_resources::Host>,
    wheel_mode: Cell<mere::canvas::gestures::WheelMode>,
    command_request: RefCell<Option<commands::Request>>,
    #[cfg(feature = "applets")]
    applet: RefCell<applet::Pane>,
    appearance: crate::web_appearance::Handle,
    appearance_revision: Cell<u64>,

    canvas: RefCell<Canvas>,
    /// Input or a new graph since the producer last drew.
    dirty: Cell<bool>,
    /// Whether the layout moved on the last frame, so frames keep coming.
    moving: Cell<bool>,
    /// The canvas's size in logical px, as last laid out.
    size: Cell<(u32, u32)>,
    physical_size: Cell<[u32; 2]>,
    /// The host's device and queue, from the producer's first frame; the
    /// timing marks are written on them.
    gpu: RefCell<Option<(wgpu::Device, wgpu::Queue)>>,
    timing: RefCell<FrameTiming>,
    physics_config: mere::canvas::ElapsedStepConfig,
    /// The page's simulation speed and budget, and the frames the receipts read.
    speed: crate::web_speed::SpeedOptions,
    pace: RefCell<speed::PaceWindow>,
    /// The speed reached, under the speed picker while the budget binds.
    reached: RefCell<crate::web_speed::ReachedNote>,
    /// The step budget, a share of the measured frame interval.
    frame_budget: RefCell<crate::web_speed::FrameBudget>,
    period_worker: crate::web_period_worker::PeriodWorker,
    gpu_options: controls::GpuOptions,
    /// The page's device for the canvas's and the board's repulsion, built
    /// once from the host's render core on the producer's first frame.
    physics_device: RefCell<Option<PhysicsDevice>>,
    visibility: Option<RefCell<visibility::Visibility>>,
    /// A released drag's drop point, canvas-local px, until the first frame
    /// that executes a physics step after the release.
    release_watch: Cell<Option<(f32, f32)>>,
    /// The focused node's distance from that drop point on that frame, and
    /// how many physics steps the frame executed.
    release_step: Cell<Option<(f32, u32)>>,
    /// The same drop point in world units, and the same distance in world
    /// units on that frame, whatever the zoom.
    release_watch_world: Cell<Option<(f32, f32)>>,
    release_step_world: Cell<Option<f32>>,
    /// Where the last scripted press landed, canvas-local px, for the receipt.
    press_point: Cell<Option<(f32, f32)>>,
    /// Every recorded release, for the receipt.
    release_log: RefCell<Vec<String>>,
    /// Lines `log-physics` recorded, for the receipt.
    physics_log: RefCell<Vec<String>>,
    /// The last `measure-faces` reading, for the snapshot.
    faces: Cell<Option<mere::canvas::face_alignment::FaceAlignment>>,
    /// The remote session (`remote::TreeRemote`); its channel's pumps reach
    /// it from outside the runner.
    remote: Rc<RefCell<remote::TreeRemote>>,
    /// Whether the canvas leaf shows the remote board rather than the graph.
    remote_shown: Cell<bool>,
    /// Keep reader actions aligned with the local editor during a file answer
    /// or an unacknowledged write. Updated at each dispatch and frame.
    selection_locked: Cell<bool>,
    /// A planted accessibility defect, the receipts' positive control
    /// (`?plant_a11y=`; dynamics grammar plan, G9). `None` in use.
    plant: graphshell::canvas_reader::Plant,
    /// The mount root, which carries the page's motion preference.
    root: Element,
    /// The page's gestures over the graph, and where the graph is.
    gestures: RefCell<gestures::Gestures>,
}

impl Shared {
    /// Whether the page asks for reduced motion: transitions land at once,
    /// the camera jumps rather than glides, and physics starts paused.
    fn reduced_motion(&self) -> bool {
        gestures::reduced_motion(&self.root)
    }

    /// Pan the camera by a keyboard step, gliding unless motion is reduced.
    fn pan(&self, dx: f32, dy: f32) {
        let mut canvas = self.canvas.borrow_mut();
        if self.reduced_motion() {
            mere::canvas::gestures::pan_at_once(&mut *canvas, dx, dy);
        } else {
            CanvasCommand::Pan { dx, dy }.apply(&mut *canvas, self.size.get());
        }
        self.dirty.set(true);
    }

    fn with_gpu<R>(&self, f: impl FnOnce(Option<(&wgpu::Device, &wgpu::Queue)>) -> R) -> R {
        let gpu = self.gpu.borrow();
        f(gpu.as_ref().map(|(device, queue)| (device, queue)))
    }
}

/// Pictograph's canvas as a texture producer.
struct CanvasProducer {
    shared: Rc<Shared>,
    generation: u64,
    scene: Option<cambium_rootstock::ProducedScene>,
    /// The last frame's texture, kept alive while the host samples its view.
    texture: Option<wgpu::Texture>,
}

/// Bind the page's WebGPU device for canvas compute once per device lifetime.
/// Runs from the frame hook, so a route that draws the graph's portable scene
/// in the page pass, and never calls the texture callback, keeps GPU physics.
fn attach_gpu(shared: &Shared, core: &genet_render_host::RenderCore) {
    if shared.gpu.borrow().is_some() {
        return;
    }
    #[cfg_attr(not(feature = "canvas-gpu"), allow(unused_variables))]
    let options = shared.gpu_options;
    // The host's own device, never one of ours: the render core's handles are
    // the ones every producer on this page draws with. Renderer access is one
    // operation; a refused frame gate or lost device retries next frame.
    #[cfg(feature = "canvas-gpu")]
    let physics = if options.enabled {
        let Ok(renderer) = cambium_rootstock::try_renderer(core) else {
            return;
        };
        Some(
            mere::canvas::physics_device_for(&renderer.wgpu_device.core)
                .with_threshold(options.threshold)
                .with_max_stale_steps(options.max_stale_steps),
        )
    } else {
        None
    };
    *shared.gpu.borrow_mut() = Some((core.device().clone(), core.queue().clone()));
    #[cfg(feature = "canvas-gpu")]
    if let Some(device) = physics {
        shared
            .canvas
            .borrow_mut()
            .set_physics_device(Some(device.clone()));
        shared
            .remote
            .borrow_mut()
            .board
            .set_physics_device(Some(device.clone()));
        *shared.physics_device.borrow_mut() = Some(device);
    }
}

impl TextureProducer for CanvasProducer {
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        attach_gpu(&self.shared, cx.core);
        let output = self.produce(cx.frame).ok()?;
        let [physical_width, physical_height] = cx.frame.physical_size;
        let raster_start = self.shared.timing.borrow().active().then(now_ms);
        let (texture, view) = cx.core.rasterize_scaled_for(
            CANVAS_RASTER,
            &output.scene,
            physical_width,
            physical_height,
            ColorLoad::Clear(SHELL_CLEAR),
            cx.frame.layout_scale,
        );
        if let Some(start) = raster_start {
            self.shared
                .timing
                .borrow_mut()
                .stages([("rasterize", now_ms() - start)]);
        }
        self.texture = Some(texture);
        Some(ProducedTexture {
            view,
            generation: output.source_revision,
            alpha: output.alpha,
            encoding: output.encoding,
        })
    }
    fn portable_scene(
        &mut self,
        frame: &cambium_rootstock::ProducerFrameInfo,
    ) -> Result<cambium_rootstock::ProducedScene, cambium_rootstock::PortableProducerError> {
        self.produce(frame)
    }
    fn suspend(&mut self) {
        self.shared.canvas.borrow_mut().reset_frame_time();
        self.texture = None;
        self.shared.gpu.borrow_mut().take();
        self.shared.physics_device.borrow_mut().take();
        #[cfg(feature = "canvas-gpu")]
        {
            self.shared.canvas.borrow_mut().set_physics_device(None);
            self.shared
                .remote
                .borrow_mut()
                .board
                .set_physics_device(None);
        }
        self.shared.timing.borrow_mut().release_device();
    }

    /// While the board is shown, the slot is a list of its cards, each named
    /// by its title and placed where it is painted. Otherwise it is the
    /// graph's items on screen, each a group whose drag and pin are buttons
    /// (dynamics grammar plan, F62, F65 to F67).
    fn semantics(&mut self) -> Option<cambium_rootstock::ProducerSemantics> {
        if !self.shared.remote_shown.get() {
            return Some(graphshell::canvas_reader::describe_canvas(
                &self.shared.canvas.borrow(),
                self.shared.plant,
            ));
        }
        let (width, height) = self.shared.size.get();
        Some(remote::board_semantics(
            &self.shared.remote.borrow(),
            width,
            height,
        ))
    }

    /// A reader pressed one of an item's buttons: Pin pins it, Drag starts
    /// a keyboard move the arrows steer.
    fn act(&mut self, key: u64, id: &str) -> bool {
        if self.shared.remote_shown.get() || self.shared.selection_locked.get() {
            return false;
        }
        let done = graphshell::canvas_reader::canvas_act(
            &mut self.shared.canvas.borrow_mut(),
            key,
            id,
            self.shared.plant,
        );
        if done {
            self.shared.dirty.set(true);
        }
        done
    }
}

/// The application state the tree renders.
pub(crate) struct TreePage {
    #[cfg(feature = "moot-resources")]
    moot_resources_seen: u64,
    commands: commands::State,
    presenter: Option<presenter::Pane>,
    #[cfg(feature = "product")]
    forme: Option<forme::Pane>,
    #[cfg(feature = "product")]
    forme_owner: forme::OwnerLifecycle,
    #[cfg(feature = "product")]
    forme_error: Option<String>,
    #[cfg(feature = "product")]
    inspected_access: Option<uuid::Uuid>,
    #[cfg(feature = "product")]
    inspected_link: Option<graphshell::link_inspection::LinkTarget>,
    #[cfg(feature = "product")]
    link_history_open: bool,
    #[cfg(feature = "product")]
    opened_link_layout: Option<scenograph::link::OpenedLinkLayout>,
    #[cfg(feature = "product")]
    opened_link_error: Option<String>,
    #[cfg(feature = "product")]
    inspected_statement: Option<String>,
    #[cfg(feature = "product")]
    inspected_link_member: Option<cambium::LinkMemberTarget>,
    #[cfg(feature = "product")]
    field_inspector: field_inspection::Pane,
    #[cfg(feature = "product")]
    presentation: presentation::Pane,
    #[cfg(feature = "product")]
    query_views: query_view::Pane,
    #[cfg(feature = "product")]
    backdrop: Rc<RefCell<backdrop::Pane>>,
    #[cfg(feature = "product")]
    promoted_links: promoted_links::Pane,
    #[cfg(feature = "product")]
    dynamics_inspector: dynamics_inspection::Pane,
    #[cfg(feature = "product")]
    node_presentation: node_presentation::Pane,
    #[cfg(feature = "applets")]
    applet_seen: u64,
    #[cfg(feature = "applets")]
    applet_generation: u32,
    #[cfg(feature = "applets")]
    applet_query: cambium::TextInput,
    shared: Rc<Shared>,
    root: Element,
    projection_source: Option<String>,
    appearance_controls: appearance::Controls,
    /// Where the graph came from, for the status line and receipts.
    source: String,
    nodes: usize,
    /// The address of the node the last click picked.
    picked: Option<String>,
    product: Option<product::SavedProduct>,
    practice: Option<practice::Practice>,
    responsive_links: Option<graphshell::responsive_links::ResponsiveLinksFixture>,
    /// The host dataset the page supplied, if any (S1).
    dataset: HostedDataset,
    /// Optional explicit containment reading over the same host authority.
    grouping: Option<graphshell::host_dataset_view::GroupedHostDataset>,
    grouping_evidence: bool,
    grouping_error: Option<String>,
    /// Its history and the checkpoint shown (S3); a v1 dataset is one.
    history: Option<history::HostedHistory>,
    /// The "Graph tools" arrangement and physics section.
    physics: physics::PhysicsPanel,
    /// Which session the canvas leaf shows.
    session: remote::Session,
    /// Which Graph tools sections are open.
    sections: remote::Sections,
    /// The open remote draft's field selects.
    #[cfg_attr(not(feature = "remote"), allow(dead_code))]
    draft: remote::DraftControls,
    /// The remote generation the view last rebuilt for.
    remote_seen: u64,
    #[cfg(feature = "remote")]
    mere_view: mere_view::MereViewState,
    /// Whether a remote link has been seen, so its section opens once.
    remote_link_seen: bool,
    /// Whether the collapsed Graph tools region is open over the canvas.
    tools_open: bool,
    /// The tree's logical size, followed from the host each frame. Genet sizes
    /// a custom leaf's cross axis from its intrinsic size and does not stretch
    /// an absolutely placed box between its insets, so the view is told its
    /// size, as mere-view's harness tells its view.
    size: (u32, u32),
}

/// What became of the page's host dataset (S1).
pub(crate) enum HostedDataset {
    /// The page supplied none.
    None,
    /// Compiled, with its relations as the viewer presents them.
    Loaded(Vec<graphshell::host_dataset_view::ViewedRelation>),
    /// Refused, and why.
    Refused(String),
}

impl HostedDataset {
    fn state(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Loaded(_) => "loaded",
            Self::Refused(_) => "refused",
        }
    }
}

/// The host dataset's refusal as an alert, or its relations as a list a
/// reader reaches beside the canvas.
fn hosted_dataset(page: &TreePage) -> Option<Child> {
    match &page.dataset {
        HostedDataset::None => None,
        HostedDataset::Refused(error) => Some(Box::new(
            el("p", error.clone())
                .attr("class", "tree-refusal")
                .attr("role", "alert")
                .attr("id", "gs-dataset-refusal"),
        )),
        HostedDataset::Loaded(relations) if relations.is_empty() => None,
        HostedDataset::Loaded(relations) => Some(Box::new(
            el(
                "section",
                (
                    el("h2", "Relations"),
                    el(
                        "ul",
                        relations
                            .iter()
                            .map(|relation| {
                                let mut children: Vec<Child> =
                                    vec![Box::new(el("span", relation.spoken()))];
                                if page.grouping_evidence {
                                    for witness in &relation.witnesses {
                                        children
                                            .push(Box::new(el("p", grouping::evidence(witness))));
                                    }
                                }
                                Box::new(el("li", children).attr("role", "listitem")) as Child
                            })
                            .collect::<Vec<_>>(),
                    )
                    .attr("role", "list"),
                ),
            )
            .attr("class", "tree-relations")
            .attr("role", "region")
            .attr("aria-label", "Relations"),
        )),
    }
}

impl TreePage {
    fn resident_mere_shown(&self) -> bool {
        #[cfg(feature = "remote")]
        return self.session == remote::Session::Remote
            && self.shared.remote.borrow().reservoir.is_some();
        #[cfg(not(feature = "remote"))]
        false
    }

    fn status(&self) -> String {
        if self.session == remote::Session::Remote {
            let remote = self.shared.remote.borrow();
            #[cfg(feature = "remote")]
            if let Some(session) = &remote.reservoir {
                let model = session.model();
                let layout = model
                    .layouts
                    .iter()
                    .find(|(id, _)| id == &model.layout)
                    .map_or(model.layout.as_str(), |(_, label)| label.as_str());
                let focus = session.focus_key();
                let record = focus.as_ref().and_then(|key| session.records().get(key));
                let layout = if layout.is_empty() {
                    String::new()
                } else {
                    format!(" · {layout}")
                };
                return format!(
                    "Resident mere: {} · {} records · {}{} · record: {} · {}",
                    model.title,
                    model.graph.nodes.len(),
                    remote.status(),
                    layout,
                    record.map_or("none", |card| card.title.as_str()),
                    if session.primary().is_none() {
                        "no attachment"
                    } else if session.read_only() {
                        "read-only"
                    } else {
                        "writable"
                    },
                );
            }
            return format!(
                "{} · {} cards · {}",
                remote.label(),
                remote.card_labels().len(),
                remote.status(),
            );
        }
        if let Some(pane) = &self.presenter {
            return format!(
                "{} visible of {} disclosed · {} selected",
                pane.model.visible.len(),
                pane.model.envelope.dataset.occurrences.len(),
                pane.model.selected.len()
            );
        }
        let selected = self.shared.canvas.borrow().selected_members().len();
        format!(
            "{} · {} nodes · selected: {} · primary: {}",
            self.source,
            self.shared.canvas.borrow().graph().node_count(),
            selected,
            self.product
                .as_ref()
                .filter(|product| product.selected.is_some())
                .map(|product| product.address.as_str())
                .or(self.picked.as_deref())
                .unwrap_or("none")
        )
    }
}

type Child = Box<dyn AnyView<TreePage, (), GenetCtx, GenetElement>>;
type Logic = fn(&TreePage) -> Child;

fn view(page: &TreePage) -> Child {
    if let Some(view) = responsive_links::view(page) {
        return view;
    }
    if let Some(view) = practice::view(page) {
        return view;
    }
    #[cfg(feature = "applets")]
    if page.shared.applet.borrow().mount.is_some() {
        return applet::view(page);
    }
    let (width, height) = page.size;
    // Genet does not stretch a custom leaf across its cross axis, so the
    // canvas column is told the width the tools region leaves it.
    let docked = tools_docked(page);
    let canvas_width = if presenter::docked(page) {
        width.saturating_sub(presenter::WIDTH).max(1)
    } else if docked {
        width.saturating_sub(TOOLS_WIDTH).max(1)
    } else {
        width.max(1)
    };
    // Wheel and touch use the graph policy; hover uses target-local coordinates.
    let graph = cambium::request_focus(
        on_key(
            on_pointer(
                cambium::on_hover(
                    custom_leaf::<TreePage, ()>(CANVAS_KEY, canvas_width, 1)
                        .attr("class", "tree-canvas")
                        .attr("role", "img")
                        .attr("tabindex", "0")
                        .attr("aria-label", "Graph"),
                    |page: &mut TreePage, event: cambium::HoverEvent| {
                        let (x, y) = if event.phase == cambium::HoverPhase::Leave {
                            (-10000., -10000.)
                        } else {
                            event.local
                        };
                        if page.resident_mere_shown() {
                            return;
                        }
                        page.shared.canvas.borrow_mut().cursor_moved(x, y);
                        page.shared.dirty.set(true);
                        event.defer_rebuild();
                    },
                ),
                |page: &mut TreePage, event: cambium::PointerEvent| page.pointer(event),
            ),
            |page: &mut TreePage, event: cambium::KeyEvent| {
                if commands::shortcut(page, &event) {
                    return;
                }
                #[cfg(feature = "product")]
                if field_inspection::cancel_pick(page, &event.key) {
                    event.prevent_default();
                    return;
                }
                #[cfg(feature = "product")]
                if page
                    .forme
                    .as_ref()
                    .is_some_and(|pane| pane.session.editing())
                    && forme::shortcut(page, &event)
                {
                    event.prevent_default();
                    return;
                }
                if keys(page, &event.key) {
                    event.prevent_default();
                }
            },
        ),
        page.commands.focus_graph,
    );
    let workbench = forme_workbench(page);
    let showing_workbench = workbench.is_some();
    let inline_product_controls = showing_workbench || !instrument_class(page).is_empty();
    #[cfg(feature = "product")]
    let scene_header = instrument::scene_header(page, showing_workbench);
    #[cfg(not(feature = "product"))]
    let scene_header: Option<Child> = None;
    #[cfg(feature = "product")]
    let backdrop_workspace = backdrop::workspace(page);
    #[cfg(not(feature = "product"))]
    let backdrop_workspace: Option<Child> = None;
    #[cfg(feature = "product")]
    let app_header = instrument::app_header(page);
    #[cfg(not(feature = "product"))]
    let app_header: Option<Child> = None;
    #[cfg(feature = "product")]
    let drop_feedback = (!showing_workbench)
        .then(|| forme::drop_feedback(page))
        .flatten();
    #[cfg(not(feature = "product"))]
    let drop_feedback: Option<Child> = None;
    Box::new(
        on_key(
            el(
                "main",
                (
                    app_header.unwrap_or_else(|| {
                        Box::new(el(
                            "h1",
                            page.presenter
                                .as_ref()
                                .map_or("Graphshell, one tree", |p| p.options.caption.as_str())
                                .to_owned(),
                        ))
                    }),
                    el("p", page.status())
                        .attr("class", "tree-status")
                        .attr("role", "status"),
                    history::region(page),
                    page.presenter
                        .is_none()
                        .then(|| hosted_dataset(page))
                        .flatten(),
                    page.presenter
                        .is_none()
                        .then(|| grouping::controls(page))
                        .flatten(),
                    if page.presenter.is_some() {
                        presenter::toolbar(page)
                    } else {
                        controls::toolbar(page)
                    },
                    forme_toolbar(page),
                    inline_product_controls.then(|| product::controls(page)),
                    el(
                        "div",
                        (
                            (!inline_product_controls).then(|| product::controls(page)),
                            product::projection_editor(page),
                            product::menu(page),
                            el(
                                "div",
                                (
                                    scene_header,
                                    backdrop_workspace
                                        .or(workbench)
                                        .unwrap_or_else(|| Box::new(graph)),
                                    drop_feedback,
                                ),
                            )
                            .attr("class", "tree-graph"),
                            presenter::panel(page),
                            if docked {
                                Some(tools_region(page))
                            } else {
                                page.tools_open.then(|| {
                                    Box::new(
                                        el("div", tools_region(page))
                                            .attr("class", "tools-overlay"),
                                    ) as Child
                                })
                            },
                        ),
                    )
                    .attr(
                        "class",
                        if page.presenter.is_some() && !presenter::docked(page) {
                            "tree-body tree-presenter-stacked"
                        } else {
                            "tree-body"
                        },
                    ),
                    commands::view(page),
                ),
            )
            .attr(
                "class",
                format!(
                    "{} {} {}",
                    if page.grouping.is_some() {
                        "tree-grouped"
                    } else {
                        ""
                    },
                    if presenter::browsing(page) {
                        "tree-presenter tree-browsing"
                    } else if page.presenter.is_some() {
                        "tree-presenter"
                    } else {
                        ""
                    },
                    instrument_class(page)
                ),
            )
            .attr(
                "style",
                format!("position:relative;width:{width}px;height:{height}px;"),
            ),
            |page: &mut TreePage, event| {
                commands::shortcut(page, &event);
            },
        )
        .capture(true)
        .focusable(false),
    )
}

fn instrument_class(page: &TreePage) -> &'static str {
    #[cfg(feature = "product")]
    if instrument::enabled(page) {
        return "instrument-workspace tree-instrument";
    }
    #[cfg(not(feature = "product"))]
    let _ = page;
    ""
}

/// Whether the Graph tools region docks beside the canvas at this width.
fn tools_docked(page: &TreePage) -> bool {
    page.presenter.is_none() && page.size.0 >= TOOLS_DOCK_MIN_WIDTH
}

/// The Graph tools region: the storage line on `app=local`, then its sections.
fn tools_region(page: &TreePage) -> Child {
    let mut children: Vec<Child> = Vec::new();
    children.extend(appearance::view(page));
    #[cfg(feature = "moot-resources")]
    children.push(moot_resources::section(page));
    #[cfg(feature = "product")]
    if let Some(header) = instrument::inspector_header(page) {
        children.push(header);
    }
    if let Some(product) = &page.product {
        children.push(Box::new(
            el("p", format!("Storage: {}", product.storage))
                .attr("class", "tools-storage")
                .attr("role", "status"),
        ));
    }
    #[cfg(feature = "remote")]
    children.extend(reservoir::view(page));
    if !page.resident_mere_shown() {
        children.push(physics::section(page));
    }
    children.push(product::find_section(page));
    children.push(product::scenes_section(page));
    children.push(product::projection_section(page));
    #[cfg(feature = "product")]
    if page.product.is_some() {
        children.push(backdrop::control(page));
    }
    children.push(product::history_section(page));
    #[cfg(feature = "product")]
    children.push(dynamics_inspection::section(page));
    #[cfg(feature = "product")]
    children.push(inspection::section(page));
    #[cfg(feature = "product")]
    children.push(link_inspection::section(page));
    #[cfg(feature = "product")]
    children.push(field_inspection::section(page));
    #[cfg(feature = "product")]
    children.push(presentation::section(page));
    #[cfg(feature = "product")]
    children.push(query_view::section(page));
    #[cfg(feature = "product")]
    children.push(promoted_links::section(page));
    #[cfg(feature = "product")]
    children.push(node_presentation::section(page));
    #[cfg(feature = "product")]
    children.push(forme::section(page));
    children.push(remote::section(page));
    Box::new(
        el("aside", children)
            .attr("class", "tree-tools workspace-controls")
            .attr("aria-label", "Graph tools"),
    )
}

/// Arrows pan and plus or minus zoom, as on the main page. During a
/// keyboard move the arrows nudge the item instead, Enter drops it and
/// Escape puts it back (F67).
impl TreePage {
    /// The command contract used by menus and explicit command-API scenarios.
    fn command(&mut self, command: &str) -> bool {
        if remote::run_command(self, command) {
            self.shared
                .events
                .borrow_mut()
                .push(format!("command {command}"));
            return true;
        }
        if self.resident_mere_shown() {
            return false;
        }
        if let Some(action) = controls::named_command(command) {
            if let CanvasCommand::Pan { dx, dy } = action {
                self.shared.pan(dx, dy);
                self.shared
                    .events
                    .borrow_mut()
                    .push(format!("command {command}"));
                return true;
            }
            action.apply(&mut *self.shared.canvas.borrow_mut(), self.size);
            crate::web_suggestions::after_command(action, &self.shared.canvas.borrow());
            self.shared
                .events
                .borrow_mut()
                .push(format!("command {command}"));
            self.shared.dirty.set(true);
            return true;
        }
        let Some(product) = &mut self.product else {
            return false;
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        match product.run_command(command, &mut canvas, &self.physics.layout_id, self.size) {
            Ok(()) => {
                self.shared
                    .events
                    .borrow_mut()
                    .push(format!("command {command}"));
                self.shared.dirty.set(true);
                true
            },
            Err(error) => {
                product.status = error;
                false
            },
        }
    }
}

fn keys(page: &mut TreePage, key: &Key) -> bool {
    if page.resident_mere_shown() {
        return false;
    }
    if matches!(key, Key::Named(NamedKey::Escape)) {
        if let Some(product) = &mut page.product {
            if product.menu_open() {
                product.close_menu();
                return true;
            }
        }
    }
    use graphshell::canvas_reader::{MoveKey, key_move};
    if page
        .product
        .as_ref()
        .is_some_and(|product| product.selection_locked())
    {
        // Keep Tab and other non-graph keys available to leave the graph and
        // reach Retry intake; only commands that could change this view wait.
        return match key {
            Key::Named(
                NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::Enter
                | NamedKey::Escape,
            ) => true,
            Key::Character(text) => text == "+" || text == "=" || text == "-",
            _ => false,
        };
    }
    let move_key = match key {
        Key::Named(NamedKey::ArrowLeft) => Some(MoveKey::Left),
        Key::Named(NamedKey::ArrowRight) => Some(MoveKey::Right),
        Key::Named(NamedKey::ArrowUp) => Some(MoveKey::Up),
        Key::Named(NamedKey::ArrowDown) => Some(MoveKey::Down),
        Key::Named(NamedKey::Enter) => Some(MoveKey::Drop),
        Key::Named(NamedKey::Escape) => Some(MoveKey::Back),
        _ => None,
    };
    if let Some(move_key) = move_key
        && key_move(&mut page.shared.canvas.borrow_mut(), move_key, PAN_STEP)
    {
        page.shared.dirty.set(true);
        return true;
    }
    if let Some(product) = &mut page.product {
        match key {
            Key::Named(NamedKey::Enter) => {
                let mut canvas = page.shared.canvas.borrow_mut();
                let _ = product.run_command(
                    "node:edit",
                    &mut canvas,
                    &page.physics.layout_id,
                    page.size,
                );
                page.shared
                    .events
                    .borrow_mut()
                    .push("command node:edit".into());
                return true;
            },
            Key::Named(NamedKey::Escape) if product.detail_open => {
                product.detail_open = false;
                page.shared
                    .events
                    .borrow_mut()
                    .push("command close-detail".into());
                return true;
            },
            _ => {},
        }
    }
    let command = match key {
        Key::Named(NamedKey::ArrowLeft) => "pan-left",
        Key::Named(NamedKey::ArrowRight) => "pan-right",
        Key::Named(NamedKey::ArrowUp) => "pan-up",
        Key::Named(NamedKey::ArrowDown) => "pan-down",
        Key::Character(text) if text == "+" || text == "=" => "view:zoom_in",
        Key::Character(text) if text == "-" => "view:zoom_out",
        _ => return false,
    };
    page.command(command)
}

/// The mounted page, for the scenario entry that reaches it from JavaScript.
struct Tree {
    shared: Rc<Shared>,
    window: WebWindow,
    lane: Option<Lane<TreeLane>>,
    appearance: cambium_genet_web_host::Mounted<TreePage, Logic, Child>,
    gestures: gestures::GestureOwner,
    root: Element,
    requested_presenter: Option<String>,
}

/// Startup owns a mounted host until all viewer listeners are installed.
struct MountedTreeGuard(Option<cambium_genet_web_host::Mounted<TreePage, Logic, Child>>);
impl std::ops::Deref for MountedTreeGuard {
    type Target = cambium_genet_web_host::Mounted<TreePage, Logic, Child>;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("startup owns host")
    }
}
impl Drop for MountedTreeGuard {
    #[inline(never)]
    fn drop(&mut self) {
        if let Some(mounted) = &self.0 {
            mounted.dispose();
        }
    }
}
fn remove_epoch_canvas(root: &Element, epoch: u64) {
    if let Ok(Some(canvas)) =
        root.query_selector(&format!("canvas[data-graphshell-canvas='{epoch}']"))
    {
        canvas.remove();
    }
}

thread_local! {
    static TREE: RefCell<Option<Tree>> = const { RefCell::new(None) };
    static TREE_EPOCH: Cell<u64> = const { Cell::new(0) };
    static TREE_PENDING: RefCell<Option<Element>> = const { RefCell::new(None) };
    static TREE_PENDING_CANVAS: RefCell<Option<(u64, HtmlCanvasElement)>> = const { RefCell::new(None) };
}

/// Whether the tree page is mounted, rather than the main page.
pub(crate) fn mounted() -> bool {
    TREE.with(|tree| tree.borrow().is_some())
}

/// Dispose only the appearance (or pending boot) owned by this element.
#[wasm_bindgen]
pub fn dispose_tree(root: Element) {
    let pending = TREE_PENDING.with(|pending| {
        let matches = pending
            .borrow()
            .as_ref()
            .is_some_and(|owner| owner.is_same_node(Some(&root)));
        if matches {
            pending.borrow_mut().take();
        }
        matches
    });
    if pending {
        TREE_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
        TREE_PENDING_CANVAS.with(|canvas| {
            if let Some((epoch, canvas)) = canvas.borrow_mut().take() {
                remove_epoch_canvas(&root, epoch);
                canvas.remove();
            }
        });
    }
    TREE.with(|tree| {
        let matches = tree
            .borrow()
            .as_ref()
            .is_some_and(|tree| tree.root.is_same_node(Some(&root)));
        if matches {
            if let Some(tree) = tree.borrow_mut().take() {
                tree.appearance.dispose();
                tree.window.canvas().remove();
                match &tree.requested_presenter {
                    Some(value) => {
                        let _ = root.set_attribute("data-presenter", value);
                    },
                    None => {
                        let _ = root.remove_attribute("data-presenter");
                    },
                }
            }
        }
    });
    for attribute in [
        "data-ready",
        "data-context-generation",
        "data-presentation-sequence",
        "data-selected-render-backend",
        "data-compute-available",
    ] {
        let _ = root.remove_attribute(attribute);
    }
}

/// Mount the one-tree page into `root`.
#[wasm_bindgen]
pub fn mount_tree(root: Element) -> Result<(), JsValue> {
    if !root.is_connected() {
        return Err(JsValue::from_str("the mount root is not connected"));
    }
    if mounted() || TREE_PENDING.with(|pending| pending.borrow().is_some()) {
        return Err(JsValue::from_str(
            "Graphshell is already mounted on this page",
        ));
    }
    let owns_title = root.has_attribute("data-owns-title");
    let failure_root = root.clone();
    let epoch = TREE_EPOCH.with(|epoch| {
        let next = epoch.get().wrapping_add(1);
        epoch.set(next);
        next
    });
    TREE_PENDING.with(|pending| *pending.borrow_mut() = Some(root.clone()));
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = boot(root.clone(), epoch).await {
            remove_epoch_canvas(&failure_root, epoch);
            // dispose_tree already released a pending boot it cancelled.
            if TREE_EPOCH.with(|current| current.get() != epoch) {
                return;
            }
            TREE_PENDING.with(|pending| pending.borrow_mut().take());
            TREE_PENDING_CANVAS.with(|pending| pending.borrow_mut().take());
            if !root.is_connected() {
                let _ = root.set_attribute("data-mount-state", "cancelled");
                return;
            }
            let _ = failure_root.set_attribute("data-mount-error", &error);
            web_sys::console::error_1(&error.clone().into());
            if let Some(document) = web_sys::window()
                .and_then(|window| window.document())
                .filter(|_| owns_title)
            {
                document.set_title(&format!("GRAPHSHELL TREE FAIL: {error}"));
            }
        }
    });
    Ok(())
}

#[inline(never)]
fn boot(
    root: Element,
    epoch: u64,
) -> std::pin::Pin<Box<impl std::future::Future<Output = Result<(), String>>>> {
    Box::pin(boot_inner(root, epoch))
}

fn page_fonts() -> Vec<HostFont> {
    let face = |bytes: &[u8]| HostFont {
        family: None,
        bytes: bytes.to_vec(),
    };
    #[allow(unused_mut)]
    let mut fonts = vec![face(include_bytes!("../web/IBMPlexSans-Regular.ttf"))];
    #[cfg(feature = "product")]
    fonts.extend([
        face(include_bytes!("../web/IBMPlexMono-Regular.ttf")),
        face(include_bytes!("../web/Newsreader-Regular.ttf")),
    ]);
    fonts
}

async fn boot_inner(root: Element, epoch: u64) -> Result<(), String> {
    if !root.is_connected() || TREE_EPOCH.with(|current| current.get() != epoch) {
        return Err("appearance boot cancelled".into());
    }
    // The embedding root and this boot's epoch own startup admission. Keep the
    // checkpoint passive: a late private acquisition reaches the host's
    // post-boot destruction path; dispose_tree advances the epoch.
    let live_root = root.clone();
    let lifetime = MountLifetime::with_liveness(move || {
        live_root.is_connected() && TREE_EPOCH.with(|current| current.get() == epoch)
    });
    #[cfg(feature = "product")]
    owner_waypoint::configure(&root);
    let document = root.owner_document().ok_or("the root has no document")?;
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "could not create the canvas")?
        .dyn_into()
        .map_err(|_| "the canvas is not a canvas")?;
    canvas.set_id("gs-graphshell-canvas");
    canvas
        .set_attribute("data-graphshell-canvas", &epoch.to_string())
        .map_err(|_| "cannot identify owned canvas")?;
    // One finger may scroll the page; pinching is the viewer's (`gestures`).
    canvas
        .set_attribute(
            "style",
            "display:block;width:100%;height:100%;touch-action:pan-x pan-y;",
        )
        .map_err(|_| "could not size the canvas")?;
    root.append_child(&canvas)
        .map_err(|_| "could not place the canvas")?;
    TREE_PENDING_CANVAS.with(|pending| *pending.borrow_mut() = Some((epoch, canvas.clone())));
    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;

    // A host dataset (S1) or history (S3) replaces every other source, and a
    // refused one shows its refusal over an empty graph, never the fixture.
    let practice = practice::open(&root)?;
    let responsive_links = responsive_links::open(&root)?;
    let hosted = crate::web_dataset::supplied_history(&root);
    let presenter_options = presenter::requested(&root)?;
    if presenter_options.is_some() && hosted.is_none() {
        return Err("Inspector presenter requires a disclosed host dataset".into());
    }
    let mut product = if hosted.is_some() {
        None
    } else {
        product::open(&root).await?
    };
    #[cfg(feature = "product")]
    let (mut forme, mut forme_error) = match &product {
        Some(p) => match forme::Pane::open(p).await {
            Ok(pane) => (Some(pane), None),
            Err(error) => (
                None,
                Some(format!(
                    "Workbench could not be restored · {error}. The saved record has been retained."
                )),
            ),
        },
        None => (None, None),
    };
    #[cfg(feature = "product")]
    let presentation = presentation::Pane::open(product.as_ref()).await;
    let mut dataset = HostedDataset::None;
    let mut grouping = None;
    let mut hosted_history = None;
    let mut placed = None;
    let (graph, source) = if let Some(hosted) = hosted {
        match hosted
            .and_then(|history| history::HostedHistory::open(history, grouping::requested(&root)?))
        {
            Ok((hosted, view, grouped)) => {
                hosted_history = Some(hosted);
                grouping = grouped;
                dataset = HostedDataset::Loaded(view.relations);
                placed = Some(view.positions);
                (
                    view.graph,
                    format!("host dataset, revision {}", view.revision),
                )
            },
            Err(error) => {
                dataset = HostedDataset::Refused(error);
                (Graph::new(), "host dataset refused".to_string())
            },
        }
    } else if let Some(product) = &product {
        placed = product.initial_fixture_positions();
        (product.graph(), "saved graph".to_string())
    } else {
        match web_graphs::requested() {
            Some((nodes, seed)) => (
                web_graphs::generated(nodes, seed, web_graphs::links()),
                format!("generated, seed {seed}"),
            ),
            None => (fixture_graph()?, "fixture".to_string()),
        }
    };
    let nodes = graph.node_count();
    let presenter = match presenter_options {
        Some(options) => {
            let history = hosted_history
                .as_ref()
                .ok_or("Inspector source was refused")?;
            let envelope = history.envelope_at(history.index)?;
            let mut model = graphshell::host_presenter::HostPresenter::new(envelope);
            model.set_visible(
                graph
                    .nodes()
                    .filter_map(|(_, node)| {
                        Some((
                            node.url().strip_prefix("urn:host-dataset:")?.to_owned(),
                            node.id,
                        ))
                    })
                    .collect(),
            );
            Some(presenter::Pane {
                model,
                options,
                roster_open: false,
                evidence_open: false,
                folds_open: false,
                history_open: false,
                checkpoint_source_open: false,
                selection_open: false,
            })
        },
        None => None,
    };
    let requested_presenter = root.get_attribute("data-presenter");
    let speed_options = crate::web_speed::options()?;
    let appearance = crate::web_appearance::mount_retained(
        root.clone(),
        product.is_some() && root.has_attribute("data-appearance-application"),
    )?;
    let appearance_controls = appearance::Controls::new(&appearance.borrow());
    let initial_sheet = tree_stylesheet(&appearance.borrow().sheet());
    let appearance_revision = Cell::new(appearance.borrow().revision);
    let shared = Rc::new(Shared {
        adapter_info: RefCell::new(None),
        events: RefCell::new(Vec::new()),
        #[cfg(feature = "product")]
        reading: RefCell::new(reading::Reading::default()),
        #[cfg(feature = "moot-resources")]
        moot_resources: RefCell::new(graphshell::moot_resources::Host::default()),
        wheel_mode: Cell::new(gestures::requested_wheel(&root)?),
        command_request: RefCell::new(None),
        #[cfg(feature = "applets")]
        applet: RefCell::new(applet::Pane::default()),
        appearance,
        appearance_revision,

        canvas: RefCell::new({
            let mut canvas = match &placed {
                Some(positions) => web_graphs::placed_canvas(graph, positions, width, height),
                None => web_graphs::prepared_canvas(graph, width, height),
            };
            canvas.set_suggestion_settings(crate::web_suggestions::load());
            canvas
        }),
        dirty: Cell::new(true),
        moving: Cell::new(false),
        size: Cell::new((width, height)),
        physical_size: Cell::new([0, 0]),
        gpu: RefCell::new(None),
        timing: RefCell::new(FrameTiming::default()),
        physics_config: controls::physics_config()?,
        speed: speed_options,
        pace: RefCell::new(speed::PaceWindow::with_grain(
            speed_options.grain.as_micros() as i64,
        )),
        reached: RefCell::new(crate::web_speed::ReachedNote::default()),
        frame_budget: RefCell::new(crate::web_speed::frame_budget(speed_options)),
        period_worker: crate::web_period_worker::PeriodWorker::start(speed_options.period_source),
        gpu_options: controls::gpu_options()?,
        physics_device: RefCell::new(None),
        visibility: visibility::requested()?,
        release_watch: Cell::new(None),
        release_step: Cell::new(None),
        release_watch_world: Cell::new(None),
        release_step_world: Cell::new(None),
        press_point: Cell::new(None),
        release_log: RefCell::new(Vec::new()),
        physics_log: RefCell::new(Vec::new()),
        faces: Cell::new(None),
        remote: Rc::new(RefCell::new(remote::TreeRemote::new())),
        remote_shown: Cell::new(false),
        selection_locked: Cell::new(false),
        plant: controls::reader_plant()?,
        root: root.clone(),
        gestures: RefCell::new(gestures::Gestures::default()),
    });
    if let Some(product) = &mut product {
        product.restore_canvas_link_hides(&mut shared.canvas.borrow_mut());
    }

    shared
        .appearance
        .borrow()
        .apply_canvas(&mut shared.canvas.borrow_mut());
    if shared.reduced_motion() {
        shared.canvas.borrow_mut().set_physics_paused(true);
    }
    #[cfg(feature = "product")]
    if let Some(pane) = &forme {
        if let Err(error) = pane.install(&mut shared.canvas.borrow_mut()) {
            forme_error = Some(format!("Workbench could not be projected · {error}"));
            forme = None;
        }
    }
    if let Some(slice) = controls::meaning_slice()? {
        shared.canvas.borrow_mut().set_meaning_slice(slice);
    }
    crate::web_speed::apply(
        &mut shared.canvas.borrow_mut(),
        shared.speed,
        &shared.frame_budget.borrow(),
    );
    visibility::install(&shared, &document, &lifetime)?;
    let choice = match root
        .get_attribute("data-render-backend")
        .as_deref()
        .unwrap_or("auto")
    {
        "auto" => cambium_rootstock::PresentationChoice::Auto,
        "webgpu" => cambium_rootstock::PresentationChoice::WebGpu,
        "webgl2" => cambium_rootstock::PresentationChoice::WebGl2,
        value => return Err(format!("invalid render_backend: {value}")),
    };
    if root.has_attribute("data-presentation-diagnostics") {
        canvas
            .set_attribute("data-presentation-diagnostics", "true")
            .map_err(|_| "cannot enable presentation diagnostics")?;
    }
    let options = HostOptions {
        presentation_choice: choice,
        // WebGPU draws the graph's portable scene in the page pass, as WebGL2
        // does, rather than resampling a producer texture at fractional scale.
        wgpu_portable_scene: true,
        title: "Graphshell, one tree".into(),
        netrender: Box::new(|| NetrenderOptions {
            tile_cache_size: Some(1024),
            enable_vello: true,
            optional_features: wgpu::Features::TIMESTAMP_QUERY,
            ..Default::default()
        }),
        ..Default::default()
    };
    let page_shared = shared.clone();
    let page_root = root.clone();
    let physics = physics::PhysicsPanel::new(&shared.canvas.borrow(), web_graphs::LAYOUT);
    let mounted = mount_with_lifetime(
        canvas.clone(),
        options,
        lifetime,
        move |_window, _commands, _wake| Init {
            state: TreePage {
                #[cfg(feature = "moot-resources")]
                moot_resources_seen: 0,
                commands: commands::State::default(),
                presenter,
                #[cfg(feature = "product")]
                forme,
                #[cfg(feature = "product")]
                forme_owner: forme::OwnerLifecycle::default(),
                #[cfg(feature = "product")]
                forme_error,
                #[cfg(feature = "product")]
                inspected_access: None,
                #[cfg(feature = "product")]
                inspected_link: None,
                #[cfg(feature = "product")]
                link_history_open: false,
                #[cfg(feature = "product")]
                opened_link_layout: None,
                #[cfg(feature = "product")]
                opened_link_error: None,
                #[cfg(feature = "product")]
                inspected_statement: None,
                #[cfg(feature = "product")]
                inspected_link_member: None,
                #[cfg(feature = "product")]
                field_inspector: field_inspection::Pane::default(),
                #[cfg(feature = "product")]
                presentation,
                #[cfg(feature = "product")]
                query_views: query_view::Pane::default(),
                #[cfg(feature = "product")]
                backdrop: Rc::new(RefCell::new(backdrop::Pane::default())),
                #[cfg(feature = "product")]
                promoted_links: promoted_links::Pane::default(),
                #[cfg(feature = "product")]
                dynamics_inspector: dynamics_inspection::Pane::default(),
                #[cfg(feature = "product")]
                node_presentation: node_presentation::Pane::default(),
                #[cfg(feature = "applets")]
                applet_seen: 0,
                #[cfg(feature = "applets")]
                applet_generation: 0,
                #[cfg(feature = "applets")]
                applet_query: cambium::TextInput::new(""),
                shared: page_shared,
                root: page_root,
                projection_source: None,
                appearance_controls,
                source,
                nodes,
                picked: None,
                product,
                practice,
                responsive_links,
                dataset,
                grouping,
                grouping_evidence: false,
                grouping_error: None,
                history: hosted_history,
                physics,
                tools_open: false,
                session: remote::Session::Local,
                sections: remote::Sections::default(),
                draft: remote::DraftControls::default(),
                remote_seen: 0,
                #[cfg(feature = "remote")]
                mere_view: mere_view::MereViewState::default(),
                remote_link_seen: false,
                size: (width, height),
            },
            logic: view as Logic,
            sheet: initial_sheet,
            // A browser lends genet no system faces, so the page brings its own: the
            // design system's interface face, and in the product its code and reading
            // faces (OFL; notices in web/). The viewer carries one face to hold its
            // size budget.
            fonts: page_fonts(),
            images: Vec::new(),
        },
        hooks(shared.clone()),
    )
    .await?;
    let mut mounted = MountedTreeGuard(Some(mounted));
    if !root.is_connected() || TREE_EPOCH.with(|current| current.get() != epoch) {
        mounted.dispose();
        return Err("appearance boot cancelled".into());
    }
    let backend = mounted
        .host()
        .borrow()
        .s
        .presentation_driver
        .as_ref()
        .map(|driver| driver.backend());
    let selected = if backend == Some(cambium_rootstock::PresentationBackend::WebGl2) {
        "webgl2"
    } else {
        "webgpu"
    };
    root.set_attribute("data-selected-render-backend", selected)
        .map_err(|_| "cannot report backend")?;
    root.set_attribute(
        "data-compute-available",
        if selected == "webgpu" {
            "true"
        } else {
            "false"
        },
    )
    .map_err(|_| "cannot report compute")?;
    if selected == "webgl2"
        && web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok())
            .and_then(|params| params.get("gpu"))
            .as_deref()
            == Some("on")
    {
        mounted.dispose();
        return Err("GPU physics requires the WebGPU presentation backend".into());
    }
    let zoom_host = mounted.host().clone();
    let release_host = mounted.host().clone();
    let gesture_owner = gestures::install(
        &root,
        mounted.window().canvas().clone(),
        shared.clone(),
        gestures::HostHandle {
            ui_zoom: Rc::new(move || zoom_host.borrow().ui_zoom()),
            release: Box::new(move || release_host.borrow_mut().release()),
            window: mounted.window().clone(),
        },
    )?;
    root.set_attribute("data-presenter", "tree")
        .map_err(|_| "could not identify presenter")?;
    root.set_attribute("data-ready", "true")
        .map_err(|_| "could not mark the page ready")?;
    TREE.with(|tree| {
        *tree.borrow_mut() = Some(Tree {
            shared,
            window: mounted.window().clone(),
            lane: None,
            root: root.clone(),
            appearance: mounted.0.take().expect("viewer takes mounted host"),
            gestures: gesture_owner,
            requested_presenter,
        })
    });
    if root.has_attribute("data-owns-title") {
        document.set_title(if root.tag_name().eq_ignore_ascii_case("graphshell-view") {
            "GRAPHSHELL H3 READY · Graphshell"
        } else {
            "Graphshell, one tree"
        });
    }
    TREE_PENDING.with(|pending| pending.borrow_mut().take());
    TREE_PENDING_CANVAS.with(|pending| pending.borrow_mut().take());
    Ok(())
}

/// The graph the main page opens with, from its fixture, without touching the
/// main page's IndexedDB.
fn fixture_graph() -> Result<Graph, String> {
    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.to_string(),
        profile: "profile:graphshell-tree".to_string(),
    };
    let app =
        GraphshellApp::fixture(MemoryBackend::new(), persona).map_err(|error| error.to_string())?;
    Ok(app.host.graph().clone())
}

fn hooks(shared: Rc<Shared>) -> HostHooks<TreePage, Logic, Child> {
    let frame_shared = shared.clone();
    let after_shared = shared.clone();
    HostHooks {
        frame: Box::new(move |ctx| {
            if let Some(core) = ctx.render_core {
                attach_gpu(&frame_shared, core);
            }
            if ctx.runner.state().appearance_controls.changed() {
                ctx.runner.update(appearance::apply);
            }
            let incoming = ctx
                .runner
                .state()
                .root
                .get_attribute("data-projection-dataset");
            if incoming != ctx.runner.state().projection_source {
                ctx.runner.update(|page| {
                    page.projection_source = incoming.clone();
                    if let Some(json) = incoming {
                        let definition = page.root.get_attribute("data-projection-definition");
                        if let Some(product) = &mut page.product {
                            product.load_projection_input(&json, definition.as_deref());
                        }
                    }
                });
            }
            if practice::needs_frame(ctx.runner.state()) {
                ctx.runner.update(|page| {
                    practice::advance(page, now_ms() as f64);
                });
            }
            #[cfg(feature = "moot-resources")]
            {
                let version = frame_shared.moot_resources.borrow().version;
                if version != ctx.runner.state().moot_resources_seen {
                    ctx.runner.update(|page| {
                        page.moot_resources_seen = version;
                        page.tools_open = true;
                    });
                }
            }
            if let Some(request) = frame_shared.command_request.borrow_mut().take() {
                if matches!(&request, commands::Request::Selection) {
                    // Secondary capture bypasses the host's focus routing.
                    // Pulse the one-shot request even after an earlier close.
                    ctx.runner.update(|page| page.commands.focus_graph = false);
                }
                ctx.runner.update(|page| commands::open(page, request));
            }
            if presenter::needs_capture(ctx.runner.state()) {
                ctx.runner.update(|page| {
                    page.capture_host_selection();
                });
            }
            #[cfg(feature = "applets")]
            {
                let (version, generation) = {
                    let pane = frame_shared.applet.borrow();
                    (pane.version, pane.generation())
                };
                if ctx.runner.state().applet_seen != version {
                    ctx.runner.update(|page| {
                        page.applet_seen = version;
                        if page.applet_generation != generation {
                            page.applet_generation = generation;
                            page.applet_query = cambium::TextInput::new("");
                        }
                    });
                }
            }
            {
                let mut appearance = frame_shared.appearance.borrow_mut();
                if let Err(message) = appearance.refresh(false) {
                    web_sys::console::error_1(&message.into());
                }
                if frame_shared.appearance_revision.get() != appearance.revision {
                    appearance.apply_canvas(&mut frame_shared.canvas.borrow_mut());
                    frame_shared.appearance_revision.set(appearance.revision);
                    *ctx.set_sheet = Some(tree_stylesheet(&appearance.sheet()));
                    frame_shared.dirty.set(true);
                }
            }
            if ctx
                .runner
                .state()
                .product
                .as_ref()
                .is_some_and(|product| product.ready())
            {
                ctx.runner.update(|page| {
                    if let Some(product) = &mut page.product {
                        let mut canvas = frame_shared.canvas.borrow_mut();
                        if product.poll(&mut canvas) {
                            page.nodes = canvas.graph().nodes().count();
                            page.picked = canvas.focused_url().map(str::to_owned);
                        }
                    }
                });
                frame_shared.dirty.set(true);
            }
            frame_shared.selection_locked.set(
                ctx.runner
                    .state()
                    .product
                    .as_ref()
                    .is_some_and(|product| product.selection_locked()),
            );
            #[cfg(feature = "product")]
            if forme::owner_ready(ctx.runner.state()) {
                ctx.runner.update(forme::poll_owner);
            }
            #[cfg(feature = "product")]
            coupling::poll(ctx);
            #[cfg(feature = "product")]
            if query_view::ready(ctx.runner.state()) {
                ctx.runner.update(query_view::poll);
            }
            #[cfg(feature = "product")]
            if promoted_links::ready(ctx.runner.state()) {
                ctx.runner.update(promoted_links::poll);
            }
            #[cfg(feature = "product")]
            field_inspection::reveal(ctx);
            #[cfg(feature = "product")]
            coupling::reveal(ctx);
            #[cfg(feature = "product")]
            if ctx.runner.state().forme.as_ref().is_some_and(|f| f.ready()) {
                ctx.runner.update(|page| {
                    forme::poll(page);
                });
            }
            #[cfg(feature = "product")]
            owner_waypoint::ready(ctx.runner.state());
            #[cfg(feature = "product")]
            if ctx.runner.state().presentation.ready() {
                ctx.runner.update(presentation::poll);
            }
            #[cfg(feature = "product")]
            {
                if let Some(inputs) = presentation::refresh(ctx.runner.state()) {
                    ctx.runner.update(|page| presentation::update(page, inputs));
                }
                presentation::install(ctx.runner.state());
                if let Some(input) = node_presentation::refresh(ctx.runner.state()) {
                    ctx.runner
                        .update(|page| node_presentation::update(page, input));
                }
                node_presentation::install(ctx.runner.state());
                if let Some(report) = dynamics_inspection::refresh(ctx.runner.state()) {
                    ctx.runner
                        .update(|page| dynamics_inspection::update(page, report));
                }
            }
            // The remote session moves outside the runner (its channel's
            // pumps); rebuild the view when it has.
            let (generation, linked) = {
                let remote = frame_shared.remote.borrow();
                (remote.generation, remote.live.is_some())
            };
            if ctx.runner.state().remote_seen != generation {
                ctx.runner.update(|page| {
                    page.remote_seen = generation;
                    #[cfg(feature = "remote")]
                    reservoir::relayout(page);
                    // The section starts closed and opens when a link exists.
                    if linked && !page.remote_link_seen {
                        page.remote_link_seen = true;
                        page.sections.remote.expanded = true;
                    }
                });
            }
            // A stop that returned anchored items (G7, F24) starts its own.
            if ctx.runner.state().physics.transition.is_some()
                || frame_shared.canvas.borrow().has_stop_return()
            {
                ctx.runner.update(|page| page.advance_arrangement(now_ms()));
            }
            // The speed picker applies when chosen ("Speed select"), and the
            // speed reached shows beneath it while the budget binds.
            if ctx.runner.state().physics.speed_pending() {
                ctx.runner.update(TreePage::apply_speed);
            }
            let (note, changed) = frame_shared
                .reached
                .borrow_mut()
                .update(&frame_shared.canvas.borrow());
            if changed {
                ctx.runner.update(|page| page.physics.speed_note = note);
            }
            let size = (
                ctx.logical_size.0.round().max(1.0) as u32,
                ctx.logical_size.1.round().max(1.0) as u32,
            );
            if ctx.runner.state().size != size {
                ctx.runner.update(|page| page.size = size);
            }
            if !ctx.producers.contains(CANVAS_KEY) {
                let producer = CanvasProducer {
                    shared: frame_shared.clone(),
                    generation: 0,
                    scene: None,
                    texture: None,
                };
                if let Err(error) = ctx.producers.register(CANVAS_KEY, producer, &[]) {
                    web_sys::console::error_1(
                        &format!("could not register the canvas producer: {error:?}").into(),
                    );
                }
            }
            #[cfg(feature = "product")]
            {
                reading::prepare(ctx.runner.state());
                backdrop::register(ctx);
                frame_shared.reading.borrow_mut().register(ctx);
                if frame_shared.reading.borrow_mut().poll() {
                    ctx.runner.update(|_| {});
                }
                let requests = frame_shared.reading.borrow_mut().take_requests();
                for (source, address) in requests {
                    if let Ok(Some(mere_document_lanes::retained_clip::ClipRequest::Export(clip))) =
                        mere_document_lanes::retained_clip::request(&address)
                    {
                        let product = ctx.runner.state().product.as_ref();
                        let owner = product.and_then(|p| p.retained_scope());
                        let store = product.and_then(|p| p.retained_store());
                        frame_shared
                            .reading
                            .borrow_mut()
                            .export_clip(source, clip, owner, store);
                        ctx.runner.update(|_| {});
                        continue;
                    }
                    let target =
                        frame_shared
                            .canvas
                            .borrow()
                            .graph()
                            .nodes()
                            .find_map(|(_, node)| {
                                (node.url() == address
                                    && (node.body.is_some()
                                        || matches!(
                                        mere_document_lanes::retained_clip::request(&address),
                                        Ok(Some(
                                            mere_document_lanes::retained_clip::ClipRequest::Read(
                                                _
                                            )
                                        ))
                                    )))
                                .then_some(node.id)
                            });
                    if let Some(target) = target {
                        ctx.runner.update(|page| forme::open_retained(page, target));
                    } else {
                        frame_shared.reading.borrow_mut().set_status(
                            source,
                            format!("That destination's content is not retained here: {address}"),
                        );
                        ctx.runner.update(|_| {});
                    }
                }
            }
            frame_shared.with_gpu(|gpu| frame_shared.timing.borrow_mut().frame_begin(gpu));
            // This comparison page continuously renders, like GpuPresenter.
            // It also keeps asynchronous Vello recovery alive when the graph
            // is settled. An event-driven host needs a renderer completion
            // signal before it can safely cache its last texture.
            true
        }),
        after_dispatch: Box::new(|ctx| {
            #[cfg(feature = "product")]
            if forme::graph_change_ready(ctx.runner.state()) {
                ctx.runner.update(forme::after_graph_change);
            }
            #[cfg(feature = "product")]
            field_inspection::reveal(ctx);
            #[cfg(feature = "product")]
            coupling::reveal(ctx);
            ctx.runner.state().shared.selection_locked.set(
                ctx.runner
                    .state()
                    .product
                    .as_ref()
                    .is_some_and(|product| product.selection_locked()),
            );
        }),
        after_frame: Box::new(move |ctx| {
            if ctx
                .runner
                .state()
                .root
                .has_attribute("data-presentation-diagnostics")
            {
                let _ = ctx.runner.state().root.set_attribute(
                    "data-physics-moving",
                    &after_shared.moving.get().to_string(),
                );
            }
            if let Some(stamp) = ctx.presentation {
                let _ = ctx.runner.state().root.set_attribute(
                    "data-context-generation",
                    &stamp.context_generation.0.to_string(),
                );
                let _ = ctx
                    .runner
                    .state()
                    .root
                    .set_attribute("data-presentation-sequence", &stamp.sequence.to_string());
            }
            // The Chronicle loader waits on the same live session that the
            // retained board presents. This is host-boundary observation only.
            {
                let remote = after_shared.remote.borrow();
                let root = &ctx.runner.state().root;
                let _ = root.set_attribute("data-remote-state", remote.status());
                if let Some(mounted) = remote.mounted() {
                    let _ = root.set_attribute(
                        "data-remote-generation",
                        &mounted.scene.tables.generation.to_string(),
                    );
                } else {
                    let _ = root.remove_attribute("data-remote-generation");
                }
            }
            if after_shared.adapter_info.borrow().is_none() {
                if let Some(info) = ctx.render_core.and_then(|core| {
                    cambium_rootstock::try_renderer(core)
                        .ok()
                        .map(|renderer| renderer.wgpu_device.core.adapter.get_info())
                }) {
                    *after_shared.adapter_info.borrow_mut() = Some(json!({
                        "name": info.name, "backend": format!("{:?}", info.backend),
                        "device_type": format!("{:?}", info.device_type),
                        "vendor": info.vendor, "device": info.device,
                        "driver": info.driver, "driver_info": info.driver_info,
                        "source": "shared host RenderCore adapter",
                    }));
                }
            }

            // Where the graph is, for the page's gestures.
            let (graph, overlays) = gestures::layout_rects(ctx);
            {
                let mut gestures = after_shared.gestures.borrow_mut();
                gestures.graph = graph;
                gestures.overlays = overlays;
            }
            lane::overlay_observation::publish(ctx, &after_shared);
            after_shared.with_gpu(|gpu| {
                let mut timing = after_shared.timing.borrow_mut();
                timing.frame_end(gpu);
                if let Some(profile) = &ctx.frame_profile {
                    timing.producer(profile.producers.render_us, profile.producers.stage_us);
                }
                timing.poll();
            });
            TREE.with(|tree| {
                if let Some(tree) = tree.borrow_mut().as_mut()
                    && let Some(lane) = tree.lane.as_mut()
                {
                    lane.after_frame(ctx);
                }
            });
        }),
        after_wake: Box::new(|_ctx| {}),
        close_request: Box::new(|_ctx, _request| CloseDisposition::KeepVisible),
        focused_text: Box::new(|runner| {
            if let Some(slot) = commands::focused_text(runner) {
                return Some(slot);
            }
            #[cfg(feature = "product")]
            if let Some(slot) = coupling::focused_text(runner) {
                return Some(slot);
            }
            #[cfg(feature = "product")]
            if let Some(slot) = presentation::focused_text(runner) {
                return Some(slot);
            }
            #[cfg(feature = "product")]
            if let Some(slot) = query_view::focused_text(runner) {
                return Some(slot);
            }
            #[cfg(feature = "product")]
            if let Some(slot) = promoted_links::focused_text(runner) {
                return Some(slot);
            }
            appearance::focused_text(runner)
        }),
        key_intercept: Box::new(|_runner, _press| false),
    }
}

/// Arm a scenario on the tree page. The lane runs from the next frame.
pub(crate) fn run(text: &str) -> Result<(), String> {
    let shared = TREE
        .with(|tree| tree.borrow().as_ref().map(|tree| tree.shared.clone()))
        .ok_or("the tree has not booted")?;
    let lane = Lane::new(
        TreeLane {
            shared,
            errors: Vec::new(),
            pointer: None,
            drop: None,
            drop_world: None,
            camera_mark: None,
        },
        Some(taproot::Scenario::parse(text).map_err(|e| e.to_string())?),
        None,
        None,
        Rc::new(Cell::new(0)),
    )
    .with_capture_backend(WebCapture);
    web_scenario::mark(&super::document()?, "running", None)?;
    TREE.with(|tree| {
        if let Some(tree) = tree.borrow_mut().as_mut() {
            tree.lane = Some(lane);
            tree.window.request_redraw();
        }
    });
    Ok(())
}

/// Write the receipt where `loader.js` collects it, and say the run is done.
fn publish(ok: bool, text: &str, shared: &Shared, saved: Option<serde_json::Value>) {
    let result = if ok { "ok" } else { "fail" };
    // The sink writes its own RESULT line from `result`.
    let log: Vec<&str> = text.lines().skip(1).collect();
    let report = json!({
        "result": result,
        "log": log,
        "timings": shared.timing.borrow().json(),
        "canvas_logical_size": shared.size.get(),
        "canvas_physical_size": shared.physical_size.get(),
        "saved_graph": saved,
        "renderer_adapter": shared.adapter_info.borrow().clone(),
    });
    let outcome = super::document().and_then(|document| {
        if let Ok(element) = web_scenario::page_element(&document, "scenario-log") {
            element.set_text_content(Some(text));
        }
        web_scenario::mark(&document, result, Some(&report.to_string()))?;
        let event = Event::new("graphshell-scenario-complete")
            .map_err(|_| "could not create the completion event".to_string())?;
        document
            .dispatch_event(&event)
            .map_err(|_| "could not announce the completion".to_string())?;
        Ok(())
    });
    if let Err(error) = outcome {
        web_sys::console::error_1(&error.into());
    }
}

#[cfg(feature = "canvas-gpu")]
use mere::canvas::PhysicsDevice;

/// Without `canvas-gpu` the page never has a physics device.
#[cfg(not(feature = "canvas-gpu"))]
enum PhysicsDevice {}

#[cfg(not(feature = "canvas-gpu"))]
impl PhysicsDevice {
    fn answers(&self) -> u64 {
        match *self {}
    }

    fn threshold(&self) -> usize {
        match *self {}
    }
}

/// Without `canvas-gpu` there is no lagged repulsion lane, so its counts stay
/// zero; the fields are seiche's `LaggedStats`.
#[cfg(not(feature = "canvas-gpu"))]
#[derive(Clone, Copy, Debug, Default)]
struct RepulsionStats {
    device_steps: u64,
    cpu_steps: u64,
    submissions: u64,
    failures: u64,
    mismatched: u64,
    waiting: u64,
    stale: u64,
    last_age: u64,
}

#[cfg(not(feature = "canvas-gpu"))]
trait NoRepulsionLane {
    fn repulsion_stats(&self) -> Option<RepulsionStats>;
}

#[cfg(not(feature = "canvas-gpu"))]
impl NoRepulsionLane for mere::canvas::Canvas {
    fn repulsion_stats(&self) -> Option<RepulsionStats> {
        None
    }
}

mod appearance;
#[cfg(feature = "applets")]
mod applet;
mod commands;
mod controls;
#[cfg(feature = "product")]
mod coupling;
#[cfg(feature = "product")]
mod dynamics_inspection;
#[cfg(feature = "product")]
mod field_inspection;
#[cfg(feature = "product")]
mod forme;
mod gesture_steps;
mod gestures;
mod grouping;
mod history;
mod host_boundary;
#[cfg(feature = "product")]
mod inspection;
#[cfg(feature = "product")]
mod instrument;
#[cfg(feature = "product")]
mod backdrop;
mod lane;
#[cfg(feature = "product")]
mod link_inspection;
#[cfg(feature = "moot-resources")]
mod moot_resources;
#[cfg(feature = "product")]
mod node_presentation;
mod observe;
#[cfg(feature = "product")]
mod owner_waypoint;
mod physics;
#[cfg(feature = "main-page")]
mod practice;
#[cfg(not(feature = "main-page"))]
#[path = "web_tree/practice_off.rs"]
mod practice;
#[cfg(feature = "product")]
mod presentation;
mod presenter;
mod producer_scene;
#[cfg(feature = "product")]
mod product;
#[cfg(not(feature = "product"))]
#[path = "web_tree/product_off.rs"]
mod product;
#[cfg(feature = "product")]
mod query_view;
#[cfg(feature = "product")]
mod promoted_links;
#[cfg(feature = "product")]
mod reading;
#[cfg(feature = "remote")]
mod remote;
#[cfg(not(feature = "remote"))]
#[path = "web_tree/remote_off.rs"]
mod remote;
#[cfg(feature = "remote")]
mod reservoir;
mod speed;
mod visibility;

/// Join a host over WebRTC as the tree's remote session (`?signal=`).
pub(crate) fn connect_remote(signal_url: String, invite: Option<String>) -> Result<(), String> {
    remote::connect(signal_url, invite)
}
use lane::TreeLane;

fn forme_toolbar(page: &TreePage) -> Child {
    #[cfg(feature = "product")]
    return forme::toolbar(page);
    #[cfg(not(feature = "product"))]
    {
        let _ = page;
        Box::new(el("span", ()))
    }
}
fn forme_workbench(page: &TreePage) -> Option<Child> {
    #[cfg(feature = "product")]
    return forme::workbench(page);
    #[cfg(not(feature = "product"))]
    {
        let _ = page;
        None
    }
}
fn forme_sheet() -> String {
    #[cfg(feature = "product")]
    return format!("{}{}", cambium::FRISKET_CSS, forme::SHEET);
    #[cfg(not(feature = "product"))]
    String::new()
}

mod responsive_links;
