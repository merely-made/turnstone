// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceProducer for TurnstoneScryProducer {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.inner.resize(width, height)
    }

    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.inner.set_offset(x, y)
    }

    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.inner.acquire_frame()
    }

    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError> {
        self.inner.send_mouse_input(ev)
    }

    fn send_pointer_input(&mut self, ev: PointerEvent) -> Result<(), SurfaceError> {
        if ev.pointer_type == PointerType::Mouse {
            // WebView2 SendPointerInput accepts touch and pen only. Bypass
            // Inker's older MouseEvent shape, which lacks modifiers and Leave.
            let input = mouse_pointer_input(&ev)?;
            tracing::debug!(target: "turnstone::scry_input", ?input, "begin native mouse delivery");
            let result = self
                .producer
                .borrow()
                .send_mouse_input(input)
                .map_err(scrying_engine::translation::map_error);
            tracing::debug!(target: "turnstone::scry_input", ok = result.is_ok(), "finished native mouse delivery");
            result
        } else {
            self.inner.send_pointer_input(ev)
        }
    }

    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError> {
        self.inner.send_keyboard_input(ev)
    }

    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        tracing::debug!(target: "turnstone::scry_input", ?reason, "begin native focus delivery");
        let result = self.inner.move_focus(reason);
        tracing::debug!(target: "turnstone::scry_input", ok = result.is_ok(), "finished native focus delivery");
        result
    }

    fn set_focus(&mut self, focused: bool) -> Result<(), SurfaceError> {
        self.inner.set_focus(focused)
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.inner.poll_cursor_shape()
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        self.inner.apply_settings(settings)
    }

    // Rehosting would move the private capture HWND into a product window or
    // fail while the root is shared. Retain the trait's explicit refusal.
    fn as_web_surface(&mut self) -> Option<&mut dyn WebSurface> {
        Some(self)
    }
}
