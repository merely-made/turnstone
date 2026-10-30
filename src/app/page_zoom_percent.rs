// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One node's REQUESTED page-zoom percentage (100 = the default scale). This
/// is what the node asked for, never what an engine settled on; the applied
/// half arrives separately, from the lanes that read their level back.
pub fn page_zoom_percent(scale: Option<f32>) -> u32 {
    (scale.unwrap_or(PAGE_ZOOM_DEFAULT).max(0.0) * 100.0).round() as u32
}
