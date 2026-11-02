// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The one composite step every scene builder ends with.
pub(super) fn composite<P: PaintList>(viewport: DeviceIntSize, plist: &P) -> netrender::Scene {
    match composite_resources(viewport, plist) {
        Ok(translated) => translated.scene,
        Err(error) => pane_scene(
            &format!("Surface cannot render: {error}"),
            viewport.width.max(1) as u32,
            viewport.height.max(1) as u32,
        ),
    }
}
