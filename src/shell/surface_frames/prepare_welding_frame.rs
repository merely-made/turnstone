// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "weld")]
pub(super) fn prepare_welding_frame(
    frame: SurfaceFrame,
) -> Result<(welding::native_frame::Dx12SharedTexture, FrameIdentity), SurfaceError> {
    if !matches!(&frame.sync, inker::SurfaceSyncHandle::None) {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Weld callback frame requires completed synchronization".into(),
        ));
    }
    let expected = FrameIdentity {
        source: FrameSource::WeldOwned,
        epoch: frame.resource_epoch,
        width: frame.width,
        height: frame.height,
        format: map_texture_format(&frame.format)?,
    };
    let native = weld_engine::into_welding_native_frame(frame).map_err(|_| {
        SurfaceError::Unsupported("Turnstone received an unknown Weld owned payload".into())
    })?;
    let welding::NativeFrame::Dx12SharedTexture(native) = native else {
        return Err(SurfaceError::Unsupported(
            "Windows Weld requires a D3D12 frame".into(),
        ));
    };
    validate_welding_native_frame(native, expected)
}
