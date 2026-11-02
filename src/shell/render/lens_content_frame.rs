// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Preserve the pool's document or native frame result for lens composition.
pub(in super::super) fn lens_content_frame(
    pool: &mut pelt_core::PeltControllerPool<
        super::super::content_pool::ContentKey,
        crate::surface::SurfaceId,
        Scene,
    >,
    appearance: &crate::surface::SurfaceId,
) -> Option<pelt_core::PeltLayer<Scene>> {
    pool.frame(appearance)
}
