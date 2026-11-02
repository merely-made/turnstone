// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl WebSurfaceProducer for SharedNativeProducer {
    fn capabilities(&self) -> native::WebSurfaceCapabilities {
        WebSurfaceProducer::capabilities(&*self.0.borrow())
    }
    fn can_go_back(&self) -> bool {
        WebSurfaceProducer::can_go_back(&*self.0.borrow())
    }
    fn can_go_forward(&self) -> bool {
        WebSurfaceProducer::can_go_forward(&*self.0.borrow())
    }
    forward_native!(acquire_frame() -> Result<native::WebSurfaceFrame, native::WebSurfaceError>);
    fn try_acquire_frame(
        &mut self,
    ) -> Result<Option<native::WebSurfaceFrame>, native::WebSurfaceError> {
        self.drive_keyboard()?;
        WebSurfaceProducer::try_acquire_frame(&mut *self.0.borrow_mut())
    }
    forward_native!(restart_capture_after_stall() -> Result<(), native::WebSurfaceError>);
    forward_native!(resize(size: PhysicalSize<u32>) -> Result<(), native::WebSurfaceError>);
    forward_native!(set_offset(x: f32, y: f32) -> Result<(), native::WebSurfaceError>);
    forward_native!(
        send_mouse_input(event: native::MouseInput) -> Result<(), native::WebSurfaceError>
    );
    forward_native!(
        send_pointer_input(event: native::PointerInput) -> Result<(), native::WebSurfaceError>
    );
    fn send_keyboard_input(
        &mut self,
        event: native::KeyboardInput,
    ) -> Result<(), native::WebSurfaceError> {
        self.1.borrow_mut().enqueue(keyboard_commands(&event)?)?;
        self.drive_keyboard()
    }
    forward_native!(move_focus(reason: native::FocusReason) -> Result<(), native::WebSurfaceError>);
    forward_native!(poll_cursor_shape() -> Option<native::CursorShape>);
    forward_native!(
        apply_settings(
            settings: &native::WebSurfaceSettings,
        ) -> Result<(), native::WebSurfaceError>
    );
    forward_native!(load_url(url: &str) -> Result<(), native::WebSurfaceError>);
    forward_native!(load_html(html: &str) -> Result<(), native::WebSurfaceError>);
    forward_native!(reload() -> Result<(), native::WebSurfaceError>);
    forward_native!(stop() -> Result<(), native::WebSurfaceError>);
    forward_native!(go_back() -> Result<bool, native::WebSurfaceError>);
    forward_native!(go_forward() -> Result<bool, native::WebSurfaceError>);
    forward_native!(set_cookie(cookie: &native::Cookie) -> Result<(), native::WebSurfaceError>);
    forward_native!(
        request_cookies_for_url(
            id: native::WebRequestId,
            url: &str,
        ) -> Result<(), native::WebSurfaceError>
    );
    forward_native!(
        request_script_result(
            id: native::WebRequestId,
            script: &str,
        ) -> Result<(), native::WebSurfaceError>
    );
    fn poll_web_surface_event(&mut self) -> Option<native::WebSurfaceEvent> {
        // Completion errors stay latched for the next fallible acquire/send;
        // this event-only API cannot report SurfaceError.
        let _ = self.drive_keyboard();
        WebSurfaceProducer::poll_web_surface_event(&mut *self.0.borrow_mut())
    }
}
