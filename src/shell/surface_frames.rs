// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-side import for the neutral inker surface-frame vocabulary.
//!
//! Inker stays wgpu-free. Turnstone owns the one place that turns an acquired
//! native resource into a texture view on this window's device, and retains the
//! imported texture for the producer's resource epoch.

use inker::{
    FrameHandleOwnership, NativeTextureHandle, SurfaceError, SurfaceFrame, SurfaceTextureFormat,
};
#[cfg(feature = "scry")]
use scrying_engine::scrying::{
    Dx12FenceSynchronizer, HostWgpuContext, ImportOptions, NativeFrame, SyncMechanism,
    TextureImporter, WgpuTextureImporter, native_frame::InteropSynchronizer,
};
#[cfg(feature = "weld")]
use winit::dpi::PhysicalSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameSource {
    #[cfg(feature = "servo")]
    ServoOwned,
    #[cfg(feature = "weld")]
    RawDx12,
    #[cfg(feature = "weld")]
    WeldOwned,
    #[cfg(feature = "scry")]
    ScryOwned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameIdentity {
    source: FrameSource,
    epoch: u64,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

pub(super) struct ImportedSurfaceFrame {
    pub(super) resource_epoch: u64,
    identity: FrameIdentity,
    // A view does not retain a texture by itself, so this is the cached
    // ownership. Each composition pass asks it for a fresh view.
    texture: wgpu::Texture,
}

#[cfg(feature = "scry")]
pub(super) struct ScryingFrameImporter {
    importer: WgpuTextureImporter,
    synchronizer: std::sync::Arc<Dx12FenceSynchronizer>,
    stats: std::cell::Cell<ScryingFrameStats>,
}

#[cfg(feature = "scry")]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScryingFrameStats {
    pub(super) frames: u64,
    pub(super) imports: u64,
    pub(super) waits: u64,
}

#[cfg(all(test, feature = "scry"))]
#[path = "surface_frames/tests.rs"]
mod tests;

#[cfg(all(test, feature = "weld"))]
#[path = "surface_frames/welding_frame_tests.rs"]
mod welding_frame_tests;

#[path = "surface_frames/imported_surface_frame_presentation_alpha.rs"]
mod imported_surface_frame_presentation_alpha;
#[path = "surface_frames/scrying_frame_importer_new.rs"]
#[cfg(feature = "scry")]
mod scrying_frame_importer_new;
#[path = "surface_frames/update_imported_frame.rs"]
mod update_imported_frame;
pub(super) use update_imported_frame::update_imported_frame;
#[path = "surface_frames/update_servo_frame.rs"]
#[cfg(feature = "servo")]
mod update_servo_frame;
#[cfg(feature = "servo")]
use update_servo_frame::update_servo_frame;
#[path = "surface_frames/reuse_allocation.rs"]
mod reuse_allocation;
use reuse_allocation::reuse_allocation;
#[path = "surface_frames/prepare_scrying_frame.rs"]
#[cfg(feature = "scry")]
mod prepare_scrying_frame;
#[cfg(feature = "scry")]
use prepare_scrying_frame::prepare_scrying_frame;
#[path = "surface_frames/synchronize_reused_scrying_frame.rs"]
#[cfg(feature = "scry")]
mod synchronize_reused_scrying_frame;
#[cfg(feature = "scry")]
use synchronize_reused_scrying_frame::synchronize_reused_scrying_frame;
#[path = "surface_frames/update_scrying_frame.rs"]
#[cfg(feature = "scry")]
mod update_scrying_frame;
#[cfg(feature = "scry")]
use update_scrying_frame::update_scrying_frame;
#[path = "surface_frames/prepare_welding_frame.rs"]
#[cfg(feature = "weld")]
mod prepare_welding_frame;
#[cfg(feature = "weld")]
use prepare_welding_frame::prepare_welding_frame;
#[path = "surface_frames/validate_welding_native_frame.rs"]
#[cfg(feature = "weld")]
mod validate_welding_native_frame;
#[cfg(feature = "weld")]
use validate_welding_native_frame::validate_welding_native_frame;
#[path = "surface_frames/update_welding_frame.rs"]
#[cfg(feature = "weld")]
mod update_welding_frame;
#[cfg(feature = "weld")]
use update_welding_frame::update_welding_frame;
#[path = "surface_frames/update_raw_frame.rs"]
#[cfg(feature = "weld")]
mod update_raw_frame;
#[cfg(feature = "weld")]
use update_raw_frame::update_raw_frame;
#[path = "surface_frames/map_texture_format.rs"]
mod map_texture_format;
use map_texture_format::map_texture_format;
#[path = "surface_frames/close_if_transferred.rs"]
mod close_if_transferred;
use close_if_transferred::close_if_transferred;
#[path = "surface_frames/close_transferred_handle.rs"]
mod close_transferred_handle;
use close_transferred_handle::close_transferred_handle;
