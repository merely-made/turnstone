// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn capture_target(
    host: &SurfaceHost,
    layers: &[CompositeLayer],
    w: u32,
    h: u32,
) -> Result<wgpu::Texture, String> {
    if let Some(loss) = host.core().device_failure() {
        return Err(format!("capture refused: DeviceLost({loss:?})"));
    }
    let target = host.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("turnstone scenario capture"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    clear_composition_target(host, &target_view);
    for layer in layers {
        host.renderer().compose_external_texture(
            &layer.view,
            &target_view,
            wgpu::TextureFormat::Rgba8Unorm,
            w,
            h,
            layer.placement,
        );
    }
    if let Some(loss) = host.core().device_failure() {
        return Err(format!("capture refused: DeviceLost({loss:?})"));
    }
    Ok(target)
}
