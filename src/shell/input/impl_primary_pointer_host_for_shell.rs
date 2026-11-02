// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PrimaryPointerHost for Shell {
    fn pointer_state(
        &mut self,
    ) -> (
        &mut crate::surface::FocusTarget,
        &mut super::super::window_input::Pool,
        &mut Option<crate::surface::SurfaceKind>,
    ) {
        (
            &mut self.app.focus,
            &mut self.content_sessions.pool,
            &mut self.pointer_capture,
        )
    }
    fn sync_pointer_placements(&mut self) {
        self.sync_content_placements();
    }
    fn pointer_content_focus(&mut self, appearance: crate::surface::SurfaceId) {
        self.content_input(
            appearance,
            pelt_core::PeltInput::Session(inker::SessionInput::Focus(true)),
        );
    }
    fn pointer_content_mouse(
        &mut self,
        appearance: crate::surface::SurfaceId,
        local: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.content_mouse(appearance, local, phase, button);
    }
    fn pointer_window_mouse(
        &mut self,
        appearance: Option<crate::surface::SurfaceId>,
        position: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.window_mouse_input(0, appearance, position, phase, button);
    }
    fn pointer_user_agent_decision_open(&self) -> bool {
        self.app.user_agent_decision.is_open()
    }
    fn pointer_has_foreign_capture(&self) -> bool {
        super::super::window_input::captured_mouse(&self.content_sessions.pool)
            .and_then(|id| self.content_sessions.pool.placement(&id))
            .is_some_and(|placement| placement.window != 0)
            || self
                .contributed_pointer
                .is_some_and(|admitted| admitted.window != 0)
    }
    fn pointer_cancel_primary_content(&mut self) {
        // Interception retires custody; only physical Up changes button masks.
        self.touch_input
            .cancel_window(&mut self.content_sessions.pool, 0);
    }
    fn pointer_has_contributed_capture(&self) -> bool {
        self.contributed_pointer.is_some()
    }
    fn pointer_release_contributed(&mut self, x: f32, y: f32) {
        self.release_contributed_pointer(0, x, y);
    }
    fn pointer_content_hovered(&mut self, node: uuid::Uuid) {
        self.hovered_surface = Some(node);
        self.apply_pending_surface_cursor();
    }
    fn pointer_redraw(&mut self) {
        self.request_redraw();
    }
}
