// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "servo")]
pub(super) fn update_servo_frame(
    cached: &mut Option<ImportedSurfaceFrame>,
    frame: SurfaceFrame,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), SurfaceError> {
    let identity = FrameIdentity {
        source: FrameSource::ServoOwned,
        epoch: frame.resource_epoch,
        width: frame.width,
        height: frame.height,
        format: map_texture_format(&frame.format)?,
    };
    if !matches!(frame.sync, inker::SurfaceSyncHandle::None) {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Servo imported frame has unexpected native synchronization".into(),
        ));
    }
    let NativeTextureHandle::OwnedPayload(payload) = frame.texture else {
        return Err(SurfaceError::Unsupported(
            "Servo requires its owned imported frame".into(),
        ));
    };
    let payload = payload
        .into_any()
        .downcast::<super::super::servo::ServoImportedFrame>()
        .map_err(|_| SurfaceError::Unsupported("unknown Servo imported-frame custody".into()))?;
    let texture = payload.imported.texture;
    let actual_size = texture.size();
    if payload.device != *device
        || payload.queue != *queue
        || payload.resource_epoch != identity.epoch
        || payload.imported.origin != grafting::TextureOrigin::TopLeft
        || payload.imported.format != identity.format
        || texture.format() != identity.format
        || (payload.imported.size.width, payload.imported.size.height)
            != (identity.width, identity.height)
        || (
            actual_size.width,
            actual_size.height,
            actual_size.depth_or_array_layers,
        ) != (identity.width, identity.height, 1)
        || !texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Servo frame disagrees with its host device, origin, epoch or texture metadata".into(),
        ));
    }
    reuse_allocation(None, identity)?;
    // The producer's pre-present hook already imported and normalized this
    // paint on this exact device. Retain that custody through composition.
    *cached = Some(ImportedSurfaceFrame {
        resource_epoch: identity.epoch,
        identity,
        texture,
    });
    Ok(())
}
