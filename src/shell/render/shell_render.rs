// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Build the standalone frame scene only when the window plan needs it.
    /// Shared native composition paints after installing current producers.
    pub(in super::super) fn frame_scene_for_plan(
        shared_frame: bool,
        paint: impl FnOnce() -> Scene,
    ) -> Option<Scene> {
        (!shared_frame).then(paint)
    }

    /// The layered present (born minimal at rung 3, grows into the surface
    /// plan at rung 5): rasterize each surface's scene to its own texture and
    /// compose them in order onto the frame — the canvas below, the chrome
    /// layer (transparent-cleared, alpha-blended) above when the omnibar is
    /// open. Chains another redraw while the canvas is still animating.
    pub(in super::super) fn render(&mut self) -> bool {
        self.refresh_retained_clip_playback();
        self.poll_capture_readback();
        let diagnostics = std::mem::take(&mut self.pending_native_diagnostics);
        for mut pending in diagnostics {
            match pending.frame.poll() {
                Some(Ok(frame)) => {
                    if let Err(error) = mesquite::write_png(&pending.path, &frame) {
                        self.main_receipt
                            .errors
                            .push(format!("native raster diagnostic: {error}"));
                    }
                },
                Some(Err(error)) => self
                    .main_receipt
                    .errors
                    .push(format!("native raster diagnostic: {error}")),
                None if pending.started.elapsed() > std::time::Duration::from_secs(5) => {
                    self.main_receipt
                        .errors
                        .push("native raster diagnostic exceeded five seconds".into());
                },
                None => self.pending_native_diagnostics.push(pending),
            }
        }
        if !self.pending_native_diagnostics.is_empty()
            && let Some(window) = &self.window
        {
            // Primary and lens diagnostics use the shared render core. Continue
            // their asynchronous readback even outside a self-drive scenario.
            window.request_redraw();
        }
        if !self.poll_render_recovery() || self.host.is_none() {
            return false;
        }
        let window_id = self.window.as_ref().expect("render window").id();
        let mut presented = false;
        if self.native_frames.has(window_id) {
            let (completed, continue_render) = self.poll_native_frame(window_id);
            presented = completed;
            if !continue_render {
                return presented;
            }
        }
        if self.native_child_requested() {
            return presented;
        }
        if !self.native_frames.prepare(window_id) {
            self.request_native_frame_redraws();
            return presented;
        }
        self.refresh_diagnostic_inspection();
        // Frame cost, off by default (the shell's own filter is `info`). Turn
        // it on with `RUST_LOG=turnstone::shell::render=debug` when a window
        // feels heavy: a frame is the unit that lags, and the surface count
        // beside it is what usually explains the number. This exists because
        // diagnosing a 24 ms-per-pane relayout once meant instrumenting the
        // frame loop by hand.
        let started = std::time::Instant::now();
        self.drain_surface_web_events();
        // Cursor callbacks may arrive after the move that provoked them. Poll
        // the hovered surface on frame wakes as well as immediately on input.
        self.apply_pending_surface_cursor();
        if self.poll_live_settings() {
            // A SettingsPane has already persisted the write. Refresh every
            // affected retained surface in this and any lens window now.
            self.request_redraw();
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        // Aim the IME candidate window at the caret's neighborhood, so
        // composition popups open beside the omnibar input rather than at
        // the window corner.
        if self.app.omnibar.open
            && self.app.shell_chrome_config().projects_omnibar()
            && let Some(window) = self.window.as_ref()
            && let scale = self.app.shell_chrome_config().layout_scale(0)
            && let Some((left, top)) = crate::chrome_view::chrome_position(
                &self.app.shell_chrome_config().omnibar.placement,
                w as f32 / scale,
                h as f32 / scale,
                crate::ui::CARD_W,
            )
        {
            // The omnibar card is laid out in layout pixels.
            let (pos, size) = crate::ui::ime_cursor_area_at(&self.app.omnibar, left, top);
            window.set_ime_cursor_area(
                PhysicalPosition::new(pos.0 * scale, pos.1 * scale),
                PhysicalSize::new(size.0 * scale, size.1 * scale),
            );
        }
        // The surface plan (rung 5 slice A): the ordered list of composited
        // surfaces, each with its own rect. Built by the same helper input
        // routing uses, so what a frame draws and what a pointer hits agree.
        self.prepare_inline_owners(0, (w, h));
        if let Some(area) = self.pane_ime_cursor_area(0)
            && let Some(window) = &self.window
        {
            window.set_ime_cursor_area(
                PhysicalPosition::new(area.x, area.y),
                PhysicalSize::new(area.w, area.h),
            );
        }
        let surfaces = self.surface_plan();
        let shared_frame = surfaces
            .iter()
            .any(|surface| surface.kind == SurfaceKind::Frame);
        let caption = self
            .app
            .focused_graph_pane()
            .and_then(|pane| self.app.graph_for_pane(pane))
            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
            .and_then(crate::app::focused_caption);

        self.pump_visible_documents();
        let mut needs_redraw = self.content_sessions.pool.window_needs_frame(0);

        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        let (surface_device, surface_queue) = {
            let host = self.host.as_ref().expect("host checked at render entry");
            (host.device().clone(), host.queue().clone())
        };

        // Pass 1 (mutable): produce each surface's scene at ITS rect size. Kept
        // separate from rasterization so framing a content session (which
        // borrows `content_sessions` mutably) never overlaps the immutable
        // `host` borrow the second pass holds.
        let mut scenes: Vec<PlannedLayer> = Vec::with_capacity(surfaces.len());
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        let mut surface_failures = Vec::new();
        // Every scene of the window is laid out at `physical / scale` and
        // rasterized at the window's layout scale.
        let scale = self.app.shell_chrome_config().layout_scale(0);
        for surface in &surfaces {
            let rect = surface.rect;
            let (rw, rh) = (
                rect.w.round().max(1.0) as u32,
                rect.h.round().max(1.0) as u32,
            );
            // Stage 4: scene construction for this surface. The chrome arm
            // accounts for itself in two parts below; every other surface
            // lands in `pane_scenes`, so a chrome regression cannot hide
            // behind pane cost or the reverse.
            let scene_started = std::time::Instant::now();
            let (scene, clear) = match surface.kind {
                crate::surface::SurfaceKind::Graph(pane) => {
                    // A graph surface is addressed by its pane. The app swaps
                    // this pane's camera/selection into its graph runtime for
                    // exactly this frame, then stashes it back before another
                    // pane can render the same graph.
                    let (scene, animating) = self
                        .app
                        .graph_pane_frame(pane, rw, rh)
                        .unwrap_or_else(|| (Scene::default(), false));
                    if scene.ops.is_empty() {
                        let error = self
                            .app
                            .graph_for_pane(pane)
                            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
                            .and_then(|canvas| canvas.last_translation_error());
                        tracing::warn!(?pane, ?error, "graph producer returned an empty scene");
                    }
                    self.save_ready_graph_scene(pane);
                    needs_redraw |= animating;
                    needs_redraw |= self.app.resolve_pending_images() > 0;
                    (scene, crate::map_theme::clear_color(self.app.map_theme()))
                },
                crate::surface::SurfaceKind::Content(node) => {
                    if self
                        .content_sessions
                        .appearance(node, surface.id)
                        .is_some_and(|content| content.cue_surface().is_some())
                    {
                        match self.cue_content_layer(surface, scale) {
                            Ok((layer, pending)) => {
                                if let Some(layer) = layer {
                                    scenes.push(PlannedLayer::Imported(layer));
                                }
                                needs_redraw |= pending;
                            },
                            Err(error) => {
                                tracing::error!(%node, %error, "shared content presentation refused");
                                self.main_receipt
                                    .errors
                                    .push(format!("shared content {node}: {error}"));
                            },
                        }
                        continue;
                    }
                    let mut surface_frame = None;
                    let document_frame = match self.content_sessions.pool.frame(&surface.id) {
                        Some(pelt_core::PeltLayer::Document(scene)) => Some(scene),
                        Some(pelt_core::PeltLayer::Surface(frame)) => {
                            surface_frame = Some(frame);
                            None
                        },
                        None => None,
                    };
                    if let Some(scene) = document_frame {
                        if let Some(path) = &self.pending_capture
                            && std::env::var("TURNSTONE_A4_DIAGNOSTICS")
                                .is_ok_and(|value| value == "1")
                        {
                            let path =
                                path.with_extension(format!("content-{}.scene.json", surface.id.0));
                            if let Err(error) = std::fs::write(&path, scene.snapshot_json()) {
                                tracing::warn!(?path, %error, "could not record content scene");
                            }
                        }
                        (scene, wgpu::Color::WHITE)
                    } else {
                        #[cfg(all(
                            any(feature = "weld", feature = "scry", feature = "servo"),
                            windows
                        ))]
                        if let Some(result) = surface_frame {
                            match self.import_content_layer(
                                node,
                                surface,
                                result,
                                &surface_device,
                                &surface_queue,
                            ) {
                                Ok(Some(layer)) => scenes.push(PlannedLayer::Imported(layer)),
                                Ok(None) => {},
                                Err(error) => {
                                    surface_failures.push((node, error));
                                    continue;
                                },
                            }
                            // Browser capture arrives asynchronously. Drive its
                            // mailbox until the shell has a wake bridge.
                            needs_redraw = true;
                            continue;
                        }
                        continue;
                    }
                },
                crate::surface::SurfaceKind::Pane(id) => {
                    if self
                        .app
                        .blueprint_space(crate::action::SpaceRef::Primary)
                        .is_none()
                        && self.renderers.inline_mount(id).is_some()
                    {
                        continue;
                    }
                    // The pane's scene by kind, through the SHARED retained
                    // runners (extracted so lens windows render the same
                    // panes through the same runners — the identity story).
                    let content = self.pane_content(id);
                    let scene = self.pane_scene_by_kind(id, content.as_ref(), rw, rh);
                    (scene, wgpu::Color::TRANSPARENT)
                },
                crate::surface::SurfaceKind::Frame | crate::surface::SurfaceKind::FloatFrame(_) => {
                    let Some(scene) =
                        Self::frame_scene_for_plan(shared_frame, || self.outer_scene(0))
                    else {
                        continue;
                    };
                    (scene, wgpu::Color::TRANSPARENT)
                },
                crate::surface::SurfaceKind::Divider(_) => {
                    // The band is the clear colour; nothing to draw over it.
                    (Scene::default(), crate::ui::SEAM_CLEAR)
                },
                crate::surface::SurfaceKind::Chrome => {
                    // One sync rebuilds every window's chrome projection (the
                    // one-state contract); this window paints ITS root.
                    let mut sizes = vec![(0usize, rw as f32, rh as f32)];
                    sizes.extend(
                        self.lens_windows
                            .values()
                            .map(|lens| (lens.ordinal + 1, lens.width as f32, lens.height as f32)),
                    );
                    // Stages 2 and 3, timed apart: `sync` rebuilds row views
                    // and every window projection from app state, `scene`
                    // cascades, lays out, and paints the subtree. The open-lag
                    // note suspects both; one number could not separate them.
                    let sync_started = std::time::Instant::now();
                    self.chrome.sync(&self.app, &sizes);
                    self.chrome.sync_forme_fields(&self.app, 0, &surfaces);
                    let sync_elapsed = sync_started.elapsed();
                    let chrome_scene_started = std::time::Instant::now();
                    let scene = self.chrome.scene(0, rw, rh);
                    let scene_elapsed = chrome_scene_started.elapsed();
                    let timings = &mut self.app.frame_timings;
                    timings.chrome_sync += sync_elapsed;
                    timings.chrome_scene += scene_elapsed;
                    timings.chrome_syncs += 1;
                    (scene, wgpu::Color::TRANSPARENT)
                },
            };
            if !matches!(surface.kind, crate::surface::SurfaceKind::Chrome) {
                self.app.frame_timings.pane_scenes += scene_started.elapsed();
            }
            scenes.push(PlannedLayer::Scene(PlannedScene {
                id: if matches!(
                    surface.kind,
                    SurfaceKind::Frame | SurfaceKind::FloatFrame(_)
                ) {
                    crate::surface::SurfaceId::frame_window(0).0
                } else {
                    surface.id.0
                },
                kind: surface.kind,
                placement: super::super::frame_placement(surface, w, h),
                dims: if matches!(surface.kind, SurfaceKind::FloatFrame(_)) {
                    (w, h)
                } else {
                    (rw, rh)
                },
                scale,
                scene,
                clear,
            }));
        }

        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        if self.fail_surface_frames(surface_failures) {
            return presented;
        }

        // Frame preparation publishes the current shaped document caret. Read
        // it afterwards so a resize, edit or page zoom cannot reuse old layout.
        if let Some(area) = self.document_ime_cursor_area(0)
            && let Some(window) = &self.window
        {
            tracing::debug!(target: "turnstone::ime", window = 0, x = area.x, y = area.y,
                width = area.w, height = area.h, "shared document caret geometry");
            window.set_ime_cursor_area(
                PhysicalPosition::new(area.x, area.y),
                PhysicalSize::new(area.w, area.h),
            );
        }

        // Pass 2 (immutable): rasterize each scene keyed by its surface id (so
        // an unchanged surface reuses its tile instead of rebuilding every
        // frame) and compose the layers in order.
        let host = self.host.as_ref().unwrap();
        // Stage 5: rasterization. Counted as well as timed, and split by
        // surface class, because the done condition is that a merely-open
        // palette provokes no repaint — and a settled frame that still
        // rebuilds tiles has to say WHICH surface rebuilt them before anyone
        // can act on it. The dirty counts are netrender's own, read back per
        // surface. Accumulated into a local rather than straight into
        // `self.app`, so the closure keeps its single immutable borrow of the
        // host.
        let mut surface_raster = crate::frame_timing::SurfaceRasterSplit::default();
        let raster_started = std::time::Instant::now();
        let generation = host.core().generation();
        let batch = match host.shared_core().try_begin_native_frame() {
            Ok(batch) => batch,
            Err(genet_render_host::NativeFrameStartError::Busy) => return presented,
            Err(genet_render_host::NativeFrameStartError::Refused(failure)) => {
                self.native_frames.cancel(window_id);
                self.refuse_raster_frame(0, generation, 0, failure);
                return presented;
            },
        };
        let mut source_diagnostics = Vec::new();
        // Keep each returned texture alive through composition and capture.
        let mut raster_textures = Vec::new();
        let layers: Result<Vec<CompositeLayer>, (u64, genet_render_host::RasterizeFailure)> =
            scenes
                .into_iter()
                .filter(|source| !shared_frame || !matches!(source,
                    PlannedLayer::Scene(scene) if matches!(scene.kind, SurfaceKind::Frame | SurfaceKind::FloatFrame(_))))
                .map(|source| match source {
                    PlannedLayer::Scene(scene) => {
                        let (texture, view) = batch
                            .rasterize(
                                scene.id,
                                &scene.scene,
                                scene.dims.0,
                                scene.dims.1,
                                ColorLoad::Clear(scene.clear),
                                scene.scale,
                            )
                            .map_err(|failure| (scene.id, failure))?;
                        let raster = texture.clone();
                        raster_textures.push(texture);
                        surface_raster.note(
                            scene.kind,
                            host.renderer().vello_last_dirty_count().unwrap_or(0),
                        );
                        Ok(CompositeLayer {
                            id: scene.id,
                            changed: host.renderer().vello_last_dirty_count().unwrap_or(1) != 0,
                            dimensions: [scene.dims.0, scene.dims.1],
                            alpha: Ok(netrender::SourceAlpha::Straight),
                            kind: scene.kind,
                            view,
                            raster: Some(raster),
                            raster_scale: scene.scale,
                            placement: scene.placement,
                        })
                    },
                    PlannedLayer::Imported(layer) => Ok(layer),
                })
                .collect();
        let layers = match layers {
            Ok(layers) => layers,
            Err((source, failure)) => {
                drop(raster_textures);
                #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
                drop((surface_device, surface_queue));
                self.refuse_raster_frame(0, generation, source, failure);
                return presented;
            },
        };
        let layers = if shared_frame {
            let mut frames = self.outer_frames.borrow_mut();
            match super::super::native_composition::compose(
                frames.get_mut(&0).expect("shared frame prepared"),
                &self.app,
                host.core(),
                &batch,
                &mut self.renderers.native_images,
                &mut self.renderers.native_masks,
                0,
                &surfaces,
                layers,
                &mut raster_textures,
                (w, h),
                self.pending_capture
                    .as_deref()
                    .filter(|_| {
                        std::env::var("TURNSTONE_A4_DIAGNOSTICS").is_ok_and(|value| value == "1")
                    })
                    .map(|path| {
                        (
                            path,
                            &mut source_diagnostics,
                            self.window
                                .as_ref()
                                .map_or(1.0, |window| window.scale_factor()),
                        )
                    }),
            ) {
                Ok(layers) => {
                    surface_raster.note(
                        SurfaceKind::Frame,
                        host.renderer().vello_last_dirty_count().unwrap_or(0),
                    );
                    layers
                },
                Err(failure) => {
                    drop(frames);
                    drop(raster_textures);
                    self.refuse_native_composition(0, generation, failure);
                    return presented;
                },
            }
        } else {
            layers
        };
        let raster_elapsed = raster_started.elapsed();

        let identity = self.native_frame_identity(window_id, 0, generation, (w, h));
        let staged = super::super::native_frames::StagedWindowFrame {
            identity,
            revision: self.native_frames.revision(),
            layers,
            raster_textures,
            started,
            timings: std::mem::take(&mut self.app.frame_timings),
            raster_elapsed,
            surface_raster,
            needs_redraw: needs_redraw || self.renderers.any_bars_visible(),
            palette: self.app.omnibar.open,
            nodes: self.app.graph_runtimes.graph().nodes().count(),
            capture: self.pending_capture.take(),
            diagnostics: source_diagnostics,
        };
        let capture_hint = staged.capture.clone();
        let pending = batch.seal_with_wake(staged, self.native_frame_wake.clone());
        self.native_frames.submit(identity, pending, capture_hint);
        presented
    }
}

#[cfg(test)]
#[path = "frame_scene_tests.rs"]
mod frame_scene_tests;
