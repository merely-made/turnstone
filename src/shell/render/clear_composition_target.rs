// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Composition blends over its target. Present and capture both begin from
/// transparent black; the retained window's authored background owns color.
pub(in super::super) fn clear_composition_target(host: &SurfaceHost, target: &wgpu::TextureView) {
    let mut encoder = host
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("turnstone window target clear"),
        });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("turnstone window target clear pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    host.queue().submit([encoder.finish()]);
}
