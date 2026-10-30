// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "scry")]
pub(super) fn prepare_scrying_frame(
    frame: SurfaceFrame,
    fence_handle: u64,
) -> Result<(NativeFrame, FrameIdentity), SurfaceError> {
    let expected = FrameIdentity {
        source: FrameSource::ScryOwned,
        epoch: frame.resource_epoch,
        width: frame.width,
        height: frame.height,
        format: map_texture_format(&frame.format)?,
    };
    let sync = match &frame.sync {
        inker::SurfaceSyncHandle::D3d12Fence { handle, value }
            if *handle == fence_handle && *handle != 0 && *value != 0 =>
        {
            *value
        },
        _ => {
            return Err(SurfaceError::FrameAcquisitionFailed(
                "Scry frame is missing its matching nonzero host fence and value".into(),
            ));
        },
    };
    let native = scrying_engine::into_scrying_native_frame(frame).map_err(|_| {
        SurfaceError::Unsupported("Turnstone received an unknown owned surface payload".into())
    })?;
    let NativeFrame::Dx12SharedTexture(payload) = &native else {
        return Err(SurfaceError::Unsupported(
            "Turnstone's Windows Scry importer requires a D3D12 frame".into(),
        ));
    };
    let actual = FrameIdentity {
        source: FrameSource::ScryOwned,
        epoch: payload.generation,
        width: payload.size.width,
        height: payload.size.height,
        format: payload.format,
    };
    if actual != expected
        || payload.producer_sync != SyncMechanism::ExplicitFence
        || payload.fence_value != sync
    {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Scry surface envelope disagrees with its owned frame or fence".into(),
        ));
    }
    reuse_allocation(None, expected)?;
    Ok((native, expected))
}
