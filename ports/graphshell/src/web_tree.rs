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
//! the node under it, the wheel pans, and the arrow keys pan and plus and
//! minus zoom. `tree.html` mounts it with [`mount_tree`]; `nodes` and `seed`
//! in the URL swap the fixture graph for a generated one.
//!
//! The page drives itself through Mesquite's scenario lane with the web
//! host's [`WebCapture`], and times its frames with the same
//! [`FrameTiming`] as the main page, so the two are measured alike.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cambium::{
    AnyView, GenetCtx, GenetElement, Key, NamedKey, WheelEvent, custom_leaf, el, on_key,
    on_pointer, on_wheel,
};
use cambium_genet_web_host::{WebCapture, WebWindow, mount};
use cambium_rootstock::{
    AppCtx, CloseDisposition, Frame, HostFont, HostHooks, HostOptions, HostPointer, HostWindow,
    Init, ProducedTexture, ProducerContext, SourceAlpha, SourceEncoding, TextureProducer,
};
use graphshell::app::GraphshellApp;
use graphshell::canvas_controls::CanvasCommand;
use graphshell::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
use mere::canvas::Canvas;
use mere::kernel::graph::Graph;
use mesquite::{CaptureRecord, Lane, Product};
use muniment::MemoryBackend;
use netrender::{ColorLoad, NetrenderOptions};
use serde_json::json;
use taproot::ProbeSnapshot;
use taproot::Selector;
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlCanvasElement};

use super::web_gpu::SHELL_CLEAR;
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
    :root { font-family: Roboto, sans-serif; font-size: 14px; color: #dce3e8; background: #070c0f; } \
    main { display: flex; flex-direction: column; } \
    h1 { font-size: 16px; margin: 8px 12px 0; } \
    .tree-status { margin: 4px 12px 8px; } \
    .tree-controls { display:flex; gap:6px; padding:6px 12px; flex-wrap:wrap; } \
    .tree-controls button { background:#263640; color:#dce3e8; padding:5px 10px; border:1px solid #637581; } \
    .tree-body { display:flex; flex-direction:row; flex:1 1 auto; min-height:0; position:relative; }     .tools-overlay { position:absolute; top:0; right:0; bottom:0; z-index:10; }     .tools-storage { margin:2px 0 6px; color:#9fb0bb; font-size:12px; } \
    .tree-graph { display:flex; flex-direction:column; flex:1 1 auto; min-width:0; } \
    .tree-canvas { display: block; flex: 1 1 auto; min-height: 0; } \
    .tree-tools { flex:0 0 auto; width:279px; padding:4px 10px; background:#0d161b; border-left:1px solid #2c3b44; overflow-y:auto; min-height:0; } \
    .tree-tools .disclosure-trigger { display:block; width:100%; text-align:left; font-size:14px; background:transparent; border:none; padding:2px 0 4px; margin:0; } \
    .tools-cards { margin:2px 0 4px; padding:0 0 0 16px; font-size:12px; color:#dce3e8; } \
    .tree-tools h2 { font-size:14px; margin:2px 0 4px; } \
    .tree-tools h3 { font-size:13px; margin:4px 0 2px; } \
    .tools-section + .tools-section { margin-top:10px; border-top:1px solid #2c3b44; padding-top:4px; } \
    .tools-switch button[aria-pressed=true] { background:#2d5a63; border-color:#79a9be; } \
    .tools-active { margin:4px 0; color:#f0dfb8; font-size:12px; } \
    .tree-tools label { display:block; margin:3px 0; } \
    .tree-tools .tools-caption { display:block; color:#9fb0bb; font-size:12px; } \
    .tree-tools button { background:#263640; color:#dce3e8; padding:3px 10px; margin:2px 0 4px; border:1px solid #637581; } \
    .tools-overlays { display:flex; flex-wrap:wrap; margin:2px 0; } \
    .tools-overlays label { width:136px; margin:1px 0; } \
    .tools-overlays label.disabled { opacity:.45; } \
    .tools-note { margin:2px 0 4px; color:#c9b27c; font-size:12px; } \
    .tools-status { margin:4px 0; color:#9fb0bb; font-size:12px; } \
    .select-box { background:#263640; border:1px solid #637581; padding:2px 8px; } \
    .select-list { background:#17232b; border:1px solid #637581; z-index:20; width:262px; } \
    .select-option { padding:1px 8px; } \
    .tree-product { position:absolute;top:112px;left:12px;max-width:360px;z-index:5; } \
    .tree-product p { margin:4px 0; } \
    .tree-detail { background:#17232b;border:1px solid #637581;padding:12px; } \
    .tree-detail label { display:block;margin:8px 0; } \
    .tree-detail .detail-key { display:block; } \
    .tree-detail .detail-value { display:block;margin:2px 0 8px;overflow-wrap:anywhere; } \
    .tree-detail input { display:block;width:280px;height:26px;color:#dce3e8;background:#263640;border:1px solid #637581; } \
    .tree-product button { background:#263640;color:#dce3e8;padding:5px 10px;border:1px solid #637581; }";

/// What the page, its producer and its hooks share.
struct Shared {
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
    gpu_options: controls::GpuOptions,
    /// The page's device for the canvas's and the board's repulsion, built
    /// once from the host's render core on the producer's first frame.
    physics_device: RefCell<Option<mere::canvas::PhysicsDevice>>,
    visibility: Option<RefCell<visibility::Visibility>>,
    /// A released drag's drop point, canvas-local px, until the first frame
    /// that executes a physics step after the release.
    release_watch: Cell<Option<(f32, f32)>>,
    /// The focused node's distance from that drop point on that frame, and
    /// how many physics steps the frame executed.
    release_step: Cell<Option<(f32, u32)>>,
    /// Where the last scripted press landed, canvas-local px, for the receipt.
    press_point: Cell<Option<(f32, f32)>>,
    /// Every recorded release, for the receipt.
    release_log: RefCell<Vec<String>>,
    /// Lines `log-physics` recorded, for the receipt.
    physics_log: RefCell<Vec<String>>,
    /// The remote session (`remote::TreeRemote`); its channel's pumps reach
    /// it from outside the runner.
    remote: Rc<RefCell<remote::TreeRemote>>,
    /// Whether the canvas leaf shows the remote board rather than the graph.
    remote_shown: Cell<bool>,
}

impl Shared {
    fn with_gpu<R>(&self, f: impl FnOnce(Option<(&wgpu::Device, &wgpu::Queue)>) -> R) -> R {
        let gpu = self.gpu.borrow();
        f(gpu.as_ref().map(|(device, queue)| (device, queue)))
    }
}

/// Pictograph's canvas as a texture producer.
struct CanvasProducer {
    shared: Rc<Shared>,
    generation: u64,
    /// The last frame's texture, kept alive while the host samples its view.
    texture: Option<wgpu::Texture>,
}

impl TextureProducer for CanvasProducer {
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        let shared = &self.shared;
        let visibility_before = visibility::before(shared);
        if shared.gpu.borrow().is_none() {
            *shared.gpu.borrow_mut() = Some((cx.device.clone(), cx.queue.clone()));
            let options = shared.gpu_options;
            if options.enabled {
                // The host's own device, never one of ours: the render core's
                // handles are the ones every producer on this page draws with.
                let device = mere::canvas::physics_device_for(&cx.core.renderer().wgpu_device.core)
                    .with_threshold(options.threshold)
                    .with_max_stale_steps(options.max_stale_steps);
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
        let (width, height) = cx.frame.logical_size;
        let size = (
            width.round().max(1.0) as u32,
            height.round().max(1.0) as u32,
        );
        let mut canvas = shared.canvas.borrow_mut();
        if size != shared.size.get() {
            canvas.resize(size.0, size.1);
            shared.size.set(size);
        }
        let profile = shared.timing.borrow().active();
        if shared.remote_shown.get() {
            // One leaf, the producer picks the scene: the board, mirroring
            // the canvas's law, ticked once a frame and drawn from its bodies.
            let choice = canvas.physics_choice();
            drop(canvas);
            let scene = {
                let mut remote = shared.remote.borrow_mut();
                remote.sync_board(choice);
                remote.board.tick();
                let remote = &mut *remote;
                let empty = mere::canvas::BoardScene::default();
                let painted = if remote.mounted().is_some() {
                    remote.board.scene()
                } else {
                    &empty
                };
                painted.paint_titled(
                    remote.board.board(),
                    size.0,
                    size.1,
                    remote::BOARD_FIT,
                    &mut remote.text,
                )
            };
            shared.dirty.set(false);
            let [physical_width, physical_height] = cx.frame.physical_size;
            shared.physical_size.set(cx.frame.physical_size);
            let (texture, view) = cx.core.rasterize_scaled_for(
                CANVAS_RASTER,
                &scene,
                physical_width,
                physical_height,
                ColorLoad::Clear(SHELL_CLEAR),
                cx.frame.layout_scale,
            );
            self.texture = Some(texture);
            self.generation += 1;
            return Some(ProducedTexture {
                view,
                generation: self.generation,
                alpha: SourceAlpha::Straight,
                encoding: SourceEncoding::Srgb,
            });
        }
        let (scene, moving) = if profile {
            let (scene, moving, sample) = match cx.frame.timestamp {
                Some(timestamp) => canvas.frame_profiled_at(
                    size.0,
                    size.1,
                    timestamp,
                    shared.physics_config,
                    now_ms,
                ),
                None => canvas.frame_profiled(size.0, size.1, now_ms),
            };
            shared.timing.borrow_mut().stages(
                mere::canvas::CanvasFrameProfile::STAGES
                    .into_iter()
                    .zip(sample.stage_ms),
            );
            shared.timing.borrow_mut().counts([
                ("total_nodes", sample.total_nodes),
                ("visible_nodes", sample.visible_nodes),
                ("paint_before_cull", sample.paint_commands_before_cull),
                ("paint_after_cull", sample.paint_commands_after_cull),
            ]);
            (scene, moving)
        } else {
            match cx.frame.timestamp {
                Some(timestamp) => {
                    canvas.frame_at(size.0, size.1, timestamp, shared.physics_config)
                },
                None => canvas.frame(size.0, size.1),
            }
        };
        if profile && let Some(report) = canvas.elapsed_step_report() {
            shared.timing.borrow_mut().counts([
                ("physics_steps", report.steps as usize),
                (
                    "physics_discarded_us",
                    report.discarded_elapsed.as_micros().min(usize::MAX as u128) as usize,
                ),
                (
                    "physics_carried_us",
                    report.carried_elapsed.as_micros() as usize,
                ),
            ]);
        }
        shared.moving.set(moving);
        if let Some((x, y)) = shared.release_watch.get()
            && cx.frame.timestamp.is_some()
            && canvas.dragging_node().is_none()
            && let Some(report) = canvas.elapsed_step_report()
            && report.steps > 0
        {
            let focused = canvas.focused_screen_position();
            let distance = focused.map_or(f32::NAN, |(fx, fy)| (fx - x).hypot(fy - y));
            let from_press = focused
                .zip(shared.press_point.get())
                .map_or(f32::NAN, |((fx, fy), (px, py))| (fx - px).hypot(fy - py));
            shared.release_step.set(Some((distance, report.steps)));
            shared.release_log.borrow_mut().push(format!(
                "release-step {} {distance:.1} px after {} steps ({from_press:.1} px from the press point, zoom {:.2})",
                canvas.physics_law().id(),
                report.steps,
                canvas.camera().zoom
            ));
            shared.release_watch.set(None);
        }
        visibility::after(shared, &canvas, visibility_before, cx.frame.timestamp);
        shared.dirty.set(false);
        // Rasterize every requested frame, as the presenter does. A settled
        // layout does not prove the first texture was complete: Vello can need
        // a later frame after its asynchronous buffer-size readback.
        let [physical_width, physical_height] = cx.frame.physical_size;
        shared.physical_size.set(cx.frame.physical_size);
        let raster_start = profile.then(now_ms);
        let (texture, view) = cx.core.rasterize_scaled_for(
            CANVAS_RASTER,
            &scene,
            physical_width,
            physical_height,
            ColorLoad::Clear(SHELL_CLEAR),
            cx.frame.layout_scale,
        );
        if let Some(start) = raster_start {
            shared
                .timing
                .borrow_mut()
                .stages([("rasterize", now_ms() - start)]);
        }
        self.texture = Some(texture);
        self.generation += 1;
        Some(ProducedTexture {
            view,
            generation: self.generation,
            alpha: SourceAlpha::Straight,
            encoding: SourceEncoding::Srgb,
        })
    }
    fn suspend(&mut self) {
        self.shared.canvas.borrow_mut().reset_frame_time();
        self.texture = None;
    }

    /// While the board is shown, the slot is a list of its cards, each named
    /// by its title and placed where it is painted. The graph keeps the
    /// slot's own DOM semantics for now.
    fn semantics(&mut self) -> Option<cambium_rootstock::ProducerSemantics> {
        if !self.shared.remote_shown.get() {
            return None;
        }
        let (width, height) = self.shared.size.get();
        Some(remote::board_semantics(
            &self.shared.remote.borrow(),
            width,
            height,
        ))
    }
}

/// The application state the tree renders.
pub(crate) struct TreePage {
    shared: Rc<Shared>,
    /// Where the graph came from, for the status line and receipts.
    source: String,
    nodes: usize,
    /// The address of the node the last click picked.
    picked: Option<String>,
    product: Option<product::SavedProduct>,
    /// The "Graph tools" arrangement and physics section.
    physics: physics::PhysicsPanel,
    /// Which session the canvas leaf shows.
    session: remote::Session,
    /// Which Graph tools sections are open.
    sections: remote::Sections,
    /// The open remote draft's field selects.
    draft: remote::DraftControls,
    /// The remote generation the view last rebuilt for.
    remote_seen: u64,
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

impl TreePage {
    fn status(&self) -> String {
        format!(
            "{} · {} nodes · picked: {}",
            self.source,
            self.nodes,
            self.picked.as_deref().unwrap_or("none")
        )
    }

    fn wheel(&mut self, wheel: WheelEvent) {
        wheel.prevent_default();
        let mut canvas = self.shared.canvas.borrow_mut();
        canvas.cursor_moved(wheel.local.0, wheel.local.1);
        canvas.wheel(wheel.delta.0, wheel.delta.1);
        self.shared.dirty.set(true);
    }
}

type Child = Box<dyn AnyView<TreePage, (), GenetCtx, GenetElement>>;
type Logic = fn(&TreePage) -> Child;

fn view(page: &TreePage) -> Child {
    let (width, height) = page.size;
    // Genet does not stretch a custom leaf across its cross axis, so the
    // canvas column is told the width the tools region leaves it.
    let docked = tools_docked(page);
    let canvas_width = if docked {
        width.saturating_sub(TOOLS_WIDTH).max(1)
    } else {
        width.max(1)
    };
    let graph = on_wheel(
        on_key(
            on_pointer(
                custom_leaf::<TreePage, ()>(CANVAS_KEY, canvas_width, 1)
                    .attr("class", "tree-canvas")
                    .attr("role", "img")
                    .attr("aria-label", "Graph"),
                |page: &mut TreePage, event: cambium::PointerEvent| page.pointer(event),
            ),
            |page: &mut TreePage, event: cambium::KeyEvent| {
                if keys(page, &event.key) {
                    event.prevent_default();
                }
            },
        ),
        |page: &mut TreePage, wheel: WheelEvent| page.wheel(wheel),
    );
    Box::new(
        el(
            "main",
            (
                el("h1", "Graphshell, one tree"),
                el("p", page.status())
                    .attr("class", "tree-status")
                    .attr("role", "status"),
                controls::toolbar(page),
                product::controls(page),
                el(
                    "div",
                    (
                        el("div", graph).attr("class", "tree-graph"),
                        if docked {
                            Some(tools_region(page))
                        } else {
                            page.tools_open.then(|| {
                                Box::new(
                                    el("div", tools_region(page)).attr("class", "tools-overlay"),
                                ) as Child
                            })
                        },
                    ),
                )
                .attr("class", "tree-body"),
            ),
        )
        .attr("style", format!("width:{width}px;height:{height}px;")),
    )
}

/// Whether the Graph tools region docks beside the canvas at this width.
fn tools_docked(page: &TreePage) -> bool {
    page.size.0 >= TOOLS_DOCK_MIN_WIDTH
}

/// The Graph tools region: the storage line on `app=local`, then its sections.
fn tools_region(page: &TreePage) -> Child {
    let mut children: Vec<Child> = Vec::new();
    if let Some(product) = &page.product {
        children.push(Box::new(
            el("p", format!("Storage: {}", product.storage))
                .attr("class", "tools-storage")
                .attr("role", "status"),
        ));
    }
    children.push(physics::section(page));
    children.push(remote::section(page));
    Box::new(
        el("aside", children)
            .attr("class", "tree-tools")
            .attr("aria-label", "Graph tools"),
    )
}

/// Arrows pan and plus or minus zoom, as on the main page.
fn keys(page: &mut TreePage, key: &Key) -> bool {
    if let Some(product) = &mut page.product {
        match key {
            Key::Named(NamedKey::Enter) if product.selected.is_some() => {
                product.detail_open = true;
                return true;
            },
            Key::Named(NamedKey::Escape) if product.detail_open => {
                product.detail_open = false;
                return true;
            },
            _ => {},
        }
    }
    let shared = &page.shared;
    let command = match key {
        Key::Named(NamedKey::ArrowLeft) => CanvasCommand::Pan {
            dx: -PAN_STEP,
            dy: 0.0,
        },
        Key::Named(NamedKey::ArrowRight) => CanvasCommand::Pan {
            dx: PAN_STEP,
            dy: 0.0,
        },
        Key::Named(NamedKey::ArrowUp) => CanvasCommand::Pan {
            dx: 0.0,
            dy: -PAN_STEP,
        },
        Key::Named(NamedKey::ArrowDown) => CanvasCommand::Pan {
            dx: 0.0,
            dy: PAN_STEP,
        },
        Key::Character(text) if text == "+" || text == "=" => {
            CanvasCommand::Zoom { delta: ZOOM_STEP }
        },
        Key::Character(text) if text == "-" => CanvasCommand::Zoom { delta: -ZOOM_STEP },
        _ => return false,
    };
    command.apply(&mut shared.canvas.borrow_mut(), shared.size.get());
    shared.dirty.set(true);
    true
}

/// The mounted page, for the scenario entry that reaches it from JavaScript.
struct Tree {
    shared: Rc<Shared>,
    window: WebWindow,
    lane: Option<Lane<TreeLane>>,
}

thread_local! {
    static TREE: RefCell<Option<Tree>> = const { RefCell::new(None) };
}

/// Whether the tree page is mounted, rather than the main page.
pub(crate) fn mounted() -> bool {
    TREE.with(|tree| tree.borrow().is_some())
}

/// Mount the one-tree page into `root`.
#[wasm_bindgen]
pub fn mount_tree(root: Element) -> Result<(), JsValue> {
    if mounted() || web_scenario::host().is_some() {
        return Err(JsValue::from_str(
            "Graphshell is already mounted on this page",
        ));
    }
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = boot(root).await {
            web_sys::console::error_1(&error.clone().into());
            if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                document.set_title(&format!("GRAPHSHELL TREE FAIL: {error}"));
            }
        }
    });
    Ok(())
}

async fn boot(root: Element) -> Result<(), String> {
    let document = root.owner_document().ok_or("the root has no document")?;
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "could not create the canvas")?
        .dyn_into()
        .map_err(|_| "the canvas is not a canvas")?;
    canvas.set_id("graphshell-canvas");
    canvas
        .set_attribute("style", "display:block;width:100%;height:100%;")
        .map_err(|_| "could not size the canvas")?;
    root.append_child(&canvas)
        .map_err(|_| "could not place the canvas")?;
    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;

    let product = product::open().await?;
    let (graph, source) = if let Some(product) = &product {
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
    let shared = Rc::new(Shared {
        canvas: RefCell::new(web_graphs::prepared_canvas(graph, width, height)),
        dirty: Cell::new(true),
        moving: Cell::new(false),
        size: Cell::new((width, height)),
        physical_size: Cell::new([0, 0]),
        gpu: RefCell::new(None),
        timing: RefCell::new(FrameTiming::default()),
        physics_config: controls::physics_config()?,
        gpu_options: controls::gpu_options()?,
        physics_device: RefCell::new(None),
        visibility: visibility::requested()?,
        release_watch: Cell::new(None),
        release_step: Cell::new(None),
        press_point: Cell::new(None),
        release_log: RefCell::new(Vec::new()),
        physics_log: RefCell::new(Vec::new()),
        remote: Rc::new(RefCell::new(remote::TreeRemote::new())),
        remote_shown: Cell::new(false),
    });
    visibility::install(&shared, &document)?;
    let options = HostOptions {
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
    let physics = physics::PhysicsPanel::new(&shared.canvas.borrow(), web_graphs::LAYOUT);
    let mounted = mount(
        canvas,
        options,
        move |_window, _commands, _wake| Init {
            state: TreePage {
                shared: page_shared,
                source,
                nodes,
                picked: None,
                product,
                physics,
                tools_open: false,
                session: remote::Session::Local,
                sections: remote::Sections::default(),
                draft: remote::DraftControls::default(),
                remote_seen: 0,
                remote_link_seen: false,
                size: (width, height),
            },
            logic: view as Logic,
            sheet: SHEET.to_string(),
            // A browser lends genet no system faces, so the page brings one.
            fonts: vec![HostFont {
                family: None,
                bytes: include_bytes!("../web/GraphshellSans.ttf").to_vec(),
            }],
            images: Vec::new(),
        },
        hooks(shared.clone()),
    )
    .await?;
    TREE.with(|tree| {
        *tree.borrow_mut() = Some(Tree {
            shared,
            window: mounted.window().clone(),
            lane: None,
        })
    });
    if root.has_attribute("data-owns-title") {
        document.set_title("Graphshell, one tree");
    }
    root.set_attribute("data-ready", "true")
        .map_err(|_| "could not mark the page ready")?;
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
            if ctx
                .runner
                .state()
                .product
                .as_ref()
                .is_some_and(|product| product.ready())
            {
                ctx.runner.update(|page| {
                    if let Some(product) = &mut page.product {
                        product.poll(&mut frame_shared.canvas.borrow_mut());
                    }
                });
                frame_shared.dirty.set(true);
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
                    // The section starts closed and opens when a link exists.
                    if linked && !page.remote_link_seen {
                        page.remote_link_seen = true;
                        page.sections.remote.expanded = true;
                    }
                });
            }
            if ctx.runner.state().physics.transition.is_some() {
                ctx.runner.update(|page| page.advance_arrangement(now_ms()));
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
                    texture: None,
                };
                if let Err(error) = ctx.producers.register(CANVAS_KEY, producer, &[]) {
                    web_sys::console::error_1(
                        &format!("could not register the canvas producer: {error:?}").into(),
                    );
                }
            }
            frame_shared.with_gpu(|gpu| frame_shared.timing.borrow_mut().frame_begin(gpu));
            // This comparison page continuously renders, like GpuPresenter.
            // It also keeps asynchronous Vello recovery alive when the graph
            // is settled. An event-driven host needs a renderer completion
            // signal before it can safely cache its last texture.
            true
        }),
        after_dispatch: Box::new(|_ctx| {}),
        after_frame: Box::new(move |ctx| {
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
        focused_text: Box::new(product::focused_text),
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

mod controls;
mod lane;
mod physics;
mod product;
mod remote;
mod visibility;

/// Join a host over WebRTC as the tree's remote session (`?signal=`).
pub(crate) fn connect_remote(signal_url: String, invite: Option<String>) -> Result<(), String> {
    remote::connect(signal_url, invite)
}
use lane::TreeLane;
