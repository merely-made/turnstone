// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl OwnedSurfaceFrame for ServoImportedFrame {
    fn payload_kind(&self) -> &'static str {
        FRAME_KIND
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}
