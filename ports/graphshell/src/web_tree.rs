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
    AnyView, GenetCtx, GenetElement, PointerClick, WheelEvent, custom_leaf, el, on_click, on_wheel,
};
use cambium_genet_web_host::{WebCapture, WebWindow, mount};
use cambium_rootstock::{
    AppCtx, CloseDisposition, Frame, HostFont, HostHooks, HostOptions, HostPointer, HostWindow,
    Init, Key, KeyPress, NamedKey, ProducedTexture, ProducerContext, SourceAlpha, SourceEncoding,
    TextureProducer,
};
use graphshell::app::GraphshellApp;
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
use super::web_timing::FrameTiming;

/// The canvas's custom-leaf key, shared by the view and its producer.
const CANVAS_KEY: u64 = 1;
/// The tile cache the canvas's scene rasterizes under, apart from the
/// document's own.
const CANVAS_RASTER: u64 = 0x7472_6565;
/// How far an arrow key pans and plus or minus zooms, as the main page does.
const PAN_STEP: f32 = 42.0;
const ZOOM_STEP: f32 = 40.0;

const SHEET: &str = "\
    :root { font-family: Roboto, sans-serif; font-size: 14px; color: #dce3e8; background: #070c0f; } \
    main { display: flex; flex-direction: column; } \
    h1 { font-size: 16px; margin: 8px 12px 0; } \
    .tree-status { margin: 4px 12px 8px; } \
    .tree-canvas { display: block; flex: 1 1 auto; min-height: 0; }";

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
        if shared.gpu.borrow().is_none() {
            *shared.gpu.borrow_mut() = Some((cx.device.clone(), cx.queue.clone()));
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
        let (scene, moving) = canvas.frame(size.0, size.1);
        shared.moving.set(moving);
        shared.dirty.set(false);
        // Rasterize every requested frame, as the presenter does. A settled
        // layout does not prove the first texture was complete: Vello can need
        // a later frame after its asynchronous buffer-size readback.
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
        Some(ProducedTexture {
            view,
            generation: self.generation,
            alpha: SourceAlpha::Straight,
            encoding: SourceEncoding::Srgb,
        })
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

    fn pick(&mut self, local: (f32, f32)) {
        let mut canvas = self.shared.canvas.borrow_mut();
        let hit = canvas.node_at_screen(local.0, local.1);
        self.picked = hit.and_then(|id| {
            canvas
                .graph()
                .get_node_by_id(id)
                .map(|(_, node)| node.url().to_string())
        });
        if let Some(id) = hit {
            canvas.select_member(id);
        }
        self.shared.dirty.set(true);
    }

    fn wheel(&mut self, wheel: WheelEvent) {
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
    Box::new(
        el(
            "main",
            (
                el("h1", "Graphshell, one tree"),
                el("p", page.status())
                    .attr("class", "tree-status")
                    .attr("role", "status"),
                on_wheel(
                    on_click(
                        custom_leaf::<TreePage, ()>(CANVAS_KEY, width, 1)
                            .attr("class", "tree-canvas")
                            .attr("role", "img")
                            .attr("aria-label", "Graph"),
                        |page: &mut TreePage, click: PointerClick| page.pick(click.local),
                    ),
                    |page: &mut TreePage, wheel: WheelEvent| page.wheel(wheel),
                ),
            ),
        )
        .attr("style", format!("width:{width}px;height:{height}px;")),
    )
}

/// Arrows pan and plus or minus zoom, as on the main page.
fn keys(shared: &Shared, press: &KeyPress) -> bool {
    let mut canvas = shared.canvas.borrow_mut();
    match &press.key {
        Key::Named(NamedKey::ArrowLeft) => canvas.wheel(-PAN_STEP, 0.0),
        Key::Named(NamedKey::ArrowRight) => canvas.wheel(PAN_STEP, 0.0),
        Key::Named(NamedKey::ArrowUp) => canvas.wheel(0.0, -PAN_STEP),
        Key::Named(NamedKey::ArrowDown) => canvas.wheel(0.0, PAN_STEP),
        Key::Character(text) if text == "+" || text == "=" => zoom(&mut canvas, shared, ZOOM_STEP),
        Key::Character(text) if text == "-" => zoom(&mut canvas, shared, -ZOOM_STEP),
        _ => return false,
    };
    shared.dirty.set(true);
    true
}

fn zoom(canvas: &mut Canvas, shared: &Shared, delta: f32) -> bool {
    let (width, height) = shared.size.get();
    canvas.cursor_moved(width as f32 * 0.5, height as f32 * 0.5);
    canvas.set_ctrl(true);
    let zoomed = canvas.wheel(0.0, delta);
    canvas.set_ctrl(false);
    zoomed
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

    let (graph, source) = match web_graphs::requested() {
        Some((nodes, seed)) => (
            web_graphs::generated(nodes, seed),
            format!("generated, seed {seed}"),
        ),
        None => (fixture_graph()?, "fixture".to_string()),
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
    });
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
    let mounted = mount(
        canvas,
        options,
        move |_window, _commands, _wake| Init {
            state: TreePage {
                shared: page_shared,
                source,
                nodes,
                picked: None,
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
        focused_text: Box::new(|_runner| None),
        key_intercept: Box::new(move |_runner, press| keys(&shared, press)),
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
fn publish(ok: bool, text: &str, shared: &Shared) {
    let result = if ok { "ok" } else { "fail" };
    // The sink writes its own RESULT line from `result`.
    let log: Vec<&str> = text.lines().skip(1).collect();
    let report = json!({
        "result": result,
        "log": log,
        "timings": shared.timing.borrow().json(),
        "canvas_logical_size": shared.size.get(),
        "canvas_physical_size": shared.physical_size.get(),
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

mod lane;
use lane::TreeLane;
