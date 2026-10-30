// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PrimaryPaneCaptureFixture {
    pub(super) fn new() -> Self {
        Self {
            content: CallerCaptureFixture::new(0, None),
            capture: None,
            delivered: Vec::new(),
            cancellation_capture: Vec::new(),
        }
    }

    pub(super) fn pane_position(&self) -> PhysicalPosition {
        let plan = self.content.plan();
        for surface in &plan {
            if !matches!(surface.kind, SurfaceKind::Pane(_)) {
                continue;
            }
            // Find an exposed ordinary Pane in the real primary plan, not a
            // synthetic rectangle placed over content solely for this test.
            for dy in [1., 8., surface.rect.h / 2., surface.rect.h - 1.] {
                for dx in [1., 8., surface.rect.w / 2., surface.rect.w - 1.] {
                    let position = PhysicalPosition {
                        x: surface.rect.x + dx,
                        y: surface.rect.y + dy,
                    };
                    if crate::surface::hit_test(
                        &plan,
                        self.content.app.focus,
                        position.x,
                        position.y,
                    )
                    .is_some_and(|hit| hit.id == surface.id)
                    {
                        return position;
                    }
                }
            }
        }
        panic!("actual primary plan must expose an ordinary pane beside content");
    }

    pub(super) fn press(&mut self, position: PhysicalPosition, button: winit::event::MouseButton) {
        use super::super::super::input::{
            PrimaryPointerHost, dispatch_primary_press, surface_mouse_button,
        };
        // This is Shell::deliver_press's native bookkeeping before admission.
        self.pointer_window_mouse(
            None,
            (position.x, position.y),
            PointerPhase::Down,
            surface_mouse_button(button),
        );
        let hit = crate::surface::hit_test(
            &self.content.plan(),
            self.content.app.focus,
            position.x,
            position.y,
        );
        // The shared production dispatch contains the unconditional capture
        // assignment AND the Content/non-Content decision implicated by I3.
        if let Some(hit) = dispatch_primary_press(self, hit, position.x, position.y, button) {
            let SurfaceKind::Pane(pane) = hit.kind else {
                panic!("this fixture exercises only admitted Content and ordinary Pane hits");
            };
            assert_eq!(button, winit::event::MouseButton::Middle);
            // Middle has no pane renderer action. Match its App effects from
            // the continuation in Shell::deliver_press.
            self.content.app.active_pane = Some(pane);
            self.content.app.raise_floating_pane(pane);
        }
    }

    pub(super) fn move_pointer(&mut self, position: PhysicalPosition) {
        // At the Pane position, Shell's non-content fallthrough has no page
        // hover receiver. Captured-content movement must go through this gate.
        super::super::super::input::dispatch_primary_content_move(self, position.x, position.y);
    }

    pub(super) fn release(
        &mut self,
        position: PhysicalPosition,
        button: winit::event::MouseButton,
    ) {
        let _ = super::super::super::input::dispatch_primary_release(
            self, position.x, position.y, button,
        );
        // Ordinary Middle-pane continuation has no contributed renderer or
        // graph/divider/workbench drag to finish in this fixture.
    }

    pub(super) fn focus_lost(&mut self) {
        // Actual pool focus-loss entry used by Shell::native_input_focus.
        // Window bookkeeping matches that caller; no synthetic Up is sent.
        TouchInput::default().focus_lost(&mut self.content.contents.pool, 0);
        self.content.buttons.remove(&0);
        self.capture = None;
        self.content.sync(false);
    }

    pub(super) fn record(&mut self, routed: Option<(ContentKey, PeltHostEffect)>) {
        if let Some((receiver, effect)) = routed {
            self.delivered.push((receiver, effect.handled));
        }
    }
}
