// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "weld")]
pub(super) fn validate_welding_native_frame(
    native: welding::native_frame::Dx12SharedTexture,
    expected: FrameIdentity,
) -> Result<(welding::native_frame::Dx12SharedTexture, FrameIdentity), SurfaceError> {
    let actual = FrameIdentity {
        source: FrameSource::WeldOwned,
        epoch: native.generation(),
        width: native.size().width,
        height: native.size().height,
        format: native.format(),
    };
    if expected != actual {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Weld surface envelope disagrees with its owned native frame".into(),
        ));
    }
    reuse_allocation(None, expected)?;
    Ok((native, expected))
}
