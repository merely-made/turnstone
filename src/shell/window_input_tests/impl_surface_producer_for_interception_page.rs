// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceProducer for InterceptionPage {
    fn resize(&mut self, w: u32, h: u32) -> Result<(), SurfaceError> {
        self.page.resize(w, h)
    }
    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.page.set_offset(x, y)
    }
    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.page.acquire_frame()
    }
    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError> {
        self.page.send_mouse_input(ev)
    }
    fn send_pointer_input(&mut self, ev: inker::PointerEvent) -> Result<(), SurfaceError> {
        self.lifetime
            .lock()
            .unwrap()
            .push(format!("{:?}", ev.phase));
        self.page.send_pointer_input(ev)
    }
    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError> {
        self.page.send_keyboard_input(ev)
    }
    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        self.page.move_focus(reason)
    }
    fn set_focus(&mut self, focused: bool) -> Result<(), SurfaceError> {
        self.lifetime
            .lock()
            .unwrap()
            .push(format!("focus {focused}"));
        Ok(())
    }
    fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
        None
    }
    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        self.page.apply_settings(settings)
    }
}
