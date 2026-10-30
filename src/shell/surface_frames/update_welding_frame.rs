// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "weld")]
pub(super) fn update_welding_frame(
    cached: &mut Option<ImportedSurfaceFrame>,
    frame: SurfaceFrame,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), SurfaceError> {
    let (native, identity) = prepare_welding_frame(frame)?;
    reuse_allocation(cached.as_ref().map(|existing| existing.identity), identity)?;
    // Callback handoff requires Weld's cache-visible read on every paint,
    // including another write to the same allocation epoch.
    let host = welding::HostWgpuContext::new(device.clone(), queue.clone());
    let imported = welding::WgpuTextureImporter::import_owned_dx12_callback_frame(native, &host)
        .map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!("Weld frame import failed: {error}"))
        })?;
    *cached = Some(ImportedSurfaceFrame {
        resource_epoch: identity.epoch,
        identity,
        texture: imported.texture,
    });
    Ok(())
}
