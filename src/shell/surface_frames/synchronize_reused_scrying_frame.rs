// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "scry")]
pub(super) fn synchronize_reused_scrying_frame(
    synchronizer: &dyn InteropSynchronizer,
    native: &NativeFrame,
) -> Result<(), SurfaceError> {
    synchronizer
        .producer_complete(native, native.producer_sync())
        .map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!("Scry frame wait failed: {error}"))
        })
}
