// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceProducer for Page {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.record(format!("resize {width}x{height}"))
    }

    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        if self.fail_offset == Some((x, y)) {
            return Err(SurfaceError::InputFailed("offset rejected".into()));
        }
        Ok(())
    }

    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.acquisitions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(None)
    }

    fn send_mouse_input(&mut self, _event: MouseEvent) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn send_pointer_input(&mut self, event: inker::PointerEvent) -> Result<(), SurfaceError> {
        self.pointers.lock().unwrap().push(event.clone());
        self.record(format!(
            "pointer {:?} {:?} ctrl={} at={},{}",
            event.phase, event.button, event.modifiers.ctrl, event.position.x, event.position.y
        ))
    }

    fn send_drag_input(&mut self, event: inker::DragEvent) -> Result<(), SurfaceError> {
        self.record(format!("drag {event:?}"))
    }

    fn send_wheel_input(&mut self, event: inker::SurfaceWheelEvent) -> Result<(), SurfaceError> {
        self.record(format!("wheel {event:?}"))
    }

    fn send_keyboard_input(&mut self, event: KeyboardEvent) -> Result<(), SurfaceError> {
        self.record(format!("keyboard {event:?}"))
    }

    fn move_focus(&mut self, _reason: FocusReason) -> Result<(), SurfaceError> {
        self.record("focus")
    }

    fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
        None
    }

    fn apply_settings(&mut self, _settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn as_web_surface(&mut self) -> Option<&mut dyn WebSurface> {
        Some(self)
    }
}
