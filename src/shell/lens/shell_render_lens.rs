// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Render one lens window: its pane space composited through its host —
    /// the canvas leaf through the lens camera (installed around the frame,
    /// stashed after), every other leaf through the SAME retained pane runner
    /// the primary uses. That shared runner is the identity story: a pane torn
    /// out to a lens keeps its DOM, widget state, and scroll because the
    /// runner never moved — only its leaf changed trees.
    pub(in super::super) fn render_lens(&mut self, id: WindowId) -> bool {
        if !self.poll_render_recovery() {
            return false;
        }
        let mut presented = false;
        if self.native_frames.has(id) {
            let (completed, continue_render) = self.poll_native_frame(id);
            presented = completed;
            if !continue_render {
                return presented;
            }
        }
        if !self.native_frames.prepare(id) {
            self.request_native_frame_redraws();
            return presented;
        }
        self.bind_lens_capture();
        if self.poll_live_settings() {
            self.request_redraw();
        }
        let Some(lens) = self.lens_windows.get(&id) else {
            self.native_frames.cancel(id);
            return presented;
        };
        let (lw, lh, ordinal) = (lens.width, lens.height, lens.ordinal);
        let Some(host) = lens.host.as_ref() else {
            self.native_frames.cancel(id);
            return presented;
        };
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        let (surface_device, surface_queue) = (host.device().clone(), host.queue().clone());
        self.prepare_inline_owners(ordinal + 1, (lw, lh));
        if let Some(area) = self.pane_ime_cursor_area(ordinal as u64 + 1)
            && let Some(lens) = self.lens_windows.get(&id)
        {
            lens.window.set_ime_cursor_area(
                PhysicalPosition::new(area.x, area.y),
                PhysicalSize::new(area.w, area.h),
            );
        }
        let surfaces = self.lens_plan(ordinal, lw, lh);
        if surfaces.is_empty() {
            self.native_frames.cancel(id);
            return presented;
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
        let scale = self.app.shell_chrome_config().layout_scale(ordinal + 1);
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
                    self.save_ready_graph_scene(pane);
                    animating |= anim;
                    animating |= self.app.resolve_pending_images() > 0;
                    (scene, crate::map_theme::clear_color(self.app.map_theme()))
                },
                crate::surface::SurfaceKind::Pane(pid) => {
                    if self
                        .app
                        .blueprint_space(crate::action::SpaceRef::Lens(ordinal))
                        .is_none()
                        && self.renderers.inline_mount(pid).is_some()
                    {
                        continue;
                    }
                    let content = self.lens_pane_content(ordinal, pid);
                    let scene = self.pane_scene_by_kind(pid, content.as_ref(), rw, rh);
                    (scene, wgpu::Color::TRANSPARENT)
                },
                // A workbench tile whose pane tore out here: the SAME session
                // the primary would frame, at this cell's size (already pumped
                // above).
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
                                animating |= pending;
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
                    match super::super::render::lens_content_frame(
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
                crate::surface::SurfaceKind::Frame | crate::surface::SurfaceKind::FloatFrame(_) => {
                    (self.outer_scene(ordinal + 1), wgpu::Color::TRANSPARENT)
                },
                crate::surface::SurfaceKind::Divider(_) => {
                    (Scene::default(), crate::ui::SEAM_CLEAR)
                },
                // No canvas-inset content / chrome layer in a lens's plan
                // (chrome composites separately below).
                _ => continue,
            };
            scenes.push(PlannedLayer::Scene(PlannedScene {
                id: if matches!(
                    surface.kind,
                    SurfaceKind::Frame | SurfaceKind::FloatFrame(_)
                ) {
                    crate::surface::SurfaceId::frame_window(ordinal + 1).0
                } else {
                    surface.id.0
                },
                kind: surface.kind,
                placement: frame_placement(surface, lw, lh),
                dims: if matches!(surface.kind, SurfaceKind::FloatFrame(_)) {
                    (lw, lh)
                } else {
                    (rw, rh)
                },
                scale,
                scene,
                clear,
            }));
        }
        // Each lens keeps its title and context rows in its own window root,
        // including the empty-selection state and its native caption insets.
        {
            let slot = ordinal + 1;
            self.chrome.ensure_slot(slot);
            self.chrome.sync(
                &self.app,
                &[
                    (0, self.width as f32, self.height as f32),
                    (slot, lw as f32, lh as f32),
                ],
            );
            let scene = self.chrome.scene(slot, lw, lh);
            scenes.push(PlannedLayer::Scene(PlannedScene {
                id: crate::surface::SurfaceId::CHROME.0,
                kind: crate::surface::SurfaceKind::Chrome,
                placement: ExternalTexturePlacement::new([0.0, 0.0, lw as f32, lh as f32]),
                dims: (lw, lh),
                scale,
                scene,
                clear: wgpu::Color::TRANSPARENT,
            }));
        }
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        if self.fail_surface_frames(surface_failures) {
            return presented;
        }
        if let Some(area) = self.document_ime_cursor_area(ordinal as u64 + 1)
            && let Some(lens) = self.lens_windows.get(&id)
        {
            tracing::debug!(target: "turnstone::ime", window = ordinal + 1, x = area.x, y = area.y,
                width = area.w, height = area.h, "shared document caret geometry");
            lens.window.set_ime_cursor_area(
                PhysicalPosition::new(area.x, area.y),
                PhysicalSize::new(area.w, area.h),
            );
        }
        // Pass 2 is all-or-refused; keep textures alive until presentation/capture.
        let Some(lens) = self.lens_windows.get_mut(&id) else {
            return presented;
        };
        let Some(host) = lens.host.as_ref() else {
            return presented;
        };
        let generation = host.core().generation();
        let batch = match host.shared_core().try_begin_native_frame() {
            Ok(batch) => batch,
            Err(genet_render_host::NativeFrameStartError::Busy) => return presented,
            Err(genet_render_host::NativeFrameStartError::Refused(failure)) => {
                self.native_frames.cancel(id);
                self.refuse_raster_frame(ordinal as u64 + 1, generation, 0, failure);
                return presented;
            },
        };
        let started = std::time::Instant::now();
        let mut source_diagnostics = Vec::new();
        let mut raster_textures = Vec::new();
        let shared_frame = surfaces
            .iter()
            .any(|surface| surface.kind == SurfaceKind::Frame);
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
                self.refuse_raster_frame(ordinal as u64 + 1, generation, source, failure);
                return presented;
            },
        };
        let layers = if shared_frame {
            let mut frames = self.outer_frames.borrow_mut();
            match super::super::native_composition::compose(
                frames
                    .get_mut(&(ordinal + 1))
                    .expect("shared lens frame prepared"),
                &self.app,
                host.core(),
                &batch,
                &mut self.renderers.native_images,
                &mut self.renderers.native_masks,
                ordinal + 1,
                &surfaces,
                layers,
                &mut raster_textures,
                (lw, lh),
                self.pending_lens_capture
                    .as_deref()
                    .filter(|_| {
                        self.pending_lens_capture_window == Some(id)
                            && std::env::var("TURNSTONE_A4_DIAGNOSTICS")
                                .is_ok_and(|value| value == "1")
                    })
                    .map(|path| (path, &mut source_diagnostics, lens.window.scale_factor())),
            ) {
                Ok(layers) => layers,
                Err(failure) => {
                    drop(frames);
                    drop(raster_textures);
                    self.refuse_native_composition(ordinal as u64 + 1, generation, failure);
                    return presented;
                },
            }
        } else {
            layers
        };
        let needs_redraw = animating || self.renderers.bars_visible_in(&surfaces);
        let capture = if self.pending_lens_capture_window == Some(id) {
            self.pending_lens_capture_window = None;
            self.pending_lens_capture.take()
        } else {
            None
        };
        let identity = self.native_frame_identity(id, ordinal + 1, generation, (lw, lh));
        let staged = super::super::native_frames::StagedWindowFrame {
            identity,
            revision: self.native_frames.revision(),
            layers,
            raster_textures,
            started,
            timings: Default::default(),
            raster_elapsed: started.elapsed(),
            surface_raster: Default::default(),
            needs_redraw,
            palette: self.app.omnibar.open,
            nodes: self.app.graph_runtimes.graph().nodes().count(),
            capture,
            diagnostics: source_diagnostics,
        };
        let capture_hint = staged.capture.clone();
        let pending = batch.seal_with_wake(staged, self.native_frame_wake.clone());
        self.native_frames.submit(identity, pending, capture_hint);
        presented
    }
}
