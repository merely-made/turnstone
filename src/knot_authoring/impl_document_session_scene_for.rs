// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl DocumentSession<Scene> for KnotDocumentSession {
    fn frame(&mut self, width: u32, height: u32) -> Scene {
        self.drain_events();
        if (self.runner.state().width, self.runner.state().height) != (width, height) {
            self.runner.update(|state| {
                state.width = width;
                state.height = height;
            });
        }
        let sheet = format!("{} {}", crate::ui::CAMBIUM_SHEET, KNOT_SHEET);
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            &sheet,
            width,
            height,
            &mut self.scroll,
        )
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        let before = self.scroll.offset();
        self.scroll.nudge(dx, dy);
        let (width, height) = (self.runner.state().width, self.runner.state().height);
        let _ = self.frame(width, height);
        self.scroll.offset() != before
    }

    fn scroll_for_key(&mut self, _key: SessionScrollKey) -> bool {
        false
    }

    /// Keys reach the editor through its Cambium key dispatch, the focused
    /// channel a pane's keys take, so the browsing controller's neutral input
    /// drives Knot as it drives every other document. A key the editor holds
    /// no focus for is left to the host.
    fn key_input(
        &mut self,
        key: SessionKey,
        state: SessionButtonState,
        modifiers: SessionModifiers,
        _repeat: bool,
    ) -> SessionEffect {
        if state != SessionButtonState::Pressed {
            return SessionEffect::Ignored;
        }
        let Some(event) = cambium_key(key, modifiers) else {
            return SessionEffect::Ignored;
        };
        let handled = self.dispatch_key(event);
        if let Some(address) = self.run_authoring_requests() {
            SessionEffect::Navigate(address)
        } else if handled {
            SessionEffect::Handled
        } else {
            SessionEffect::Ignored
        }
    }

    fn text_input(&mut self, text: &str) -> bool {
        self.dispatch_key(KeyEvent::new(Key::Composition(CompositionEvent::Commit(
            text.to_owned(),
        ))))
    }

    fn ime_input(&mut self, ime: SessionIme) -> bool {
        let composition = match ime {
            SessionIme::Enabled => CompositionEvent::Enabled,
            SessionIme::Preedit { text, selection } => {
                CompositionEvent::Preedit { text, selection }
            },
            SessionIme::Commit(text) => CompositionEvent::Commit(text),
            SessionIme::Disabled => CompositionEvent::Disabled,
        };
        self.dispatch_key(KeyEvent::new(Key::Composition(composition)))
    }

    fn editable_focus(&self) -> bool {
        self.runner.focus().is_some()
    }

    fn click_at(&mut self, x: f32, y: f32) -> SessionClick {
        let result = self.pointer_down(x, y);
        self.pointer_up(x, y);
        result
    }

    fn pointer_down(&mut self, x: f32, y: f32) -> SessionClick {
        self.cancel_pointer();
        self.drain_events();
        let size = (self.runner.state().width, self.runner.state().height);
        let sheet = format!("{} {}", crate::ui::CAMBIUM_SHEET, KNOT_SHEET);
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            &sheet,
            size.0,
            size.1,
            x,
            y,
            &self.scroll,
        );
        let Some(node) = hit else {
            return SessionClick::Miss;
        };
        let event = self.pointer_event(node, PointerPhase::Down, x, y);
        self.runner
            .dispatch_click(node, PointerClick::at(event.local));
        let mut prevented = self.runner.default_prevented();
        if let Some(target) = self.runner.pointer_target(node) {
            let event = self.pointer_event(target, PointerPhase::Down, x, y);
            self.runner.dispatch_pointer_down(node, event);
            prevented |= self.runner.default_prevented();
        }
        if !prevented {
            self.text_drag = self.runner.focus().filter(|focus| {
                use layout_dom_api::LayoutDom;
                let dom = self.dom.borrow();
                let mut current = Some(node);
                while let Some(hit) = current {
                    if hit == *focus {
                        return self.layout.editable_focus(&dom, Some(*focus));
                    }
                    current = dom.parent(hit);
                }
                false
            });
            self.drag_selection_to(x, y, false);
        }
        tracing::debug!(target: "turnstone::knot_pointer", x, y, ?node, drag = ?self.text_drag, selection = ?self.runner.state().editor.selection(), "pointer down");
        if let Some(address) = self.run_authoring_requests() {
            return SessionClick::Navigate(address);
        }
        SessionClick::Handled
    }

    fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        let captured = self.runner.pointer_capture();
        if let Some(node) = captured {
            let event = self.pointer_event(node, PointerPhase::Move, x, y);
            self.runner.dispatch_pointer_move(event);
        }
        let changed = self.drag_selection_to(x, y, true);
        tracing::debug!(target: "turnstone::knot_pointer", x, y, changed, drag = ?self.text_drag, selection = ?self.runner.state().editor.selection(), "pointer move");
        changed || captured.is_some()
    }

    fn pointer_up(&mut self, x: f32, y: f32) -> SessionClick {
        let handled = self.text_drag.is_some() || self.runner.pointer_capture().is_some();
        self.pointer_move(x, y);
        self.text_drag = None;
        if let Some(node) = self.runner.pointer_capture() {
            let event = self.pointer_event(node, PointerPhase::Up, x, y);
            self.runner.dispatch_pointer_up(event);
        }
        if let Some(address) = self.run_authoring_requests() {
            return SessionClick::Navigate(address);
        }
        if handled {
            SessionClick::Handled
        } else {
            SessionClick::Miss
        }
    }

    fn cancel_input(&mut self) -> bool {
        self.cancel_pointer()
    }

    fn focus_input(&mut self, focused: bool) {
        if !focused {
            self.cancel_pointer();
        }
    }

    fn links(&self) -> Vec<SessionLink> {
        Vec::new()
    }

    fn settled(&mut self) -> bool {
        self.drain_events();
        !matches!(
            self.runner.state().status,
            AuthoringStatus::Saving
                | AuthoringStatus::Resolving
                | AuthoringStatus::Running
                | AuthoringStatus::Reloading
        )
    }

    fn inspect(&self) -> Option<ContentReport> {
        let outline = self
            .runner
            .state()
            .editor
            .outline()
            .into_iter()
            .map(|item| OutlineEntry {
                depth: item.level.saturating_sub(1) as usize,
                role: "heading",
                name: item.text,
            })
            .collect::<Vec<_>>();
        Some(ContentReport {
            title: Some(self.address.clone()),
            headings: outline.iter().map(|entry| entry.name.clone()).collect(),
            outline,
            links: Vec::new(),
            lineage: None,
        })
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
