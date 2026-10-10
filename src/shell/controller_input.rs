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
        let placement = self.content_sessions.pool.placement(&appearance)?;
        let node = match placement.content {
            super::content_pool::ContentKey::Node(node)
            | super::content_pool::ContentKey::Reader { node, .. } => node,
        };
        let effect = self.content_sessions.pool.input(&appearance, input);
        Some(self.apply_content_effect(node, effect))
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
