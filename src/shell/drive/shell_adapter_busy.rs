// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn adapter_busy(&mut self) -> Option<bool> {
        if super::super::appearance_editor::workshop_pending(
            self.theme_editor_requested,
            self.theme_editor.is_some(),
        ) {
            // Shared `wait` yields to winit while the child owns native work.
            // The legacy synchronous file wait cannot drive a child window.
            return Some(true);
        }
        if self.pending_capture.is_some()
            || self.pending_lens_capture.is_some()
            || self.pending_capture_readback.is_some()
        {
            return Some(true);
        }
        // Keep the hot `wait` poll allocation-free and short-circuiting. The
        // explicit `record-idle` and content-ready assertions call
        // `idle_diagnosis` when a receipt needs every concurrent cause.
        if self.pending_fetches.any_in_flight() {
            return Some(true);
        }
        if self.app.content.any_requested() {
            return Some(true);
        }
        if self.app.graph_runtimes.is_settling() {
            return Some(true);
        }
        // Wait polls also advance clocks when a hidden finite animation has
        // stopped asking the render loop for frames.
        let _ = self.pump_visible_documents();
        Some(
            self.content_sessions
                .pool
                .pending_contents()
                .next()
                .is_some(),
        )
    }

    pub(super) fn adapter_press(&mut self, x: f32, y: f32) {
        self.deliver_press(x, y, MouseButton::Left);
    }

    pub(super) fn adapter_moved(&mut self, x: f32, y: f32) {
        self.deliver_move(x, y);
    }

    pub(super) fn adapter_release(&mut self, x: f32, y: f32) {
        self.deliver_release(x, y, MouseButton::Left);
    }
}
