// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One surface's scene, produced by render's mutable first pass and consumed by
/// its immutable rasterization pass. Splitting the two keeps a content session's
/// mutable borrow off the immutable `host` borrow.
pub(super) struct PlannedScene {
    pub(super) id: u64,
    pub(super) kind: SurfaceKind,
    pub(super) placement: ExternalTexturePlacement,
    pub(super) dims: (u32, u32),
    /// The scale `scene` is rasterized at; its coordinates are `dims / scale`.
    pub(super) scale: f32,
    pub(super) scene: Scene,
    // Stored as the `Copy` clear color (netrender's `ColorLoad` derives nothing,
    // so it cannot be moved out of the collected vec); wrapped at the call.
    pub(super) clear: wgpu::Color,
}
