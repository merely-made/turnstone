// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The practice document session: input, focus and frames for one recording.

use super::*;

pub(super) type Runner = GenetAppRunner<State, fn(&State) -> view::PracticeView, view::PracticeView, ()>;

pub(super) struct PracticeSession {
    pub(super) address: String,
    pub(super) size: (u32, u32),
    pub(super) dom: DomHandle,
    pub(super) runner: Runner,
    pub(super) layout: RetainedLayout,
    pub(super) scroll: PaneScroll,
}

impl PracticeSession {
    fn textbox_at(&self, x: f32, y: f32) -> Option<genet_scripted_dom::NodeId> {
        let dom = self.dom.borrow();
        self.runner.focusables().into_iter().find(|node| {
            is_textbox(&dom, *node)
                && self.layout.visible_rect(&dom, *node).is_some_and(
                    |(left, top, width, height)| {
                        x >= left && x < left + width && y >= top && y < top + height
                    },
                )
        })
    }
}

fn is_textbox(dom: &ScriptedDom, node: genet_scripted_dom::NodeId) -> bool {
    dom.attribute(node, &Namespace::default(), &LocalName::from("role")) == Some("textbox")
}

impl DocumentSession<Scene> for PracticeSession {
    fn inspect(&self) -> Option<inker::ContentReport> {
        let title = self.runner.state().title.clone();
        Some(inker::ContentReport {
            title: Some(title.clone()),
            headings: vec![title],
            ..Default::default()
        })
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn links(&self) -> Vec<inker::SessionLink> {
        Vec::new()
    }

    fn current_address(&self) -> Option<String> {
        Some(self.address.clone())
    }

    fn frame(&mut self, width: u32, height: u32) -> Scene {
        self.size = (width, height);
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            view::SHEET,
            width,
            height,
            &mut self.scroll,
        )
    }

    fn click_at(&mut self, x: f32, y: f32) -> SessionClick {
        let (w, h) = self.size;
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            view::SHEET,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        // The empty controlled field has a visible border box but can lack a
        // painted text fragment. Focus the box using the same retained layout
        // that paints it, including its clip and current scroll position.
        if let Some(field) = self.textbox_at(x, y) {
            self.runner.set_focus(Some(field));
            return SessionClick::Handled;
        }
        if let Some(node) = hit {
            self.runner.dispatch_click(node, PointerClick::at((x, y)));
            SessionClick::Handled
        } else {
            SessionClick::Miss
        }
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        let before = self.scroll.offset();
        self.scroll.nudge(dx, dy);
        let (width, height) = self.size;
        let _ = self.frame(width, height);
        self.scroll.offset() != before
    }

    fn scroll_for_key(&mut self, key: SessionScrollKey) -> bool {
        let distance = self.size.1 as f32 * 0.8;
        match key {
            SessionScrollKey::LineDown => self.scroll_by(0.0, 42.0),
            SessionScrollKey::LineUp => self.scroll_by(0.0, -42.0),
            SessionScrollKey::PageDown => self.scroll_by(0.0, distance),
            SessionScrollKey::PageUp => self.scroll_by(0.0, -distance),
            _ => false,
        }
    }

    fn key_input(
        &mut self,
        key: SessionKey,
        state: SessionButtonState,
        modifiers: SessionModifiers,
        _: bool,
    ) -> SessionEffect {
        if state != SessionButtonState::Pressed || self.runner.focus().is_none() {
            return SessionEffect::Ignored;
        }
        let key = match key {
            SessionKey::Backspace => Key::Named(NamedKey::Backspace),
            SessionKey::Delete => Key::Named(NamedKey::Delete),
            SessionKey::ArrowLeft => Key::Named(NamedKey::ArrowLeft),
            SessionKey::ArrowRight => Key::Named(NamedKey::ArrowRight),
            SessionKey::Home => Key::Named(NamedKey::Home),
            SessionKey::End => Key::Named(NamedKey::End),
            SessionKey::Enter => Key::Named(NamedKey::Enter),
            SessionKey::Space if !self.editable_focus() => Key::Named(NamedKey::Space),
            SessionKey::Character(text) if modifiers.control || modifiers.meta => {
                Key::Character(text)
            },
            _ => return SessionEffect::Ignored,
        };
        self.runner.dispatch_key(KeyEvent::with_mods(
            key,
            Modifiers {
                shift: modifiers.shift,
                ctrl: modifiers.control,
                alt: modifiers.alt,
                meta: modifiers.meta,
            },
        ));
        SessionEffect::Handled
    }

    fn text_input(&mut self, text: &str) -> bool {
        if !self.editable_focus() {
            return false;
        }
        self.runner
            .dispatch_key(KeyEvent::new(Key::Composition(CompositionEvent::Commit(
                text.to_string(),
            ))));
        true
    }

    fn editable_focus(&self) -> bool {
        self.runner
            .focus()
            .is_some_and(|node| is_textbox(&self.dom.borrow(), node))
    }

    fn ime_input(&mut self, ime: SessionIme) -> bool {
        if !self.editable_focus() {
            return false;
        }
        let composition = match ime {
            SessionIme::Enabled => CompositionEvent::Enabled,
            SessionIme::Preedit { text, selection } => {
                CompositionEvent::Preedit { text, selection }
            },
            SessionIme::Commit(text) => CompositionEvent::Commit(text),
            SessionIme::Disabled => CompositionEvent::Disabled,
        };
        self.runner
            .dispatch_key(KeyEvent::new(Key::Composition(composition)));
        true
    }

    fn focus_input(&mut self, focused: bool) {
        if !focused {
            self.runner.set_focus(None);
        }
    }

    fn focus_move(&mut self, direction: SessionFocusDirection) -> bool {
        let choices = self.runner.focusables();
        if choices.is_empty() {
            return false;
        }
        let current = self
            .runner
            .focus()
            .and_then(|node| choices.iter().position(|id| *id == node));
        let next = match (direction, current) {
            (SessionFocusDirection::Forward, None) => choices.first(),
            (SessionFocusDirection::Backward, None) => choices.last(),
            (SessionFocusDirection::Forward, Some(index)) => choices.get(index + 1),
            (SessionFocusDirection::Backward, Some(index)) => {
                index.checked_sub(1).and_then(|i| choices.get(i))
            },
        };
        self.runner.set_focus(next.copied());
        next.is_some()
    }

    fn pump(&mut self, _: f64) {
        self.runner.update(State::tick);
    }

    fn pending_work(&mut self) -> SessionPendingWork {
        let state = self.runner.state();
        if state.playing
            || state.attached
            || state.preparing.is_some()
            || state.connection.as_ref().is_some_and(Connection::busy)
        {
            SessionPendingWork::timer(Duration::from_millis(140))
        } else {
            SessionPendingWork::idle()
        }
    }
}
