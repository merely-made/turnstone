// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
impl Shell {
    pub(in super::super) fn present_native_lens(
        &mut self,
        staged: super::super::native_frames::StagedWindowFrame,
    ) -> bool {
        let super::super::native_frames::StagedWindowFrame {
            identity,
            revision,
            layers,
            raster_textures,
            needs_redraw,
            capture,
            diagnostics,
            ..
        } = staged;
        let (lw, lh) = identity.dimensions;
        let ordinal = identity.ordinal - 1;
        let generation = identity.generation;
        let Some(lens) = self.lens_windows.get_mut(&identity.window) else {
            return false;
        };
        let Some(host) = lens.host.as_ref() else {
            return false;
        };
        let frame = match host.acquire_status() {
            genet_render_host::SurfaceAcquire::Frame(frame) => frame,
            genet_render_host::SurfaceAcquire::DeviceLost(loss) => {
                drop(layers);
                drop(raster_textures);
                self.refuse_raster_frame(
                    ordinal as u64 + 1,
                    generation,
                    0,
                    genet_render_host::RasterizeFailure::DeviceLost(loss),
                );
                return false;
            },
            genet_render_host::SurfaceAcquire::SurfaceLost => {
                drop(layers);
                drop(raster_textures);
                self.detach_render_surface(ordinal as u64 + 1);
                return false;
            },
            genet_render_host::SurfaceAcquire::Reconfigured => {
                tracing::debug!(
                    window = ordinal + 1,
                    "lens surface reconfigured; frame skipped"
                );
                return false;
            },
            genet_render_host::SurfaceAcquire::Unavailable { reason } => {
                tracing::debug!(%reason, window = ordinal + 1, "lens surface acquire skipped");
                return false;
            },
        };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        super::super::render::clear_composition_target(&host, &target);
        for layer in &layers {
            host.renderer().compose_external_texture(
                &layer.view,
                &target,
                host.format(),
                lw,
                lh,
                layer.placement,
            );
        }
        if let Some(loss) = host.core().device_failure() {
            drop(frame);
            drop(target);
            drop(layers);
            drop(raster_textures);
            self.refuse_raster_frame(
                ordinal as u64 + 1,
                generation,
                0,
                genet_render_host::RasterizeFailure::DeviceLost(loss),
            );
            return false;
        }
        host.queue().present(frame);
        self.pending_native_diagnostics.extend(diagnostics);
        self.main_receipt.lens_presented();
        // Only the request's chosen Lens may consume it. Failure participates
        // in the same scenario receipt as the primary, rather than warning only.
        if let Some(path) = capture {
            if !capture_composed(host, &layers, lw, lh, &path) {
                self.main_receipt.errors.push(format!(
                    "lens capture {} window={}: readback or PNG failed",
                    path.display(),
                    ordinal + 1
                ));
            }
        }
        if needs_redraw || self.native_frames.revision() != revision {
            lens.window.request_redraw();
        }
        self.finish_inline_frame();
        true
    }
}
