// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// The key as a Cambium event with the shell's tracked modifiers.
    pub(in super::super) fn cambium_key(&self, key: &WinitKey) -> Option<cambium::KeyEvent> {
        cambium_winit::key_event_from_winit(
            key,
            cambium::Modifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        )
    }

    /// Deliver a key or IME event to the open find field. The field's shared
    /// key map answers; the shell only lowers what it reports.
    pub(in super::super) fn deliver_find_key(&mut self, event: cambium::KeyEvent) {
        let mut sizes = vec![(0, self.width as f32, self.height as f32)];
        sizes.extend(
            self.lens_windows
                .values()
                .map(|lens| (lens.ordinal + 1, lens.width as f32, lens.height as f32)),
        );
        self.chrome.sync(&self.app, &sizes);
        for reported in self.chrome.find_key(event) {
            if let Some(action) = crate::document_find::find_action(reported) {
                self.act(action);
            }
        }
        self.request_redraw();
    }

    /// Self-drive committed text exercises producer dispatch without claiming
    /// a platform IME composition or physical-keyboard acceptance receipt.
    pub(in super::super) fn type_surface_text(&mut self, text: &str) -> bool {
        if let crate::surface::FocusTarget::Pane(pane) = self.app.focus {
            if !self.inline_text_in_window(self.input_window) {
                return false;
            }
            for character in text.chars() {
                let key = WinitKey::Character(character.to_string().into());
                let Some(event) =
                    cambium_winit::key_event_from_winit(&key, cambium::Modifiers::default())
                else {
                    return false;
                };
                if !self.deliver_inline_event(pane, event) {
                    return false;
                }
            }
            return true;
        }
        let crate::surface::FocusTarget::Content { node, .. } = self.app.focus else {
            return false;
        };
        if !self.content_sessions.has_surface(&node) {
            return false;
        }
        for character in text.chars() {
            let text = character.to_string();
            let key = WinitKey::Character(text.clone().into());
            if !self.deliver_surface_key(&key, true, Some(&text)) {
                return false;
            }
            self.deliver_surface_key(&key, false, None);
        }
        true
    }

    /// Deliver a key directly to a focused frame-streaming surface. Document
    /// sessions keep their semantic scroll-key path below; a browser surface
    /// needs the platform-style key stream itself. Escape intentionally falls
    /// through so Turnstone retains its established blur-to-canvas behavior.
    pub(in super::super) fn deliver_surface_key(
        &mut self,
        key: &WinitKey,
        pressed: bool,
        text: Option<&str>,
    ) -> bool {
        if super::super::frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            self.input_window,
            self.app.session_id.as_uuid(),
        ) || (self.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::F6)))
        {
            return false;
        }
        let crate::surface::FocusTarget::Content { node, appearance } = self.app.focus else {
            return false;
        };
        if self.app.omnibar.open
            || self.app.user_agent_decision.is_open()
            || self.app.document_find.open
            || (self.ctrl && is_claimed_ctrl_chord(key))
            || (self.meta
                && matches!(key, WinitKey::Character(value) if value.eq_ignore_ascii_case("f")))
            || (self.alt
                && matches!(
                    key,
                    WinitKey::Named(WinitNamedKey::ArrowLeft | WinitNamedKey::ArrowRight)
                ))
        {
            return false;
        }
        if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
            return false;
        }
        if !self.content_sessions.has_surface(&node) {
            return false;
        }
        let key_code = windows_virtual_key(key);
        let event = inker::KeyboardEvent {
            key_code,
            scan_code: 0,
            modifiers: inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
            pressed,
            text: pressed.then(|| text.unwrap_or_default().to_string()),
        };
        self.content_input(appearance, pelt_core::PeltInput::Keyboard(event))
            .unwrap_or(false)
    }

    /// IME for the focused document goes through its browsing controller,
    /// as its keys do: Knot's editor and a page's form fields take it alike.
    pub(in super::super) fn deliver_document_ime(&mut self, ime: &winit::event::Ime) -> bool {
        if super::super::frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            self.input_window,
            self.app.session_id.as_uuid(),
        ) {
            return true;
        }
        let crate::surface::FocusTarget::Content { node, .. } = self.app.focus else {
            return false;
        };
        self.document_input(
            node,
            inker::SessionInput::Ime(super::super::controller_input::session_ime(ime)),
        )
        .unwrap_or(false)
    }

    pub(super) fn deliver_contributed_key(&mut self, key: &WinitKey) -> bool {
        let crate::surface::FocusTarget::Pane(pane_id) = self.app.focus else {
            return false;
        };
        if self.renderers.inline_mount(pane_id).is_some() {
            if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
                self.renderers.blur_inline(pane_id);
                let plan = if self.input_window == 0 {
                    self.surface_plan()
                } else {
                    let Some(lens) = self
                        .lens_windows
                        .values()
                        .find(|lens| lens.ordinal as u64 + 1 == self.input_window)
                    else {
                        return false;
                    };
                    self.lens_plan(lens.ordinal, lens.width, lens.height)
                };
                self.app.focus = plan
                    .iter()
                    .find_map(|surface| match surface.kind {
                        crate::surface::SurfaceKind::Graph(pane) => {
                            Some(crate::surface::FocusTarget::Graph(pane))
                        },
                        _ => None,
                    })
                    .unwrap_or(crate::surface::FocusTarget::Pane(pane_id));
            } else {
                let modifiers = cambium::Modifiers {
                    shift: self.shift,
                    ctrl: self.ctrl,
                    alt: self.alt,
                    meta: self.meta,
                };
                let Some(event) = cambium_winit::key_event_from_winit(key, modifiers) else {
                    return false;
                };
                if !self.deliver_inline_event(pane_id, event) {
                    return false;
                }
            }
            self.sync_ime_allowed();
            self.request_redraw();
            return true;
        }
        if let Some(pane) = self.renderers.settings.get_mut(&pane_id) {
            if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
                pane.blur();
                self.app.focus = crate::surface::FocusTarget::Graph(self.app.default_graph_pane());
                self.request_redraw();
                return true;
            }
            if matches!(key, WinitKey::Named(WinitNamedKey::Tab)) {
                pane.focus_traverse(!self.shift);
                return true;
            }
            let modifiers = cambium::Modifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: false,
            };
            return cambium_winit::key_event_from_winit(key, modifiers).is_some_and(|event| {
                pane.key(event);
                true
            });
        }
        if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
            if let Some(pane) = self.renderers.contributed.get_mut(pane_id) {
                pane.focus(None);
            }
            self.app.focus = crate::surface::FocusTarget::Graph(self.app.default_graph_pane());
            self.request_redraw();
            return true;
        }
        let Some(pane) = self.renderers.contributed.get_mut(pane_id) else {
            return false;
        };
        if matches!(key, WinitKey::Named(WinitNamedKey::Tab)) {
            pane.focus_traverse(!self.shift);
            return true;
        }
        let modifiers = cambium::Modifiers {
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.alt,
            meta: self.meta,
        };
        cambium_winit::key_event_from_winit(key, modifiers).is_some_and(|event| {
            pane.key(event);
            true
        })
    }

    pub(in super::super) fn deliver_contributed_ime(&mut self, ime: &winit::event::Ime) -> bool {
        if super::super::frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            self.input_window,
            self.app.session_id.as_uuid(),
        ) {
            return true;
        }
        let crate::surface::FocusTarget::Pane(pane_id) = self.app.focus else {
            return false;
        };
        if self.renderers.inline_mount(pane_id).is_some() {
            return self.renderers.inline_accepts_text(pane_id)
                && self.deliver_inline_event(pane_id, cambium_winit::ime_event_from_winit(ime));
        }
        self.renderers
            .contributed
            .get_mut(pane_id)
            .is_some_and(|pane| {
                pane.key(cambium_winit::ime_event_from_winit(ime));
                true
            })
    }

    /// Deliver an ephemeral key to the FOCUSED content session (the gesture
    /// law, exactly as the wheel does): scroll keys scroll the page, Escape
    /// blurs back to the canvas. Returns whether the key was consumed here, so
    /// the caller skips the Action path. Keys that are NOT ephemeral content
    /// keys (the durable node/nav chords) return `false` and fall through to
    /// become Actions. Unlike the wheel this is focus-routed, not
    /// position-routed: a page reader's keys go to the page they are reading.
    pub(in super::super) fn deliver_content_key(&mut self, key: &WinitKey) -> bool {
        let crate::surface::FocusTarget::Content { node, appearance } = self.app.focus else {
            return false;
        };
        // The document's own controller takes the key first, so an editor,
        // a form field or a focused link keeps it (Enter follows, Tab moves
        // focus within the page). What it leaves falls to the scroll keys,
        // the blur, and then the Actions.
        if self.document_key(node, key) == Some(true) {
            return true;
        }
        // Escape blurs back to the canvas. Focus is ephemeral UI state (the
        // press path sets it directly too), so this rides on state, not an
        // Action.
        if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
            self.app.focus = crate::surface::FocusTarget::Graph(self.app.default_graph_pane());
            self.request_redraw();
            return true;
        }
        let Some(scroll) = content_scroll_key(key, self.shift) else {
            return false;
        };
        let moved = self
            .content_input(appearance, pelt_core::PeltInput::ScrollKey(scroll))
            .unwrap_or(false);
        // Record the outcome for the scenario probe, and repaint when the page
        // actually moved.
        self.content_scroll_moved = Some(moved);
        if moved {
            self.request_redraw();
        }
        // Consumed even when the page did not move (already at the end): the
        // key belonged to the focused page, so the canvas must not also act on
        // it.
        true
    }

    /// The whole pressed-key path, shared by winit and the scenario runner so
    /// one description drives two runners for keys as well as pointers. Focus
    /// decides the lane, exactly as it does for the pointer: the omnibar when
    /// it is open, the focused content when a page holds focus, the canvas
    /// otherwise. Ephemeral content keys (scroll, blur) are delivered inline
    /// and consumed; everything else lowers to an Action through the spine.
    pub(in super::super) fn on_key(&mut self, key: &WinitKey) {
        self.on_key_in_window(0, (self.width, self.height), key);
    }
}
