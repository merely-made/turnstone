// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
impl Shell {
    pub(in super::super) fn present_native_primary(
        &mut self,
        staged: super::super::native_frames::StagedWindowFrame,
    ) -> bool {
        let super::super::native_frames::StagedWindowFrame {
            identity,
            revision,
            layers,
            raster_textures,
            started,
            mut timings,
            raster_elapsed,
            surface_raster,
            needs_redraw,
            palette,
            nodes,
            capture,
            diagnostics,
        } = staged;
        let (w, h) = identity.dimensions;
        let generation = identity.generation;
        let Some(host) = self.host.as_ref() else {
            return false;
        };
        // Stage 6: composition and present. If the surface cannot be acquired
        // nothing is presented, so there is no frame to report: the stages
        // already written into `App` carry into the frame that does present,
        // while this attempt's raster and compose time — still locals — is
        // dropped with it. A dropped frame is worth knowing about, but it is
        // the swapchain's story, not the chrome path's.
        let compose_started = std::time::Instant::now();
        let frame = match host.acquire_status() {
            genet_render_host::SurfaceAcquire::Frame(frame) => frame,
            genet_render_host::SurfaceAcquire::DeviceLost(loss) => {
                drop(layers);
                drop(raster_textures);
                self.refuse_raster_frame(
                    0,
                    generation,
                    0,
                    genet_render_host::RasterizeFailure::DeviceLost(loss),
                );
                return false;
            },
            genet_render_host::SurfaceAcquire::SurfaceLost => {
                drop(layers);
                drop(raster_textures);
                self.detach_render_surface(0);
                return false;
            },
            genet_render_host::SurfaceAcquire::Reconfigured => {
                tracing::debug!("main surface reconfigured; frame skipped");
                return false;
            },
            genet_render_host::SurfaceAcquire::Unavailable { reason } => {
                tracing::debug!(%reason, "main surface acquire skipped");
                return false;
            },
        };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        clear_composition_target(&host, &target);
        for layer in &layers {
            host.renderer().compose_external_texture(
                &layer.view,
                &target,
                host.format(),
                w,
                h,
                layer.placement,
            );
        }
        if let Some(loss) = host.core().device_failure() {
            drop(frame);
            drop(target);
            drop(layers);
            drop(raster_textures);
            self.refuse_raster_frame(
                0,
                generation,
                0,
                genet_render_host::RasterizeFailure::DeviceLost(loss),
            );
            return false;
        }
        // wgpu 30 moved presentation from SurfaceTexture to Queue.
        host.queue().present(frame);
        self.pending_native_diagnostics.extend(diagnostics);
        self.main_receipt.presented();
        let compose_elapsed = compose_started.elapsed();

        // Scenario self-capture: compose the SAME layer views this frame just
        // presented into an owned COPY_SRC target and read it back — the
        // receipt is the presented frame, not a re-rasterization (a second
        // `canvas.frame()` in the same pass produced stale, layer-dropping
        // captures). Immune to focus theft and occlusion by construction.
        if let Some(path) = capture {
            tracing::info!(
                open = palette,
                suggestions = self.app.omnibar.suggestions.len(),
                surfaces = layers.len(),
                chrome = layers
                    .iter()
                    .any(|l| matches!(l.kind, crate::surface::SurfaceKind::Chrome)),
                nodes,
                "capture state"
            );
            match start_capture_composed(host, &layers, w, h) {
                Ok(frame) => {
                    self.pending_capture_readback =
                        Some(super::super::appearance_receipt::PendingCapture {
                            path,
                            frame,
                            started: std::time::Instant::now(),
                        })
                },
                Err(error) => self.main_receipt.errors.push(error),
            }
        }

        timings.raster += raster_elapsed;
        timings.note_surface_raster(surface_raster);
        timings.compose += compose_elapsed;
        // One line per presented frame, carrying the whole and its parts. The
        // stage fields are what turn "the palette is open and the frame is
        // slow" into "this stage is slow": `palette` labels each frame, so a
        // closed-versus-open pair attributes its own delta without a second
        // tool.
        let timings = &timings;
        let split = timings.surface_raster;
        let raster_totals = split.totals();
        tracing::debug!(
            frame_ms = started.elapsed().as_secs_f32() * 1000.0,
            surfaces = layers.len(),
            palette,
            suggestions_ms = ms(timings.suggestions),
            suggestion_runs = timings.suggestion_runs,
            suggestion_refits = timings.suggestion_refits,
            chrome_sync_ms = ms(timings.chrome_sync),
            chrome_scene_ms = ms(timings.chrome_scene),
            chrome_syncs = timings.chrome_syncs,
            pane_scenes_ms = ms(timings.pane_scenes),
            raster_ms = ms(timings.raster),
            rasterized = raster_totals.rasterized,
            dirty_tiles = raster_totals.dirty_tiles,
            graph_rast = split.graph.rasterized,
            graph_tiles = split.graph.dirty_tiles,
            content_rast = split.content.rasterized,
            content_tiles = split.content.dirty_tiles,
            pane_rast = split.pane.rasterized,
            pane_tiles = split.pane.dirty_tiles,
            divider_rast = split.divider.rasterized,
            divider_tiles = split.divider.dirty_tiles,
            frame_rast = split.frame.rasterized,
            frame_tiles = split.frame.dirty_tiles,
            chrome_rast = split.chrome.rasterized,
            chrome_tiles = split.chrome.dirty_tiles,
            compose_ms = ms(timings.compose),
            w,
            h,
            "frame"
        );

        // An overlay bar mid-hold or mid-fade needs the next frame to draw it
        // one step dimmer. Without this the renderer, being change-driven,
        // would stop after the scroll and leave the bar frozen on screen --
        // an auto-hiding bar that never hides.
        if (needs_redraw || self.native_frames.revision() != revision)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
        self.finish_inline_frame();
        true
    }
}
