// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Compatibility only for callers that have not yet crossed the A2 routing
/// boundary. It dereferences to the active runtime's canvas, while new code
/// must use the explicit graph lookup above.
impl Deref for GraphRuntimePool {
    type Target = Canvas;

    fn deref(&self) -> &Self::Target {
        self.active_canvas()
    }
}
