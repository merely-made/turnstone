// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Lens windows: a second window as a pane host over the one app state.
//!
//! A lens owns its own frisket space; each graph pane owns its camera and
//! selection, so panes torn out of the primary keep their
//! retained runners and their identity. Arrangement rides the SESSION, so
//! adopting a session closes these and reopens that session's own.

use super::NodeSessions;
use std::sync::Arc;

use genet_winit_host::SurfaceHost;
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, Scene};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::action::Action;
use crate::panes::{PaneContent, PaneId};
use crate::surface::{Rect, SurfaceKind};

use inker::SessionClick;

use super::input::pointer_button;
use super::{CompositeLayer, PlannedLayer, PlannedScene, Shell, capture_composed};

/// One lens window's record: its platform window, present stack, size,
/// cursor, and graph gesture capture.
pub(super) struct LensWindow {
    pub(super) window: Arc<Window>,
    pub(super) host: Option<SurfaceHost>,
    pub(super) recovery: super::WindowRecovery,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) cursor: (f32, f32),
    /// The graph pane holding a pointer gesture (grab/pan), if any.
    pub(super) pointer_graph: Option<PaneId>,
    /// Which `App::lenses` pane space this window shows (stable; the space
    /// tombstones on close).
    pub(super) ordinal: usize,
    pub(super) focused: bool,
    pub(super) a11y: genet_winit_host::AccessKitBridge,
    pub(super) a11y_actions: super::a11y_bridge::ActionQueue,
    pub(super) a11y_epoch: Option<u64>,
    pub(super) a11y_forest: crate::foreign_a11y::ForeignA11y,
}

/// The actual Lens press resolver before Pane/Content/Divider dispatch.
/// Current same-window custody precedes every fresh hit-kind branch.
pub(super) fn lens_press_hit(
    pool: &super::window_input::Pool,
    window: u64,
    plan: &[crate::surface::Surface],
    focus: crate::surface::FocusTarget,
    position: inker::PhysicalPosition,
) -> Option<crate::surface::HitResult> {
    super::window_input::captured_content_hit(pool, window, position)
        .or_else(|| crate::surface::hit_test(plan, focus, position.x, position.y))
}

/// Platform-independent Lens pane custody, before fresh surface selection.
/// App plans and renderer generation validation remain owned by the host.
pub(super) trait LensPanePointerHost {
    fn lens_pane_admission(&self) -> Option<super::input::ContributedPointer>;
    fn lens_pane_admission_current(&self, admitted: super::input::ContributedPointer) -> bool;
    fn lens_cancel_stale_admission(&mut self);
    fn lens_release_admission(&mut self, window: u64, x: f32, y: f32);
}

pub(super) fn reject_lens_pane_press(host: &mut impl LensPanePointerHost) -> bool {
    let Some(admitted) = host.lens_pane_admission() else { return false; };
    // One physical mouse gesture cannot admit a competing page or pane Down,
    // including a new press arriving in another native window.
    if !host.lens_pane_admission_current(admitted) {
        // Existing stale-withdrawal policy is distinct from valid custody.
        host.lens_cancel_stale_admission();
    }
    true
}

pub(super) fn dispatch_lens_pane_release(
    host: &mut impl LensPanePointerHost,
    window: u64,
    position: (f32, f32),
    button: MouseButton,
) -> bool {
    // Existing correct terminal rule: every other button is swallowed, only
    // the host's same-window admitted Left can release this receiver.
    if host.lens_pane_admission().is_none() { return false; }
    if button == MouseButton::Left {
        host.lens_release_admission(window, position.0, position.1);
    }
    true
}

impl LensPanePointerHost for Shell {
    fn lens_pane_admission(&self) -> Option<super::input::ContributedPointer> {
        self.contributed_pointer
    }
    fn lens_pane_admission_current(&self, admitted: super::input::ContributedPointer) -> bool {
        self.contributed_pointer_is_current(admitted)
    }
    fn lens_cancel_stale_admission(&mut self) {
        self.cancel_content_pointer();
    }
    fn lens_release_admission(&mut self, window: u64, x: f32, y: f32) {
        self.release_contributed_pointer(window, x, y);
    }
}

impl Shell {
    /// Create any requested lens windows (rung 7). Called from the event
    /// handlers, where an `ActiveEventLoop` is in scope. Its graph panes
    /// install their own view state on their first render/input pass.
    pub(super) fn drain_pending_windows(&mut self, event_loop: &ActiveEventLoop) {
        if !self.poll_render_recovery() { return; }
        while let Some(ordinal) = self.pending_windows.pop() {
            let Some(primary) = self.host.as_ref() else {
                tracing::warn!("lens requires the primary render core");
                continue;
            };
            let shared_core = primary.shared_core();
            let attributes = Window::default_attributes()
                .with_title("Turnstone — lens")
                .with_visible(false)
                .with_inner_size(PhysicalSize::new(800u32, 600u32));
            let Ok(window) = event_loop.create_window(attributes) else {
                tracing::warn!("lens window creation failed");
                continue;
            };
            let window = Arc::new(window);
            let size = window.inner_size();
            match SurfaceHost::from_shared_core(
                window.clone(),
                size.width.max(1),
                size.height.max(1),
                shared_core.clone(),
            ) {
                Ok(host) => {
                    debug_assert!(Arc::ptr_eq(&shared_core, &host.shared_core()));
                    let a11y_actions = Arc::new(std::sync::Mutex::new(Default::default()));
                    let a11y = super::a11y_bridge::install(
                        &window,
                        a11y_actions.clone(),
                        self.proxy.clone(),
                        &format!("Turnstone lens {ordinal}"),
                    );
                    window.set_visible(true);
                    window.request_redraw();
                    let owner = self.render_recovery.as_ref().expect("primary recovery owner").clone();
                    let proxy = self.proxy.clone();
                    let recovery = super::WindowRecovery::new(&owner, Arc::new(move || {
                        let _ = proxy.send_event(());
                    }));
                    self.lens_windows.insert(
                        window.id(),
                        LensWindow {
                            window,
                            host: Some(host),
                            recovery,
                            width: size.width.max(1),
                            height: size.height.max(1),
                            cursor: (0.0, 0.0),
                            pointer_graph: None,
                            ordinal,
                            focused: true,
                            a11y,
                            a11y_actions,
                            a11y_epoch: None,
                            a11y_forest: crate::foreign_a11y::ForeignA11y::new(),
                        },
                    );
                },
                Err(err) => tracing::warn!(%err, "lens surface boot failed"),
            }
        }
        self.app.window_count = 1 + self.lens_windows.len();
    }

    pub(super) fn push_lens_a11y_trees(&mut self) {
        let mut windows: Vec<_> = self
            .lens_windows
            .iter()
            .map(|(id, lens)| (*id, lens.ordinal, lens.width, lens.height))
            .collect();
        windows.sort_by_key(|(_, ordinal, _, _)| *ordinal);
        for (id, ordinal, width, height) in windows {
            let plan = self.lens_plan(ordinal, width, height);
            let pane_rects = plan
                .iter()
                .filter_map(|surface| match surface.kind {
                    SurfaceKind::Pane(pane) => Some((pane, surface.rect)),
                    _ => None,
                })
                .collect();
            let focused_pane = match self.app.focus {
                crate::surface::FocusTarget::Pane(pane) => Some(pane),
                _ => None,
            };
            let contributed = crate::contributed_a11y::project(
                &self.renderers.contributed,
                &pane_rects,
                focused_pane,
            );
            let root = accesskit::NodeId(0);
            let mut node = accesskit::Node::new(accesskit::Role::Window);
            node.set_label(format!("Turnstone lens {ordinal}"));
            node.set_bounds(accesskit::Rect::new(0.0, 0.0, width as f64, height as f64));
            let mut trees: Vec<_> = contributed.trees.into_iter().collect();
            trees.sort_by_key(|(pane, _)| pane.0);
            node.set_children(trees.iter().map(|(_, tree)| tree.root).collect::<Vec<_>>());
            let mut nodes = vec![(root, node)];
            for (_, tree) in trees {
                nodes.extend(tree.nodes);
            }
            let mut frame = super::document_a11y::Projected {
                tree: uxtree::UxTree { root, nodes },
                routes: Default::default(),
                focus: contributed.focus,
                guests: contributed.guests,
                admissions: contributed
                    .routes
                    .into_iter()
                    .map(|(id, route)| (id, super::a11y_bridge::Admission::Contributed(route)))
                    .collect(),
            };
            self.project_document_accessibility(&mut frame, &plan);
            let foreign_placements = self.foreign_a11y_placements_in(&plan);
            let Some(lens) = self.lens_windows.get_mut(&id) else {
                continue;
            };
            lens.a11y_forest.sync_retained_from(&self.surface_a11y);
            let epoch = lens.a11y.activation_epoch();
            if lens.a11y_epoch != Some(epoch) {
                lens.a11y_forest.reset_publication();
                lens.a11y_actions
                    .lock()
                    .expect("lens a11y queue poisoned")
                    .publish(Default::default());
                lens.a11y_epoch = Some(epoch);
            }
            let visible = append_foreign_lens(
                &mut frame,
                &lens.a11y_forest,
                foreign_placements,
                self.app.focus,
            );
            let update = frame.tree.to_tree_update(frame.focus);
            let initial = super::a11y_bridge::safe_initial(&update);
            let updates = match lens
                .a11y_forest
                .frame_with_guests(update, &visible, &frame.guests)
            {
                Ok(updates) => updates,
                Err(error) => {
                    tracing::warn!(%error, "lens accessibility composition refused");
                    continue;
                },
            };
            let served = lens
                .a11y
                .update_batch_at(epoch, initial, updates)
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "lens accessibility publication refused");
                    false
                });
            lens.a11y_actions
                .lock()
                .expect("lens a11y queue poisoned")
                .publish(if served {
                    frame.admissions
                } else {
                    Default::default()
                });
            if !served {
                lens.a11y_forest.retry_publication();
            }
        }
    }

    /// A lens window's surface plan: its OWN pane space (`App::lenses`) walked
    /// at its size — the same geometry the primary uses, per window. Canvas
    /// leaf = the lens camera's view; other leaves = panes; seams = dividers;
    /// a torn-out workbench's live tiles = content surfaces. No canvas-inset
    /// content in a lens (the tile IS the lens's content story); chrome
    /// composites separately in `render_lens`.
    pub(super) fn lens_plan(&self, ordinal: usize, w: u32, h: u32) -> Vec<crate::surface::Surface> {
        let frame = self.app.lenses.get(ordinal).and_then(Option::as_ref)
            .filter(|_| self.app.blueprint_space(crate::action::SpaceRef::Lens(ordinal)).is_none())
            .map(|space| self.outer_rects(ordinal + 1, space, None, (w.max(1), h.max(1))));
        lens_content_plan_with_rects(&self.app, &self.content_sessions, ordinal, w, h,
            frame.as_deref())
    }

    /// A pane's `PaneContent` in a LENS window's space.
    pub(super) fn lens_pane_content(
        &self,
        ordinal: usize,
        id: crate::panes::PaneId,
    ) -> Option<PaneContent> {
        lens_pane_content(&self.app, ordinal, id)
    }

    /// Render one lens window: its pane space composited through its host —
    /// the canvas leaf through the lens camera (installed around the frame,
    /// stashed after), every other leaf through the SAME retained pane runner
    /// the primary uses. That shared runner is the identity story: a pane torn
    /// out to a lens keeps its DOM, widget state, and scroll because the
    /// runner never moved — only its leaf changed trees.
    pub(super) fn render_lens(&mut self, id: WindowId) {
        if !self.poll_render_recovery() { return; }
        self.bind_lens_capture();
        if self.poll_live_settings() {
            self.request_redraw();
        }
        let Some(lens) = self.lens_windows.get(&id) else {
            return;
        };
        let (lw, lh, ordinal) = (lens.width, lens.height, lens.ordinal);
        let Some(host) = lens.host.as_ref() else { return; };
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        let (surface_device, surface_queue) =
            (host.device().clone(), host.queue().clone());
        let surfaces = self.lens_plan(ordinal, lw, lh);
        if surfaces.is_empty() {
            return;
        }
        // Visibility includes every open window. Each render invocation pumps
        // a retained document once; separate window events remain separate polls.
        self.pump_visible_documents();
        let mut animating = self
            .content_sessions
            .pool
            .window_needs_frame(ordinal as u64 + 1);
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        let mut surface_failures = Vec::new();
        // Pass 1 keeps scene construction and imported frames in plan order.
        let mut scenes: Vec<PlannedLayer> = Vec::with_capacity(surfaces.len());
        for surface in &surfaces {
            let rect = surface.rect;
            let (rw, rh) = (
                rect.w.round().max(1.0) as u32,
                rect.h.round().max(1.0) as u32,
            );
            let (scene, clear) = match surface.kind {
                crate::surface::SurfaceKind::Graph(pane) => {
                    let (scene, anim) = self
                        .app
                        .graph_pane_frame(pane, rw, rh)
                        .unwrap_or_else(|| (Scene::default(), false));
                    animating |= anim;
                    animating |= self.app.resolve_pending_images() > 0;
                    (scene, wgpu::Color::WHITE)
                },
                crate::surface::SurfaceKind::Pane(pid) => {
                    let content = self.lens_pane_content(ordinal, pid);
                    let scene = self.pane_scene_by_kind(pid, content.as_ref(), rw, rh);
                    (scene, wgpu::Color::TRANSPARENT)
                },
                // A workbench tile whose pane tore out here: the SAME session
                // the primary would frame, at this cell's size (already pumped
                // above).
                crate::surface::SurfaceKind::Content(node) => {
                    match super::render::lens_content_frame(
                        &mut self.content_sessions.pool,
                        &surface.id,
                    ) {
                        Some(pelt_core::PeltLayer::Document(scene)) => (scene, wgpu::Color::WHITE),
                        Some(pelt_core::PeltLayer::Surface(result)) => {
                            #[cfg(all(
                                any(feature = "weld", feature = "scry", feature = "servo"),
                                windows
                            ))]
                            {
                                match self.import_content_layer(
                                    node,
                                    surface,
                                    result,
                                    &surface_device,
                                    &surface_queue,
                                ) {
                                    Ok(Some(layer)) => scenes.push(PlannedLayer::Imported(layer)),
                                    Ok(None) => {},
                                    Err(error) => surface_failures.push((node, error)),
                                }
                                // Browser capture is asynchronous, as in the primary.
                                animating = true;
                            }
                            // No native frame importer is registered on other platforms.
                            #[cfg(not(all(
                                any(feature = "weld", feature = "scry", feature = "servo"),
                                windows
                            )))]
                            let _ = (node, result);
                            continue;
                        },
                        None => continue,
                    }
                },
                crate::surface::SurfaceKind::Frame => (self.outer_scene(ordinal + 1), wgpu::Color::TRANSPARENT),
                crate::surface::SurfaceKind::Divider(_) => {
                    (Scene::default(), crate::ui::SEAM_CLEAR)
                },
                // No canvas-inset content / chrome layer in a lens's plan
                // (chrome composites separately below).
                _ => continue,
            };
            scenes.push(PlannedLayer::Scene(PlannedScene {
                id: surface.id.0,
                kind: surface.kind,
                placement: ExternalTexturePlacement::new(rect.dest()),
                dims: (rw, rh),
                scene,
                clear,
            }));
        }
        // The lens's chrome (its window-root in the shared chrome forest):
        // the caption chip, composited on top when there is one to show.
        if crate::app::focused_caption(&self.app.graph_runtimes).is_some() {
            let slot = ordinal + 1;
            self.chrome.ensure_slot(slot);
            let scene = self.chrome.scene(slot, lw, lh);
            scenes.push(PlannedLayer::Scene(PlannedScene {
                id: crate::surface::SurfaceId::CHROME.0,
                kind: crate::surface::SurfaceKind::Chrome,
                placement: ExternalTexturePlacement::new([0.0, 0.0, lw as f32, lh as f32]),
                dims: (lw, lh),
                scene,
                clear: wgpu::Color::TRANSPARENT,
            }));
        }
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        if self.fail_surface_frames(surface_failures) {
            return;
        }
        // Pass 2 is all-or-refused; keep textures alive until presentation/capture.
        let Some(lens) = self.lens_windows.get_mut(&id) else { return; };
        let Some(host) = lens.host.as_ref() else { return; };
        let generation = host.core().generation();
        let mut raster_textures = Vec::new();
        let layers: Result<Vec<CompositeLayer>, (u64, genet_render_host::RasterizeFailure)> = scenes
            .into_iter()
            .map(|source| match source {
                PlannedLayer::Scene(scene) => {
                    let (texture, view) = host.core().try_rasterize_scaled_for(
                        scene.id, &scene.scene, scene.dims.0, scene.dims.1,
                        ColorLoad::Clear(scene.clear), 1.0,
                    ).map_err(|failure| (scene.id, failure))?;
                    raster_textures.push(texture);
                    Ok(CompositeLayer { kind: scene.kind, view, placement: scene.placement })
                },
                PlannedLayer::Imported(layer) => Ok(layer),
            }).collect();
        let layers = match layers {
            Ok(layers) => layers,
            Err((source, failure)) => {
                drop(raster_textures);
                #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
                drop((surface_device, surface_queue));
                self.refuse_raster_frame(ordinal as u64 + 1, generation, source, failure);
                return;
            }
        };
        let frame = match host.acquire_status() {
            genet_render_host::SurfaceAcquire::Frame(frame) => frame,
            genet_render_host::SurfaceAcquire::DeviceLost(loss) => {
                drop(layers);
                drop(raster_textures);
                #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
                drop((surface_device, surface_queue));
                self.refuse_raster_frame(ordinal as u64 + 1, generation, 0, genet_render_host::RasterizeFailure::DeviceLost(loss));
                return;
            },
            genet_render_host::SurfaceAcquire::SurfaceLost => {
                drop(layers);
                drop(raster_textures);
                #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
                drop((surface_device, surface_queue));
                self.detach_render_surface(ordinal as u64 + 1);
                return;
            },
            genet_render_host::SurfaceAcquire::Unavailable { .. } => return,
        };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        for layer in &layers {
            host.renderer().compose_external_texture(&layer.view, &target, host.format(), lw, lh, layer.placement);
        }
        if let Some(loss) = host.core().device_failure() {
            drop(frame);
            drop(target);
            drop(layers);
            drop(raster_textures);
            #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
            drop((surface_device, surface_queue));
            self.refuse_raster_frame(ordinal as u64 + 1, generation, 0, genet_render_host::RasterizeFailure::DeviceLost(loss));
            return;
        }
        host.queue().present(frame);
        // Only the request's chosen Lens may consume it. Failure participates
        // in the same scenario receipt as the primary, rather than warning only.
        if self.pending_lens_capture_window == Some(id) {
            self.pending_lens_capture_window = None;
            if let Some(path) = self.pending_lens_capture.take() {
                if !capture_composed(host, &layers, lw, lh, &path) {
                    self.main_receipt.errors.push(format!("lens capture {} window={}: readback or PNG failed", path.display(), ordinal + 1));
                }
            }
        }
        if animating {
            lens.window.request_redraw();
        }
    }

    /// Route one lens window's event: pane presses dispatch into the SHARED
    /// retained runners (the same round trips the primary uses); canvas
    /// gestures run with the lens's own camera installed around the pass
    /// (pan, zoom, grab); resize, close.
    pub(super) fn lens_event(&mut self, id: WindowId, event: WindowEvent) {
        let Some(lens) = self.lens_windows.get(&id) else {
            return;
        };
        let (window, position) = (lens.ordinal as u64 + 1, lens.cursor);
        let mouse_delivered = if let WindowEvent::MouseInput { state, button, .. } = &event {
            let phase = match state {
                ElementState::Pressed => inker::PointerPhase::Down,
                ElementState::Released => inker::PointerPhase::Up,
            };
            let appearance = if phase == inker::PointerPhase::Up
                && super::input::surface_mouse_button(*button)
                    .is_some_and(super::window_input::lens_button)
                && self.contributed_pointer.is_none()
            {
                super::window_input::captured_mouse(&self.content_sessions.pool).filter(
                    |appearance| {
                        self.content_sessions
                            .pool
                            .placement(appearance)
                            .is_some_and(|p| p.window == window)
                    },
                )
            } else {
                None
            };
            if appearance.is_some() {
                self.sync_content_placements();
            }
            self.window_mouse_input(
                window,
                appearance,
                position,
                phase,
                super::input::surface_mouse_button(*button),
            )
        } else {
            false
        };
        let Some(lens) = self.lens_windows.get_mut(&id) else {
            return;
        };
        if matches!(
            event,
            WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            lens.a11y.update_window_bounds(&lens.window);
        }
        if let WindowEvent::Focused(focused) = &event {
            lens.focused = *focused;
            lens.a11y.update_window_focus(*focused);
            let window = lens.ordinal as u64 + 1;
            self.native_input_focus(window, *focused);
            self.push_a11y_tree();
            return;
        }
        match event {
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl = mods.state().control_key();
                self.alt = mods.state().alt_key();
                self.shift = mods.state().shift_key();
                self.meta = mods.state().super_key();
                return;
            },
            WindowEvent::Touch(touch) => {
                let window = lens.ordinal as u64 + 1;
                self.deliver_touch_in(window, touch);
                return;
            },
            WindowEvent::KeyboardInput { event, .. } => {
                let window = lens.ordinal as u64 + 1;
                self.deliver_window_key(
                    window,
                    &event.logical_key,
                    event.state == ElementState::Pressed,
                    event.text.as_deref(),
                );
                return;
            },
            WindowEvent::Ime(ime) => {
                let window = lens.ordinal as u64 + 1;
                self.deliver_window_ime(window, &ime);
                return;
            },
            WindowEvent::CloseRequested => {
                let ordinal = lens.ordinal;
                self.bind_lens_capture();
                if self.pending_lens_capture_window == Some(id) {
                    let generation = self.lens_windows.get(&id).and_then(|lens| lens.host.as_ref()).map_or(0, |host| host.core().generation());
                    self.refuse_window_capture(ordinal as u64 + 1, cambium_rootstock::CaptureRefusal::SurfaceUnavailable {
                        generation, error: "capture target Lens closed".into(),
                    });
                }
                self.native_input_focus(ordinal as u64 + 1, false);
                self.window_focus.remove(&(ordinal as u64 + 1));
                self.lens_windows.remove(&id);
                self.outer_frames.borrow_mut().remove(&(ordinal+1));
                let closed_panes: Vec<_> = self
                    .app
                    .lenses
                    .get(ordinal)
                    .and_then(Option::as_ref)
                    .map(|space| space.iter_leaves().map(|(id, _, _)| id).collect())
                    .unwrap_or_default();
                if let Some(space) = self.app.lenses.get_mut(ordinal) {
                    *space = None;
                }
                if let Some(blueprint) = self.app.lens_blueprints.get_mut(ordinal) {
                    *blueprint = None;
                }
                for pane in closed_panes {
                    self.evict_pane_renderer(pane);
                }
                self.app.window_count = 1 + self.lens_windows.len();
                self.app.note(crate::observe::AppEvent::WindowClosed);
                // Persist the departure: a window closed on purpose stays
                // closed across a restart (its slot saves as null).
                self.act(Action::SaveSession);
                return;
            },
            WindowEvent::Resized(size) => {
                lens.width = size.width.max(1);
                lens.height = size.height.max(1);
                if let Some(host) = lens.host.as_mut() { host.resize(lens.width, lens.height); }
                lens.window.request_redraw();
                return;
            },
            WindowEvent::RedrawRequested => {
                self.render_lens(id);
                self.push_a11y_tree();
                return;
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if super::input::surface_mouse_button(button)
                .is_some_and(super::window_input::lens_button) =>
            {
                // A press routes by the lens's OWN plan: a pane press
                // dispatches into the shared runner; a tile press routes into
                // the shared SESSION (a link is a durable navigation through
                // the spine); a canvas press falls through to the
                // camera-gesture block below.
                let (x, y) = lens.cursor;
                let (lw, lh, ordinal) = (lens.width, lens.height, lens.ordinal);
                if reject_lens_pane_press(self) {
                    return;
                }
                if button == MouseButton::Left && self.outer_press(ordinal + 1,
                    (lw.max(1), lh.max(1)), x, y)
                {
                    if let Some(lens) = self.lens_windows.get(&id) { lens.window.request_redraw(); }
                    return;
                }
                let plan = self.lens_plan(ordinal, lw, lh);
                // Revalidate while the retained content focus still names its
                // receiver, before a fresh Pane/None branch can change it.
                self.sync_content_placements();
                if let Some(hit) = lens_press_hit(
                    &self.content_sessions.pool,
                    ordinal as u64 + 1,
                    &plan,
                    crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                    inker::PhysicalPosition { x, y },
                ) {
                    match hit.kind {
                        crate::surface::SurfaceKind::Pane(pid) => {
                            // The press anchors pane ops here, exactly as in
                            // the primary — the active pane is GLOBAL (ids are
                            // unique across spaces), so close/divider/summon-
                            // beside now aim at this lens's tree.
                            if button != MouseButton::Left {
                                return;
                            }
                            self.app.active_pane = Some(pid);
                            self.app.raise_floating_pane(pid);
                            if let Some(pane) = self.renderers.contributed.get_mut(pid) {
                                self.app.focus = crate::surface::FocusTarget::Pane(pid);
                                self.pointer_capture = Some(SurfaceKind::Pane(pid));
                                self.contributed_pointer = plan
                                    .iter()
                                    .find(|surface| surface.id == hit.id)
                                    .copied()
                                    .map(|surface| super::input::ContributedPointer {
                                        surface,
                                        generation: pane.generation(),
                                        window: ordinal as u64 + 1,
                                    });
                                pane.focus_at(hit.local.0, hit.local.1);
                                pane.mouse_press(hit.local.0, hit.local.1);
                                self.request_redraw();
                                return;
                            }
                            if let Some(content) = self.lens_pane_content(ordinal, pid) {
                                let dims = plan
                                    .iter()
                                    .find(|s| s.id == hit.id)
                                    .map(|s| {
                                        (
                                            s.rect.w.round().max(1.0) as u32,
                                            s.rect.h.round().max(1.0) as u32,
                                        )
                                    })
                                    .unwrap_or((lw, lh));
                                let actions =
                                    self.pane_click_actions(pid, &content, hit.local, dims);
                                for action in actions {
                                    self.act(action);
                                }
                            }
                            self.request_redraw();
                            return;
                        },
                        crate::surface::SurfaceKind::Content(_) => {
                            self.sync_content_placements();
                            let hit = super::window_input::content_press_target(
                                &mut self.app.focus,
                                &self.content_sessions.pool,
                                ordinal as u64 + 1,
                                inker::PhysicalPosition { x, y },
                                hit,
                            );
                            self.pointer_capture = Some(hit.kind);
                            self.content_mouse(
                                hit.id,
                                hit.local,
                                inker::PointerPhase::Down,
                                super::input::surface_mouse_button(button),
                            );
                            if let Some(lens) = self.lens_windows.get_mut(&id) {
                                lens.window.request_redraw();
                            }
                            return;
                        },
                        // A lens seam drag: capture the band; moves lower
                        // SetSplitRatio at THIS lens's space (same spine as
                        // the primary's seam, different target tree).
                        crate::surface::SurfaceKind::Divider(index)
                            if button == MouseButton::Left =>
                        {
                            if let Some(Some(space)) = self.app.lenses.get(ordinal) {
                                let area = Rect::full(lw, lh);
                                let tiling = crate::pane::place_panes(space, area, None);
                                self.lens_divider_drag = tiling
                                    .dividers
                                    .into_iter()
                                    .find(|d| d.index == index)
                                    .map(|d| (ordinal, d));
                            }
                            return;
                        },
                        _ => {},
                    }
                }
                // Canvas press: handled by the gesture block below.
            },
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                lens.cursor = (x, y);
                let ordinal = lens.ordinal;
                if self.move_contributed_pointer(ordinal as u64 + 1, x, y) {
                    return;
                }
                if let Some(appearance) =
                    super::window_input::captured_mouse(&self.content_sessions.pool)
                    && let Some(placement) = self.content_sessions.pool.placement(&appearance)
                    && placement.window == ordinal as u64 + 1
                {
                    let local = (x - placement.rect.x, y - placement.rect.y);
                    self.content_mouse(appearance, local, inker::PointerPhase::Move, None);
                    return;
                }
                if self.outer_move(ordinal + 1, x, y) {
                    if let Some(lens) = self.lens_windows.get(&id) { lens.window.request_redraw(); }
                    return;
                }
                // A held lens seam: each move becomes a ratio through the
                // same component math as the primary's seam, lowered at the
                // LENS's space. Falls through to the camera block otherwise.
                if let Some((ord, drag)) = self.lens_divider_drag.clone() {
                    let split = crate::pane::cambium_split(drag.axis, drag.ratio);
                    let ratio =
                        split.ratio_at(drag.area.w, drag.area.h, x - drag.area.x, y - drag.area.y);
                    self.act(Action::SetSplitRatio {
                        space: crate::action::SpaceRef::Lens(ord),
                        path: drag.path,
                        ratio,
                    });
                    if let Some(lens) = self.lens_windows.get_mut(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
            },
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } if super::input::surface_mouse_button(button)
                .is_some_and(super::window_input::lens_button) =>
            {
                let (cursor, ordinal, lw, lh) = (lens.cursor, lens.ordinal, lens.width, lens.height);
                if dispatch_lens_pane_release(self, ordinal as u64 + 1, cursor, button) {
                    return;
                }
                if mouse_delivered {
                    return;
                }
                if button == MouseButton::Left && self.outer_release(ordinal + 1,
                    (lw.max(1), lh.max(1)), cursor.0, cursor.1)
                {
                    if let Some(lens) = self.lens_windows.get(&id) { lens.window.request_redraw(); }
                    return;
                }
                // Like the primary seam: moves rode Redraw; persist once.
                if self.lens_divider_drag.take().is_some() {
                    self.act(Action::SaveSession);
                    return;
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel over a tile scrolls the PAGE (the rung-5 slice-B rule,
                // per window); off-tile falls through to the camera pan below.
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        x * mere::canvas::WHEEL_PAN_SCALE,
                        y * mere::canvas::WHEEL_PAN_SCALE,
                    ),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                let (x, y) = lens.cursor;
                let (lw, lh, ordinal) = (lens.width, lens.height, lens.ordinal);
                let plan = self.lens_plan(ordinal, lw, lh);
                if let Some(hit) = crate::surface::hit_test(&plan, self.app.focus, x, y)
                    && let crate::surface::SurfaceKind::Content(node) = hit.kind
                {
                    self.content_wheel(ordinal as u64 + 1, hit.id, node, hit.local, (dx, dy));
                    if let Some(lens) = self.lens_windows.get(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
            },
            _ => {},
        }
        // Continuous graph gestures route to the captured graph PaneId. The
        // graph runtime supplies truth; `GraphPaneViews` installs and stashes
        // this pane's camera and selection around every call.
        let Some(lens) = self.lens_windows.get(&id) else {
            return;
        };
        let (lw, lh, ordinal, cursor, captured) = (
            lens.width,
            lens.height,
            lens.ordinal,
            lens.cursor,
            lens.pointer_graph,
        );
        let plan = self.lens_plan(ordinal, lw, lh);
        let graph_local = |pane: PaneId, x: f32, y: f32| {
            plan.iter()
                .find(|surface| surface.kind == SurfaceKind::Graph(pane))
                .map(|surface| (x - surface.rect.x, y - surface.rect.y))
        };
        let mut redraw = false;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let pane = captured.or_else(|| {
                    crate::surface::hit_test(
                        &plan,
                        crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                        x,
                        y,
                    )
                    .and_then(|hit| match hit.kind {
                        SurfaceKind::Graph(pane) => Some(pane),
                        _ => None,
                    })
                });
                if let Some((pane, (local_x, local_y))) =
                    pane.and_then(|pane| graph_local(pane, x, y).map(|local| (pane, local)))
                {
                    redraw = self.app.graph_pane_cursor_moved(pane, local_x, local_y);
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        x * mere::canvas::WHEEL_PAN_SCALE,
                        y * mere::canvas::WHEEL_PAN_SCALE,
                    ),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                if let Some(hit) = crate::surface::hit_test(
                    &plan,
                    crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                    cursor.0,
                    cursor.1,
                ) && let SurfaceKind::Graph(pane) = hit.kind
                {
                    redraw = self.app.graph_pane_wheel(pane, dx, dy);
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = pointer_button(button) {
                    let pane = match state {
                        ElementState::Pressed => crate::surface::hit_test(
                            &plan,
                            crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                            cursor.0,
                            cursor.1,
                        )
                        .and_then(|hit| match hit.kind {
                            SurfaceKind::Graph(pane) => Some(pane),
                            _ => None,
                        }),
                        ElementState::Released => self
                            .lens_windows
                            .get_mut(&id)
                            .and_then(|lens| lens.pointer_graph.take()),
                    };
                    if let Some((pane, (local_x, local_y))) = pane.and_then(|pane| {
                        graph_local(pane, cursor.0, cursor.1).map(|local| (pane, local))
                    }) {
                        redraw = match state {
                            ElementState::Pressed => {
                                self.app.raise_floating_pane(pane);
                                let handled = self
                                    .app
                                    .graph_pane_pointer_down(pane, button, local_x, local_y);
                                self.app.focus = crate::surface::FocusTarget::Graph(pane);
                                if let Some(lens) = self.lens_windows.get_mut(&id) {
                                    lens.pointer_graph = Some(pane);
                                }
                                handled
                            },
                            ElementState::Released => self
                                .app
                                .graph_pane_pointer_up(pane, button, local_x, local_y),
                        };
                    }
                }
            },
            _ => {},
        }
        if redraw {
            if let Some(lens) = self.lens_windows.get_mut(&id) {
                lens.window.request_redraw();
            }
        }
    }
}

pub(super) fn lens_content_plan(
    app: &crate::app::App,
    contents: &super::NodeDocuments,
    ordinal: usize,
    w: u32,
    h: u32,
) -> Vec<crate::surface::Surface> {
    let frame = super::outer_workspace::frame_body_rects(app,
        crate::action::SpaceRef::Lens(ordinal), (w.max(1), h.max(1)));
    lens_content_plan_with_rects(app, contents, ordinal, w, h, frame.as_deref())
}

fn lens_content_plan_with_rects(
    app: &crate::app::App,
    contents: &super::NodeDocuments,
    ordinal: usize,
    w: u32,
    h: u32,
    frame_rects: Option<&[(PaneId, Rect)]>,
) -> Vec<crate::surface::Surface> {
    let Some(Some(space)) = app.lenses.get(ordinal) else {
        return Vec::new();
    };
    let area = Rect::full(w.max(1), h.max(1));
    let (pane_rects, divider_rects, float_rects): (
        Vec<(crate::panes::PaneId, Rect)>,
        Vec<(u32, Rect)>,
        Vec<(crate::panes::PaneId, Rect)>,
    ) = match app.blueprint_space(crate::action::SpaceRef::Lens(ordinal)) {
        Some(blueprint) => {
            let placements = crate::panes::place_space(blueprint, area, None);
            (
                placements
                    .panes
                    .into_iter()
                    .map(|pane| (pane.id, pane.rect))
                    .collect(),
                placements
                    .dividers
                    .into_iter()
                    .map(|divider| (divider.index, divider.rect))
                    .collect(),
                placements
                    .floats
                    .into_iter()
                    .map(|pane| (pane.id, pane.rect))
                    .collect(),
            )
        },
        None => (frame_rects.unwrap_or_default().to_vec(), Vec::new(), Vec::new()),
    };
    let mut base: Vec<(SurfaceKind, Rect)> = pane_rects
        .iter()
        .filter_map(|(id, rect)| {
            lens_pane_content(app, ordinal, *id).map(|content| (id, rect, content))
        })
        .map(|(id, rect, content)| {
            if matches!(content, PaneContent::Orrery) {
                (SurfaceKind::Graph(*id), *rect)
            } else if let PaneContent::Tile(m) = content
                && contents.contains_key(&m)
            {
                // A torn-out tile: the pinned pane composites its live
                // session as this window's content surface.
                (SurfaceKind::Content(m), *rect)
            } else {
                (SurfaceKind::Pane(*id), *rect)
            }
        })
        .collect();
    if app.blueprint_space(crate::action::SpaceRef::Lens(ordinal)).is_none() {
        base.insert(0, (SurfaceKind::Frame, area));
    }
    base.extend(
        divider_rects
            .iter()
            .map(|(index, rect)| (SurfaceKind::Divider(*index), *rect)),
    );
    // Workbench tiles in a LENS (rung-7 depth: content tiles follow the
    // pane): when the workbench pane tore out to this window, its cells'
    // live tiles compose as content surfaces at their body rects — the
    // same walk the primary plan does, at the lens pane's rect.
    let workbench_pane = pane_rects
        .iter()
        .find(|(id, _)| {
            matches!(
                lens_pane_content(app, ordinal, *id),
                Some(PaneContent::Workbench)
            )
        })
        .map(|(id, rect)| (*id, *rect));
    let tiles: Vec<(uuid::Uuid, Rect)> = workbench_pane
        .map(|(pane, rect)| {
            let geom = app
                .workbench_for_pane(pane)
                .and_then(|workbench| workbench.to_arrangement().1);
            crate::workbench_tiling::place_workbench(geom.as_ref(), rect)
                .cells
                .iter()
                .filter_map(|c| {
                    let m = c.active_member()?;
                    contents.contains_key(&m).then(|| (m, c.body()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut surfaces = crate::surface::assemble(&base, &tiles, None, None);
    let mut appearance_roles = Vec::new();
    if let Some((pane, rect)) = workbench_pane {
        let geom = app
            .workbench_for_pane(pane)
            .and_then(|workbench| workbench.to_arrangement().1);
        for cell in crate::workbench_tiling::place_workbench(geom.as_ref(), rect).cells {
            if let Some(node) = cell.active_member()
                && contents.contains_key(&node)
            {
                appearance_roles.push((
                    node,
                    crate::surface::appearance::workbench_appearance_role(pane, node),
                    cell.body(),
                ));
            }
        }
    }
    appearance_roles.extend(pane_rects.iter().filter_map(|(pane, rect)| {
        match lens_pane_content(app, ordinal, *pane) {
            Some(PaneContent::Tile(node)) if contents.contains_key(&node) => {
                Some((node, 0x1000_0000_0000_0000 | pane.0, *rect))
            },
            _ => None,
        }
    }));
    appearance_roles.extend(
        float_rects.iter().filter_map(|(pane, rect)| {
            match lens_pane_content(app, ordinal, *pane) {
                Some(PaneContent::Tile(node)) if contents.contains_key(&node) => {
                    Some((node, 0x1000_0000_0000_0000 | pane.0, *rect))
                },
                _ => None,
            }
        }),
    );
    surfaces.extend(float_rects.into_iter().filter_map(|(id, rect)| {
        let content = lens_pane_content(app, ordinal, id)?;
        let kind = match content {
            PaneContent::Orrery => SurfaceKind::Graph(id),
            PaneContent::Tile(member) if contents.contains_key(&member) => {
                SurfaceKind::Content(member)
            },
            _ => SurfaceKind::Pane(id),
        };
        Some(crate::surface::Surface {
            id: crate::surface::SurfaceId::for_kind(kind),
            kind,
            rect,
        })
    }));
    crate::surface::appearance::assign_content_appearance_ids(
        &mut surfaces,
        ordinal as u32 + 1,
        &appearance_roles,
    );
    surfaces
}

fn lens_pane_content(app: &crate::app::App, ordinal: usize, id: PaneId) -> Option<PaneContent> {
    app.lenses
        .get(ordinal)
        .and_then(|s| s.as_ref())
        .and_then(|space| {
            space
                .iter_leaves()
                .find(|(pid, _, _)| *pid == id)
                .map(|(_, content, _)| content.clone())
        })
}

#[cfg(test)]
#[path = "lens_tests.rs"]
mod tests;

fn append_foreign_lens(
    frame: &mut super::document_a11y::Projected,
    forest: &crate::foreign_a11y::ForeignA11y,
    placements: Vec<(uuid::Uuid, accesskit::Rect)>,
    focus: crate::surface::FocusTarget,
) -> Vec<uuid::Uuid> {
    let mut visible = Vec::new();
    for (node, rect) in placements {
        let Some(mut graft) = forest.graft(node, rect) else {
            continue;
        };
        graft.set_label("browser page");
        let graft_id = uxtree::node_id_for_path(&format!("turnstone/doc/{node}"));
        if let Some((_, root)) = frame
            .tree
            .nodes
            .iter_mut()
            .find(|(id, _)| *id == frame.tree.root)
        {
            let mut children = root.children().to_vec();
            children.push(graft_id);
            root.set_children(children);
        }
        frame.tree.nodes.push((graft_id, graft));
        if matches!(focus, crate::surface::FocusTarget::Content { node: current, .. } if current == node)
        {
            frame.focus = Some(graft_id);
        }
        visible.push(node);
    }
    visible
}
