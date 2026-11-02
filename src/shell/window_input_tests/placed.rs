// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn placed(
    content: ContentKey,
    window: u64,
    x: f32,
    focused: bool,
) -> pelt_core::PeltPlacement<ContentKey> {
    pelt_core::PeltPlacement {
        content,
        window,
        rect: pelt_core::WorkspaceRect::new(x, 0., 100., 100.),
        visible: true,
        focused,
        scale_factor: 1.,
    }
}
