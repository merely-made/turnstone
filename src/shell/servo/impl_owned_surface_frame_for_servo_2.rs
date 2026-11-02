// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(target_os = "linux")]
impl OwnedSurfaceFrame for ServoDmaBufFrame {
    fn payload_kind(&self) -> &'static str {
        FRAME_KIND
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}
