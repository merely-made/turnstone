// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Placement-addressed input over the shared retained pool. Graph navigation
//! and submission remain host effects, including for independent Reader views.

use inker::{
    SessionButtonState, SessionFocusDirection, SessionInput, SessionKey, SessionModifiers,
    SessionPointerButton,
};
use winit::event::MouseButton;
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

use super::{NodeSessions, NodeSurfaces, Shell, content_link_target};
use crate::action::Action;

impl Shell {
    /// Whether `node`'s pointer and keys go through its browsing controller.
    pub(super) fn controller_drives(&self, node: &uuid::Uuid) -> bool {
        self.content_sessions.has_document(node)
    }

    pub(super) fn session_modifiers(&self) -> SessionModifiers {
        SessionModifiers {
            shift: self.shift,
            control: self.ctrl,
            alt: self.alt,
            meta: self.meta,
        }
    }

    /// Deliver `input` to `node`'s document controller and act on what it
    /// hands up. `None` when the controller does not drive `node`; otherwise
    /// whether the document took the input.
    pub(super) fn document_input(&mut self, node: uuid::Uuid, input: SessionInput) -> Option<bool> {
        if !self.controller_drives(&node) {
            return None;
        }
        let appearance = match self.app.focus {
            crate::surface::FocusTarget::Content {
                node: focused,
                appearance,
            } if focused == node => appearance,
            _ => {
                self.surface_plan()
                    .into_iter()
                    .find(|surface| surface.kind == crate::surface::SurfaceKind::Content(node))?
                    .id
            },
        };
        self.content_input(appearance, pelt_core::PeltInput::Session(input))
    }

    pub(super) fn content_input(
        &mut self,
        appearance: crate::surface::SurfaceId,
        input: pelt_core::PeltInput,
    ) -> Option<bool> {
        self.sync_content_placements();
        let focused = matches!(
            &input,
            pelt_core::PeltInput::Keyboard(_)
                | pelt_core::PeltInput::ScrollKey(_)
                | pelt_core::PeltInput::Session(
                    SessionInput::Key { .. }
                        | SessionInput::Text(_)
                        | SessionInput::Ime(_)
                        | SessionInput::FocusMove(_)
                )
        );
        let content = if focused {
            *self
                .content_sessions
                .pool
                .focused_content(self.input_window)?
        } else {
            self.content_sessions.pool.placement(&appearance)?.content
        };
        // Content-local positions arrive in physical pixels; placements are
        // in layout pixels.
        let mut input = input;
        if let Some(placement) = self.content_sessions.pool.placement(&appearance) {
            input.to_layout(placement.scale_factor);
        }
        let node = match content {
            super::content_pool::ContentKey::Node(node)
            | super::content_pool::ContentKey::Reader { node, .. } => node,
        };
        let effect = match input {
            pelt_core::PeltInput::Keyboard(_)
            | pelt_core::PeltInput::ScrollKey(_)
            | pelt_core::PeltInput::Session(
                SessionInput::Key { .. }
                | SessionInput::Text(_)
                | SessionInput::Ime(_)
                | SessionInput::FocusMove(_),
            ) => super::window_input::focused_input(
                &mut self.content_sessions.pool,
                self.input_window,
                appearance,
                input,
            ),
            _ => self.content_sessions.pool.input(&appearance, input),
        };
        let handled = self.apply_content_effect(node, effect);
        self.sync_ime_allowed();
        Some(handled)
    }

    pub(super) fn apply_content_effect(
        &mut self,
        node: uuid::Uuid,
        effect: pelt_core::PeltHostEffect,
    ) -> bool {
        if let Some(error) = &effect.error {
            tracing::warn!(%node, %error, "content input failed");
        }
        let mut acted = false;
        if let Some(navigation) = effect.navigation {
            let url = content_link_target(&self.app, node, &navigation.request.address);
            self.act(Action::OpenAddress(url));
            acted = true;
        }
        if let Some(submission) = effect.submission {
            self.act(Action::BeginSmolwebSubmission {
                source: Some(node),
                target: submission.action,
            });
            acted = true;
        }
        // Native selection changes must reach contextual retention actions in
        // this turn, even when the document has no background wake pending.
        if effect.handled || acted {
            self.refresh_knot_documents();
        }
        if effect.redraw || acted {
            self.request_redraw();
        }
        effect.handled || acted
    }

    /// A pressed key for the focused document: the key itself first, so an
    /// editor or form field keeps it, then Tab as focus traversal within the
    /// page.
    pub(super) fn document_key(&mut self, node: uuid::Uuid, key: &WinitKey) -> Option<bool> {
        let modifiers = self.session_modifiers();
        let Some(session_key) = session_key(key) else {
            return self.controller_drives(&node).then_some(false);
        };
        let tab = session_key == SessionKey::Tab;
        let handled = self.document_input(
            node,
            SessionInput::Key {
                key: session_key,
                state: SessionButtonState::Pressed,
                modifiers,
                repeat: false,
            },
        )?;
        if handled || !tab {
            return Some(handled);
        }
        self.document_input(
            node,
            SessionInput::FocusMove(if self.shift {
                SessionFocusDirection::Backward
            } else {
                SessionFocusDirection::Forward
            }),
        )
    }
}

pub(super) fn session_pointer_button(button: MouseButton) -> Option<SessionPointerButton> {
    match button {
        MouseButton::Left => Some(SessionPointerButton::Primary),
        MouseButton::Right => Some(SessionPointerButton::Secondary),
        MouseButton::Middle => Some(SessionPointerButton::Auxiliary),
        _ => None,
    }
}

/// A winit key in the neutral session vocabulary.
pub(super) fn session_key(key: &WinitKey) -> Option<SessionKey> {
    Some(match key {
        WinitKey::Character(text) => SessionKey::Character(text.to_string()),
        WinitKey::Named(named) => match named {
            WinitNamedKey::Enter => SessionKey::Enter,
            WinitNamedKey::Tab => SessionKey::Tab,
            WinitNamedKey::Backspace => SessionKey::Backspace,
            WinitNamedKey::Delete => SessionKey::Delete,
            WinitNamedKey::Escape => SessionKey::Escape,
            WinitNamedKey::Space => SessionKey::Space,
            WinitNamedKey::ArrowLeft => SessionKey::ArrowLeft,
            WinitNamedKey::ArrowRight => SessionKey::ArrowRight,
            WinitNamedKey::ArrowUp => SessionKey::ArrowUp,
            WinitNamedKey::ArrowDown => SessionKey::ArrowDown,
            WinitNamedKey::Home => SessionKey::Home,
            WinitNamedKey::End => SessionKey::End,
            WinitNamedKey::PageUp => SessionKey::PageUp,
            WinitNamedKey::PageDown => SessionKey::PageDown,
            _ => return None,
        },
        _ => return None,
    })
}

/// Native event-boundary diagnostics omit text. Scenario inserts bypass this
/// boundary and therefore cannot be mistaken for OS composition observations.
pub(super) fn trace_native_ime(window: u64, ime: &winit::event::Ime) {
    let (kind, utf8_bytes, selection) = match ime {
        winit::event::Ime::Enabled => ("enabled", 0, None),
        winit::event::Ime::Disabled => ("disabled", 0, None),
        winit::event::Ime::Preedit(text, selection) => ("preedit", text.len(), *selection),
        winit::event::Ime::Commit(text) => ("commit", text.len(), None),
    };
    tracing::debug!(target: "turnstone::ime", window, kind, utf8_bytes, ?selection,
        "native IME event");
}

/// A winit IME event in the neutral session vocabulary.
pub(super) fn session_ime(ime: &winit::event::Ime) -> inker::SessionIme {
    match ime {
        winit::event::Ime::Enabled => inker::SessionIme::Enabled,
        winit::event::Ime::Preedit(text, selection) => inker::SessionIme::Preedit {
            text: text.clone(),
            selection: *selection,
        },
        winit::event::Ime::Commit(text) => inker::SessionIme::Commit(text.clone()),
        winit::event::Ime::Disabled => inker::SessionIme::Disabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winit_keys_map_to_the_neutral_vocabulary() {
        assert_eq!(
            session_key(&WinitKey::Character("a".into())),
            Some(SessionKey::Character("a".to_owned()))
        );
        assert_eq!(
            session_key(&WinitKey::Named(WinitNamedKey::Tab)),
            Some(SessionKey::Tab)
        );
        assert_eq!(
            session_key(&WinitKey::Named(WinitNamedKey::PageDown)),
            Some(SessionKey::PageDown)
        );
        assert_eq!(session_key(&WinitKey::Named(WinitNamedKey::F5)), None);
        assert_eq!(
            session_pointer_button(MouseButton::Middle),
            Some(SessionPointerButton::Auxiliary)
        );
    }
}

impl Shell {
    pub(super) fn input_plan(&self, window: u64) -> Vec<crate::surface::Surface> {
        if window == 0 {
            self.surface_plan()
        } else {
            self.lens_windows
                .values()
                .find(|lens| lens.ordinal as u64 + 1 == window)
                .map(|lens| self.lens_plan(lens.ordinal, lens.width, lens.height))
                .unwrap_or_default()
        }
    }
    pub(super) fn focus_in_window(&self, window: u64, focus: crate::surface::FocusTarget) -> bool {
        use crate::surface::{FocusTarget, SurfaceKind};
        self.input_plan(window).iter().any(|surface| match focus {
            FocusTarget::Content { node, appearance } => {
                surface.id == appearance && surface.kind == SurfaceKind::Content(node)
            },
            FocusTarget::Pane(pane) => surface.kind == SurfaceKind::Pane(pane),
            FocusTarget::Graph(pane) => surface.kind == SurfaceKind::Graph(pane),
            _ => window == 0,
        })
    }
    pub(super) fn select_input_window(&mut self, window: u64) -> bool {
        let active = if window == 0 {
            self.primary_focused
        } else {
            self.lens_windows
                .values()
                .any(|lens| lens.ordinal as u64 + 1 == window && lens.focused)
        };
        if !active {
            return false;
        }
        if super::frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            window,
            self.app.session_id.as_uuid(),
        ) || self.focus_in_window(window, self.app.focus)
        {
            return true;
        }
        if let Some(focus) = self
            .window_focus
            .get(&window)
            .copied()
            .filter(|focus| self.focus_in_window(window, *focus))
        {
            self.app.focus = focus;
            return true;
        }
        if let Some(focus) = self.fallback_window_focus(window) {
            self.app.focus = focus;
            self.window_focus.insert(window, focus);
            return true;
        }
        false
    }
    pub(super) fn fallback_window_focus(&self, window: u64) -> Option<crate::surface::FocusTarget> {
        use crate::surface::{FocusTarget, SurfaceKind};
        let plan = self.input_plan(window);
        let focus = |surface: &crate::surface::Surface| match surface.kind {
            SurfaceKind::Pane(pane) => Some(FocusTarget::Pane(pane)),
            SurfaceKind::Graph(pane) => Some(FocusTarget::Graph(pane)),
            SurfaceKind::Content(node) => Some(FocusTarget::Content {
                node,
                appearance: surface.id,
            }),
            _ => None,
        };
        plan.iter().find(|surface| matches!(surface.kind, SurfaceKind::Pane(pane)|SurfaceKind::Graph(pane) if Some(pane)==self.app.active_pane))
            .and_then(focus).or_else(|| plan.iter().find_map(focus))
    }
    pub(super) fn native_input_focus(&mut self, window: u64, focused: bool) {
        if focused {
            self.select_input_window(window);
        } else {
            self.retire_frame_focus(window);
            super::outer_workspace::retire_outer_capture(&mut self.outer_capture, window);
            if self.focus_in_window(window, self.app.focus) {
                self.window_focus.insert(window, self.app.focus);
            }
            self.touch_input
                .focus_lost(&mut self.content_sessions.pool, window);
            self.window_buttons.remove(&window);
            if self
                .contributed_pointer
                .is_some_and(|pointer| pointer.window == window)
            {
                let pointer = self.contributed_pointer.take().unwrap();
                if let crate::surface::SurfaceKind::Pane(pane) = pointer.surface.kind {
                    if let Some(surface) = self.renderers.contributed.get_mut(pane) {
                        surface.mouse_cancel(0., 0.);
                    }
                }
                self.pointer_capture = None;
            }
            if window == 0 {
                self.surface_pointer_buttons = inker::PointerButtons::NONE;
                self.pointer_capture = None;
            }
        }
        self.sync_content_placements();
        self.sync_ime_allowed();
    }
    pub(super) fn deliver_window_key(
        &mut self,
        window: u64,
        key: &WinitKey,
        pressed: bool,
        text: Option<&str>,
    ) {
        let host_editor = self.app.omnibar.open
            || self.app.user_agent_decision.is_open()
            || self.app.document_find.open;
        if window != 0 && host_editor {
            return;
        }
        if !self.select_input_window(window) && !(window == 0 && host_editor) {
            return;
        }
        self.input_window = window;
        if !self.deliver_surface_key(key, pressed, text) && pressed {
            let size = if window == 0 {
                (self.width, self.height)
            } else {
                self.lens_windows
                    .values()
                    .find(|lens| lens.ordinal as u64 + 1 == window)
                    .map(|lens| (lens.width, lens.height))
                    .unwrap_or((1, 1))
            };
            self.on_key_in_window(window as usize, size, key);
        }
        if self.focus_in_window(window, self.app.focus) {
            self.window_focus.insert(window, self.app.focus);
        }
        self.input_window = 0;
        self.sync_ime_allowed();
    }
    pub(super) fn deliver_window_ime(&mut self, window: u64, ime: &winit::event::Ime) -> bool {
        if !self.select_input_window(window) {
            return false;
        }
        if window != 0
            && (self.app.omnibar.open
                || self.app.user_agent_decision.is_open()
                || self.app.document_find.open)
        {
            return false;
        }
        self.input_window = window;
        let handled = self.deliver_contributed_ime(ime) || self.deliver_document_ime(ime);
        tracing::debug!(target: "turnstone::ime", window, handled, "IME delivery");
        self.input_window = 0;
        handled
    }
    pub(super) fn content_wheel(
        &mut self,
        window: u64,
        appearance: crate::surface::SurfaceId,
        node: uuid::Uuid,
        local: (f32, f32),
        delta: (f32, f32),
    ) -> bool {
        if super::window_input::wheel_zoom(
            window,
            self.ctrl,
            delta.1,
            self.app.page_zoom_offered(node),
        ) {
            self.act(if delta.1 > 0. {
                Action::PageZoomIn { member: node }
            } else {
                Action::PageZoomOut { member: node }
            });
            return true;
        }
        self.content_input(
            appearance,
            pelt_core::PeltInput::Wheel {
                position: inker::PhysicalPosition {
                    x: local.0,
                    y: local.1,
                },
                delta,
                modifiers: self.session_modifiers(),
            },
        )
        .unwrap_or(false)
    }
}
