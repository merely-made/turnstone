// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn reuse_allocation(
    cached: Option<FrameIdentity>,
    incoming: FrameIdentity,
) -> Result<bool, SurfaceError> {
    if incoming.width == 0 || incoming.height == 0 {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "surface producer emitted an empty texture".into(),
        ));
    }
    let Some(cached) = cached else {
        return Ok(false);
    };
    if cached.source != incoming.source || cached.epoch != incoming.epoch {
        return Ok(false);
    }
    if cached != incoming {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "surface producer reused a resource epoch with changed texture metadata".into(),
        ));
    }
    Ok(true)
}
