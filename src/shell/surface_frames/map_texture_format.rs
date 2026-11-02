// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn map_texture_format(
    format: &SurfaceTextureFormat,
) -> Result<wgpu::TextureFormat, SurfaceError> {
    match format {
        SurfaceTextureFormat::Rgba8Unorm => Ok(wgpu::TextureFormat::Rgba8Unorm),
        SurfaceTextureFormat::Rgba8UnormSrgb => Ok(wgpu::TextureFormat::Rgba8UnormSrgb),
        SurfaceTextureFormat::Bgra8Unorm => Ok(wgpu::TextureFormat::Bgra8Unorm),
        SurfaceTextureFormat::Bgra8UnormSrgb => Ok(wgpu::TextureFormat::Bgra8UnormSrgb),
        SurfaceTextureFormat::Other(format) => Err(SurfaceError::Unsupported(format!(
            "Turnstone has no explicit import mapping for surface texture format {format}"
        ))),
    }
}
