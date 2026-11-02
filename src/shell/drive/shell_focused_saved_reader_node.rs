// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// turnstone drives through the shared taproot harness: implementing this
/// small surface grants the `resolve` / `click` verbs (used by the collapsed
/// `click_pane_*` above) for free. `with_surfaces` hands the retained pane DOMs
/// to a visitor — the borrow guards live only for the callback, which is why the
/// trait takes a visitor rather than returning a `Vec` (turnstone's DOMs are behind
/// `RefCell`). Inspector/Workbench panes join by adding their `dom_ref` here when
/// they grow click verbs.
impl Shell {
    pub(super) fn focused_saved_reader_node(&self) -> Option<uuid::Uuid> {
        focused_saved_reader_node_for(&self.app, |node| self.app.saved_readers.contains_key(&node))
    }

    pub(super) fn saved_reader_engine_live(&self, node: uuid::Uuid) -> bool {
        matches!(
            self.app.content.get(node),
            Some(crate::content::NodeContent::Live)
        ) && self
            .app
            .content
            .facts(node)
            .is_some_and(|facts| facts.engine == crate::saved_sources::ENGINE_ID)
            && self.content_sessions.session(&node).is_some_and(|session| {
                session
                    .as_any_ref()
                    .downcast_ref::<mere_document_lanes::SmolwebDocumentSession>()
                    .is_some()
            })
    }

    pub(super) fn focused_saved_reader_ready(&self) -> bool {
        let Some(node) = self.focused_saved_reader_node() else {
            return false;
        };
        self.app.saved_readers.get(&node).is_some_and(|reader| {
            reader.request.is_none()
                && reader.error.is_none()
                && reader.content.is_some()
                && self.saved_reader_engine_live(node)
        })
    }

    pub(super) fn scenario_click_content_window(
        &mut self,
        window: usize,
        x: f32,
        y: f32,
    ) -> Result<(), String> {
        let focused = if window == 0 {
            self.primary_focused
        } else {
            self.lens_windows
                .values()
                .find(|lens| lens.ordinal + 1 == window)
                .ok_or("click-content-window target is absent")?
                .focused
        };
        if !focused
            || self.app.omnibar.open
            || self.app.document_find.open
            || self.app.user_agent_decision.is_open()
        {
            return Err(
                "click-content-window requires the focused native window and no host editor".into(),
            );
        }
        self.sync_content_placements();
        let plan = self.input_plan(window as u64);
        let position = inker::PhysicalPosition { x, y };
        let hit = super::super::lens::lens_press_hit(
            &self.content_sessions.pool,
            window as u64,
            &plan,
            self.app.focus,
            position,
        )
        .ok_or("click-content-window misses content")?;
        if !matches!(hit.kind, crate::surface::SurfaceKind::Content(_)) {
            return Err("click-content-window hit is not document content".into());
        }
        // The same native mouse prelude, focus admission and content dispatch
        // used by a lens press. Release keeps its exact captured appearance.
        self.input_window = window as u64;
        self.window_mouse_input(
            window as u64,
            None,
            (x, y),
            inker::PointerPhase::Down,
            Some(inker::MouseButton::Left),
        );
        let hit = super::super::window_input::content_press_target(
            &mut self.app.focus,
            &self.content_sessions.pool,
            window as u64,
            position,
            hit,
        );
        self.pointer_capture = Some(hit.kind);
        self.content_mouse(
            hit.id,
            hit.local,
            inker::PointerPhase::Down,
            Some(inker::MouseButton::Left),
        );
        self.window_mouse_input(
            window as u64,
            Some(hit.id),
            (x, y),
            inker::PointerPhase::Up,
            Some(inker::MouseButton::Left),
        );
        self.pointer_capture = None;
        self.sync_ime_allowed();
        self.request_redraw();
        Ok(())
    }

    pub(super) fn scenario_key_in_window(
        &mut self,
        window: usize,
        key: &crate::scenario::EditKey,
    ) -> Result<(), String> {
        if window != 0
            && !self
                .lens_windows
                .values()
                .any(|lens| lens.ordinal + 1 == window)
        {
            return Err("key-window target is absent".into());
        }
        if window != 0
            && (self.app.omnibar.open
                || self.app.document_find.open
                || self.app.user_agent_decision.is_open())
        {
            return Err("a primary host editor owns keyboard input".into());
        }
        if !super::super::frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            window as u64,
            self.app.session_id.as_uuid(),
        ) && !self.focus_in_window(window as u64, self.app.focus)
        {
            let focus = self
                .window_focus
                .get(&(window as u64))
                .copied()
                .filter(|focus| self.focus_in_window(window as u64, *focus))
                .or_else(|| self.fallback_window_focus(window as u64))
                .ok_or("window has no current focus target")?;
            self.app.focus = focus;
        }
        use crate::scenario::EditKey;
        use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

        // Route through the SAME key seam winit uses, so `key` drives
        // whatever holds focus (the omnibar, a focused page, the
        // canvas) exactly as a real press would — one description, two
        // runners, for keys as well as pointers. The former direct
        // EditKey->omnibar-Action map only ever reached the omnibar.
        let (winit_key, ctrl) = match key {
            EditKey::Enter | EditKey::ShiftEnter => (WinitKey::Named(WinitNamedKey::Enter), false),
            EditKey::Escape => (WinitKey::Named(WinitNamedKey::Escape), false),
            EditKey::Tab | EditKey::BackTab => (WinitKey::Named(WinitNamedKey::Tab), false),
            EditKey::FrameFocus => (WinitKey::Named(WinitNamedKey::F6), true),
            EditKey::FrameClose => (WinitKey::Character("w".into()), true),
            EditKey::Backspace => (WinitKey::Named(WinitNamedKey::Backspace), false),
            EditKey::Delete => (WinitKey::Named(WinitNamedKey::Delete), false),
            EditKey::Up => (WinitKey::Named(WinitNamedKey::ArrowUp), false),
            EditKey::Down => (WinitKey::Named(WinitNamedKey::ArrowDown), false),
            EditKey::Left | EditKey::ShiftLeft => {
                (WinitKey::Named(WinitNamedKey::ArrowLeft), false)
            },
            EditKey::Right | EditKey::ShiftRight => {
                (WinitKey::Named(WinitNamedKey::ArrowRight), false)
            },
            EditKey::SelectAll => (WinitKey::Character("a".into()), true),
            EditKey::Home => (WinitKey::Named(WinitNamedKey::Home), false),
            EditKey::End => (WinitKey::Named(WinitNamedKey::End), false),
            EditKey::PageDown => (WinitKey::Named(WinitNamedKey::PageDown), false),
            EditKey::PageUp => (WinitKey::Named(WinitNamedKey::PageUp), false),
            EditKey::Space => (WinitKey::Named(WinitNamedKey::Space), false),
            EditKey::Save => (WinitKey::Character("s".into()), true),
            EditKey::Find => (WinitKey::Character("f".into()), true),
            EditKey::KeepCommand => (WinitKey::Character("d".into()), true),
        };
        let previous_ctrl = self.ctrl;
        let previous_shift = self.shift;
        self.shift |= matches!(
            key,
            EditKey::BackTab | EditKey::ShiftRight | EditKey::ShiftLeft | EditKey::ShiftEnter
        );
        self.ctrl |= ctrl;
        // Match the text accompanying a native winit key press. In
        // Chromium, Enter's carriage-return char event activates the
        // focused button after rawKeyDown.
        let text = match winit_key {
            WinitKey::Named(WinitNamedKey::Enter) => Some("\r"),
            WinitKey::Named(WinitNamedKey::Space) => Some(" "),
            _ => None,
        };
        // Purpose-built self-drive enters after OS admission. Native
        // input keeps its physical activation guard; this fixture
        // validates current window custody without stealing OS focus.
        self.input_window = window as u64;
        if self.deliver_surface_key(&winit_key, true, text) {
            self.deliver_surface_key(&winit_key, false, None);
        } else {
            let size = if window == 0 {
                (self.width, self.height)
            } else {
                self.lens_windows
                    .values()
                    .find(|lens| lens.ordinal + 1 == window)
                    .map(|lens| (lens.width, lens.height))
                    .unwrap()
            };
            self.on_key_in_window(window, size, &winit_key);
        }
        self.ctrl = previous_ctrl;
        self.shift = previous_shift;
        Ok(())
    }
}
