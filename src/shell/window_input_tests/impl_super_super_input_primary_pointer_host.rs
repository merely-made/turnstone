// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl super::super::super::input::PrimaryPointerHost for PrimaryPaneCaptureFixture {
    fn pointer_state(&mut self) -> (&mut FocusTarget, &mut Pool, &mut Option<SurfaceKind>) {
        (
            &mut self.content.app.focus,
            &mut self.content.contents.pool,
            &mut self.capture,
        )
    }
    fn sync_pointer_placements(&mut self) {
        self.content.sync(true);
    }
    fn pointer_content_focus(&mut self, appearance: SurfaceId) {
        self.content.sync(true);
        self.content.contents.pool.input(
            &appearance,
            PeltInput::Session(inker::SessionInput::Focus(true)),
        );
    }
    fn pointer_content_mouse(
        &mut self,
        appearance: SurfaceId,
        local: (f32, f32),
        phase: PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.content.sync(true);
        let placement = self
            .content
            .contents
            .pool
            .placement(&appearance)
            .unwrap()
            .clone();
        let routed = content_mouse_input(
            &mut self.content.contents.pool,
            &self.content.buttons,
            placement.window,
            Some(appearance),
            PhysicalPosition {
                x: local.0 + placement.rect.x,
                y: local.1 + placement.rect.y,
            },
            phase,
            button,
            Default::default(),
        );
        self.record(routed);
    }
    fn pointer_window_mouse(
        &mut self,
        appearance: Option<SurfaceId>,
        position: (f32, f32),
        phase: PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        let routed = native_mouse_input(
            &mut self.content.contents.pool,
            &mut self.content.buttons,
            0,
            appearance,
            PhysicalPosition {
                x: position.0,
                y: position.1,
            },
            phase,
            button,
            Default::default(),
        );
        self.record(routed);
    }
    fn pointer_user_agent_decision_open(&self) -> bool {
        self.content.app.user_agent_decision.is_open()
    }
    fn pointer_has_foreign_capture(&self) -> bool {
        captured_mouse(&self.content.contents.pool)
            .and_then(|id| self.content.contents.pool.placement(&id))
            .is_some_and(|placement| placement.window != 0)
    }
    fn pointer_cancel_primary_content(&mut self) {
        self.cancellation_capture.push(self.capture);
        TouchInput::default().focus_lost(&mut self.content.contents.pool, 0);
    }
    fn pointer_has_contributed_capture(&self) -> bool {
        false
    }
    fn pointer_release_contributed(&mut self, _: f32, _: f32) {
        panic!("ordinary Pane fixture cannot hold contributed renderer custody");
    }
    fn pointer_content_hovered(&mut self, _: uuid::Uuid) {}
    fn pointer_redraw(&mut self) {}
}
