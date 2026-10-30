// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl LensPaneCustodyFixture {
    pub(super) fn new(foreign_primary: bool, extra_lens_pane: bool) -> Self {
        let mut content = CallerCaptureFixture::new(1, None);
        if extra_lens_pane || foreign_primary {
            if foreign_primary {
                content.app.active_pane = Some(content.app.default_graph_pane());
            }
            content.app.update(crate::action::Action::SummonPane(
                crate::panes::PaneKindId::new(crate::panes::kind::ROSTER),
            ));
            let plan = content.plan();
            content.surfaces = content.nodes.map(|node| {
                *plan
                    .iter()
                    .find(|surface| surface.kind == SurfaceKind::Content(node))
                    .expect("actual Lens content remains exposed")
            });
        }
        let owner_window = if foreign_primary { 0 } else { 1 };
        let owner_plan = if foreign_primary {
            super::super::super::primary_content_plan(&content.app, &content.contents, 1200, 900)
        } else {
            content.plan()
        };
        let surface = *owner_plan
            .iter()
            .find(|surface| matches!(surface.kind, SurfaceKind::Pane(_)))
            .expect("actual owner window exposes a retained pane");
        let SurfaceKind::Pane(pane) = surface.kind else {
            unreachable!()
        };
        content.app.focus = FocusTarget::Pane(pane);
        content.sync(true);
        let position = PhysicalPosition {
            x: surface.rect.x + 1.,
            y: surface.rect.y + 1.,
        };
        assert!(
            native_mouse_input(
                &mut content.contents.pool,
                &mut content.buttons,
                owner_window,
                None,
                position,
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                Default::default()
            )
            .is_none()
        );
        let admitted = super::super::super::input::ContributedPointer {
            surface,
            generation: 77,
            window: owner_window,
        };
        Self {
            content,
            admitted: Some(admitted),
            capture: Some(surface.kind),
            pane_events: vec![(PointerPhase::Down, owner_window, (1., 1.))],
            generation: 77,
        }
    }

    pub(super) fn actual_owner_plan(&self, window: u64) -> Vec<Surface> {
        if window == 0 {
            super::super::super::primary_content_plan(
                &self.content.app,
                &self.content.contents,
                1200,
                900,
            )
        } else {
            self.content.plan()
        }
    }

    pub(super) fn press(
        &mut self,
        window: u64,
        position: PhysicalPosition,
        button: winit::event::MouseButton,
    ) -> bool {
        assert!(
            native_mouse_input(
                &mut self.content.contents.pool,
                &mut self.content.buttons,
                window,
                None,
                position,
                PointerPhase::Down,
                super::super::super::input::surface_mouse_button(button),
                Default::default()
            )
            .is_none()
        );
        if super::super::super::lens::reject_lens_pane_press(self) {
            return true;
        }
        let plan = self.content.plan();
        let graphfocus = FocusTarget::Graph(self.content.app.default_graph_pane());
        let Some(hit) = super::super::super::lens::lens_press_hit(
            &self.content.contents.pool,
            window,
            &plan,
            graphfocus,
            position,
        ) else {
            return false;
        };
        match hit.kind {
            SurfaceKind::Content(_) => {
                self.content.sync(true);
                let hit = content_press_target(
                    &mut self.content.app.focus,
                    &self.content.contents.pool,
                    window,
                    position,
                    hit,
                );
                self.capture = Some(hit.kind);
                self.content.sync(true);
                let routed = content_mouse_input(
                    &mut self.content.contents.pool,
                    &self.content.buttons,
                    window,
                    Some(hit.id),
                    position,
                    PointerPhase::Down,
                    super::super::super::input::surface_mouse_button(button),
                    Default::default(),
                );
                assert!(routed.is_some_and(|(_, effect)| effect.handled));
                true
            },
            SurfaceKind::Pane(pane) if button == winit::event::MouseButton::Left => {
                // Renderer adapter boundary in the actual Lens Pane arm: it
                // retains the exact real-plan surface and receiver generation.
                self.content.app.focus = FocusTarget::Pane(pane);
                self.capture = Some(hit.kind);
                self.admitted = Some(super::super::super::input::ContributedPointer {
                    surface: *plan.iter().find(|surface| surface.id == hit.id).unwrap(),
                    generation: self.generation,
                    window,
                });
                self.pane_events
                    .push((PointerPhase::Down, window, hit.local));
                true
            },
            _ => false,
        }
    }

    pub(super) fn release(
        &mut self,
        window: u64,
        position: PhysicalPosition,
        button: winit::event::MouseButton,
    ) -> bool {
        // Same production Lens prelude: while a pane owns the mouse, terminal
        // bookkeeping is physical only; the pane terminal seam owns delivery.
        let appearance = if self.admitted.is_none() {
            captured_mouse(&self.content.contents.pool)
        } else {
            None
        };
        native_mouse_input(
            &mut self.content.contents.pool,
            &mut self.content.buttons,
            window,
            appearance,
            position,
            PointerPhase::Up,
            super::super::super::input::surface_mouse_button(button),
            Default::default(),
        );
        super::super::super::lens::dispatch_lens_pane_release(
            self,
            window,
            (position.x, position.y),
            button,
        )
    }

    pub(super) fn assert_retained(&self, original: super::super::super::input::ContributedPointer) {
        assert_eq!(self.admitted.unwrap().surface, original.surface);
        assert_eq!(self.admitted.unwrap().generation, original.generation);
        assert_eq!(self.admitted.unwrap().window, original.window);
        assert_eq!(self.capture, Some(original.surface.kind));
        let SurfaceKind::Pane(pane) = original.surface.kind else {
            unreachable!()
        };
        assert_eq!(self.content.app.focus, FocusTarget::Pane(pane));
        assert_eq!(
            self.pane_events.len(),
            1,
            "a chord cannot admit a second retained pane Down"
        );
        assert_eq!(
            captured_mouse(&self.content.contents.pool),
            None,
            "a pane-owned chord cannot admit a competing content receiver"
        );
        assert!(
            self.content
                .pointers
                .iter()
                .all(|events| events.lock().unwrap().is_empty())
        );
    }

    pub(super) fn fresh_content_after_terminal(&mut self) {
        let pos = self.content.position(0);
        assert!(self.press(1, pos, winit::event::MouseButton::Right));
        assert!(self.admitted.is_none());
        assert_eq!(
            captured_mouse(&self.content.contents.pool),
            Some(self.content.surfaces[0].id)
        );
        assert!(!self.release(1, pos, winit::event::MouseButton::Right));
        assert_eq!(captured_mouse(&self.content.contents.pool), None);
        assert_eq!(self.content.buttons[&1], PointerButtons::NONE);
        assert_eq!(
            self.content.pointers[0]
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.phase)
                .collect::<Vec<_>>(),
            vec![PointerPhase::Down, PointerPhase::Up]
        );
    }
}
