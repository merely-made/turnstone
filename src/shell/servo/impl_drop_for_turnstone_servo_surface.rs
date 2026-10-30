// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for TurnstoneServoSurface {
    fn drop(&mut self) {
        self.delegate.deactivate_accessibility();
        if let Some(view) = &self.view {
            view.set_accessibility_active(false);
        }
        drop(self.view.take());
        self.root.servo.spin_event_loop();
        drop(self.interop.take());
        self.root.views.set(self.root.views.get() - 1);
    }
}
