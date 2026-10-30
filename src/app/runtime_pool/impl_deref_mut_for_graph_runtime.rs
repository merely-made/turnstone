// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl DerefMut for GraphRuntimePool {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active_canvas_mut()
    }
}
