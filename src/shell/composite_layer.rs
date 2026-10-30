// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A rasterized surface ready to compose: its view and where it lands in the
/// frame. The self-capture path composes the same list, so the receipt is the
/// presented frame.
pub(super) struct CompositeLayer {
    pub(super) id: u64,
    pub(super) changed: bool,
    pub(super) dimensions: [u32; 2],
    pub(super) alpha: Result<netrender::SourceAlpha, String>,
    pub(super) kind: SurfaceKind,
    pub(super) view: wgpu::TextureView,
    /// Actual owned scene raster; imports retain their texture at the importer.
    pub(super) raster: Option<wgpu::Texture>,
    /// The scale the layer's scene was rasterized at.
    pub(super) raster_scale: f32,
    pub(super) placement: ExternalTexturePlacement,
}
