// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Keyboard routing. Focus picks the lane, exactly as it does for the pointer:
//! the omnibar when open, the focused page when one holds focus, the canvas
//! otherwise.
//!
//! A page's scroll keys and Escape are ephemeral (delivered inline, consumed,
//! never an Action) while its durable node/nav chords still lower through the
//! spine. The canvas view hotkeys stay suspended while a page reads, so a
//! stray `space` cannot reseed the graph behind it.

use super::NodeSurfaces;
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

use inker::SessionScrollKey;

use crate::action::{Action, CaretMove};

use super::Shell;

/// The scroll a focused content session should perform for a key, or `None`
/// when the key is not a content-scroll key. Space pages down, Shift+Space
/// pages up (the browser convention); the arrows line-scroll, and Home/End
/// jump to the ends.
/// The page-zoom chords, held under ctrl. `=` joins `+` and `_` joins `-` so
/// neither needs shift, which is what every browser binds.
#[derive(Clone, Copy)]
enum PageZoomChord {
    In,
    Out,
    Reset,
}

impl PageZoomChord {
    fn for_character(value: &str) -> Option<Self> {
        Some(match value {
            "+" | "=" => Self::In,
            "-" | "_" => Self::Out,
            "0" => Self::Reset,
            _ => return None,
        })
    }

    fn action(self, member: uuid::Uuid) -> Action {
        match self {
            Self::In => Action::PageZoomIn { member },
            Self::Out => Action::PageZoomOut { member },
            Self::Reset => Action::PageZoomReset { member },
        }
    }
}

/// Whether a key is a chord this module claims before the focused surface sees
/// it: address/command summons, reload, find, and page zoom.
fn is_claimed_ctrl_chord(key: &WinitKey) -> bool {
    matches!(key, WinitKey::Character(value)
        if ["f", "l", "k", "r"].iter().any(|claimed| value.eq_ignore_ascii_case(claimed))
            || PageZoomChord::for_character(value).is_some())
}

fn content_scroll_key(key: &WinitKey, shift: bool) -> Option<SessionScrollKey> {
    Some(match key {
        WinitKey::Named(WinitNamedKey::ArrowDown) => SessionScrollKey::LineDown,
        WinitKey::Named(WinitNamedKey::ArrowUp) => SessionScrollKey::LineUp,
        WinitKey::Named(WinitNamedKey::PageDown) => SessionScrollKey::PageDown,
        WinitKey::Named(WinitNamedKey::PageUp) => SessionScrollKey::PageUp,
        WinitKey::Named(WinitNamedKey::Home) => SessionScrollKey::Home,
        WinitKey::Named(WinitNamedKey::End) => SessionScrollKey::End,
        WinitKey::Named(WinitNamedKey::Space) if shift => SessionScrollKey::PageUp,
        WinitKey::Named(WinitNamedKey::Space) => SessionScrollKey::PageDown,
        _ => return None,
    })
}

impl Shell {
    /// The key as a Cambium event with the shell's tracked modifiers.
    pub(super) fn cambium_key(&self, key: &WinitKey) -> Option<cambium::KeyEvent> {
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
    pub(super) fn deliver_find_key(&mut self, event: cambium::KeyEvent) {
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
    pub(super) fn type_surface_text(&mut self, text: &str) -> bool {
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
    pub(super) fn deliver_surface_key(
        &mut self,
        key: &WinitKey,
        pressed: bool,
        text: Option<&str>,
    ) -> bool {
        if super::frame_keys::header_focus_in_window(
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
            || (self.meta && matches!(key, WinitKey::Character(value) if value.eq_ignore_ascii_case("f")))
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
    pub(super) fn deliver_document_ime(&mut self, ime: &winit::event::Ime) -> bool {
        if super::frame_keys::header_focus_in_window(
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
            inker::SessionInput::Ime(super::controller_input::session_ime(ime)),
        )
        .unwrap_or(false)
    }

    fn deliver_contributed_key(&mut self, key: &WinitKey) -> bool {
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

    pub(super) fn deliver_contributed_ime(&mut self, ime: &winit::event::Ime) -> bool {
        if super::frame_keys::header_focus_in_window(
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
    pub(super) fn deliver_content_key(&mut self, key: &WinitKey) -> bool {
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
    pub(super) fn on_key(&mut self, key: &WinitKey) {
        self.on_key_in_window(0, (self.width, self.height), key);
    }

    pub(super) fn on_key_in_window(&mut self, slot: usize, size: (u32, u32), key: &WinitKey) {
        if slot == 0 {
            let menu_key=match key {
                WinitKey::Named(WinitNamedKey::Escape)=>Some("escape"),
                WinitKey::Named(WinitNamedKey::Enter)=>Some("enter"),
                WinitKey::Named(WinitNamedKey::ArrowUp)=>Some("up"),
                WinitKey::Named(WinitNamedKey::ArrowDown)=>Some("down"),
                WinitKey::Named(WinitNamedKey::ArrowLeft)=>Some("left"),
                WinitKey::Named(WinitNamedKey::ArrowRight)=>Some("right"),
                _=>None,
            };
            if let Some(intents)=menu_key.and_then(|key|self.chrome.menu_key(key)) {
                for intent in intents { super::input::dispatch_primary_chrome_press(self,Some(intent)); }
                self.request_redraw();
                return;
            }
        }
        if let Some(decision) = self.app.user_agent_decision.active() {
            let request = decision.request();
            let action = match decision {
                crate::user_agent_decision::PendingUserAgentDecision::Permission { .. } => {
                    matches!(key, WinitKey::Named(WinitNamedKey::Escape)).then_some(
                        Action::ChoosePermission {
                            request,
                            choice: crate::user_agent_decision::PermissionChoice::Dismiss,
                        },
                    )
                },
                crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. } => {
                    match key {
                        WinitKey::Named(WinitNamedKey::Escape) => {
                            Some(Action::CancelAuthentication { request })
                        },
                        WinitKey::Named(WinitNamedKey::Enter) => {
                            Some(Action::SubmitAuthentication { request })
                        },
                        WinitKey::Named(WinitNamedKey::Tab) => {
                            Some(Action::FocusAuthenticationField(
                                match self.app.user_agent_decision.authentication.field {
                                    crate::user_agent_decision::AuthenticationField::Username => {
                                        crate::user_agent_decision::AuthenticationField::Password
                                    },
                                    crate::user_agent_decision::AuthenticationField::Password => {
                                        crate::user_agent_decision::AuthenticationField::Username
                                    },
                                },
                            ))
                        },
                        WinitKey::Named(WinitNamedKey::Backspace) => {
                            Some(Action::BackspaceAuthentication)
                        },
                        WinitKey::Named(WinitNamedKey::Space) => {
                            Some(Action::InsertAuthentication(" ".into()))
                        },
                        WinitKey::Character(text) if !self.ctrl && !self.alt => {
                            Some(Action::InsertAuthentication(text.to_string()))
                        },
                        _ => None,
                    }
                },
            };
            if let Some(action) = action {
                self.act(action);
            }
            return;
        }
        // Close projection Find before the generic pane/frame Escape path
        // blurs its editable field back to the graph.
        if !self.app.omnibar.open && !self.app.document_find.open
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
            && let Some(target) = self.app.focused_read_target().filter(|target|
                self.app.reading.find.get(target).is_some_and(|ui| ui.open))
        {
            self.act(Action::Reading(crate::reading_view::ReadAction::Command(target, cambium::ReadCommand::CloseFind)));
            return;
        }
        if matches!(key,WinitKey::Named(WinitNamedKey::Escape)) && !self.app.omnibar.open && !self.app.document_find.open
            && let Some(target)=self.app.focused_read_target().filter(|target|self.app.reading.find.get(target).is_some_and(|ui|ui.appearance_open)) {
            self.act(Action::Reading(crate::reading_view::ReadAction::Command(target,cambium::ReadCommand::Appearance)));return;
        }
        if self.cambium_key(key).as_ref().is_some_and(cambium::is_find_shortcut) {
            if let Some(target) = self.app.focused_read_target() {
                self.act(Action::Reading(crate::reading_view::ReadAction::Command(target, cambium::ReadCommand::Find)));
            } else { self.act(Action::OpenDocumentFind); }
            return;
        }
        if self.deliver_frame_key(slot, size, key) {
            return;
        }
        // Page zoom on the focused document, beside Ctrl+F because it is the
        // same shape: a durable document command that never reaches the page.
        if self.ctrl
            && let WinitKey::Character(value) = key
            && let Some(chord) = PageZoomChord::for_character(value)
        {
            if let Some((member, _)) = self.app.browser_command_target() {
                self.act(chord.action(member));
            }
            return;
        }
        if self.app.document_find.open {
            if let Some(event) = self.cambium_key(key) {
                self.deliver_find_key(event);
            }
            return;
        }
        if !self.app.omnibar.open && self.deliver_contributed_key(key) {
            self.request_redraw();
            return;
        }
        // Content-focused ephemeral keys take priority and never become
        // Actions (the gesture law). When one is consumed, no Action is
        // computed — the canvas view hotkeys stay suspended while a page reads.
        if !self.app.omnibar.open && self.deliver_content_key(key) {
            return;
        }
        let action = if self.app.omnibar.open {
            // The omnibar has keyboard focus: edit keys route to it; canvas
            // hotkeys are suspended while it is open.
            match key {
                WinitKey::Named(WinitNamedKey::Escape) => Some(Action::OmnibarClose),
                WinitKey::Named(WinitNamedKey::Enter) => Some(Action::OmnibarCommit),
                WinitKey::Named(WinitNamedKey::Backspace) => Some(Action::OmnibarBackspace),
                WinitKey::Named(WinitNamedKey::ArrowUp) => Some(Action::OmnibarMove(-1)),
                WinitKey::Named(WinitNamedKey::ArrowDown) => Some(Action::OmnibarMove(1)),
                WinitKey::Named(WinitNamedKey::ArrowLeft) => {
                    Some(Action::OmnibarCaret(CaretMove::Left))
                },
                WinitKey::Named(WinitNamedKey::ArrowRight) => {
                    Some(Action::OmnibarCaret(CaretMove::Right))
                },
                WinitKey::Named(WinitNamedKey::Home) => Some(Action::OmnibarCaret(CaretMove::Home)),
                WinitKey::Named(WinitNamedKey::End) => Some(Action::OmnibarCaret(CaretMove::End)),
                WinitKey::Named(WinitNamedKey::Delete) => Some(Action::OmnibarDelete),
                WinitKey::Named(WinitNamedKey::Space) => Some(Action::OmnibarChar(' ')),
                // Keep or drop the highlighted command (the browsers' bookmark
                // chord; Ctrl+K already summons the palette).
                WinitKey::Character(s) if self.ctrl && s.eq_ignore_ascii_case("d") => {
                    Some(Action::OmnibarToggleKeepSelected)
                },
                WinitKey::Character(s) if !self.ctrl => s.chars().next().map(Action::OmnibarChar),
                _ => None,
            }
        } else if matches!(self.app.focus, crate::surface::FocusTarget::Pane(_)) {
            None
        } else if matches!(self.app.focus, crate::surface::FocusTarget::Content { .. }) {
            // A page holds focus. Its scroll keys and Escape were already
            // consumed above; only the durable node/nav chords still apply
            // here. The canvas VIEW hotkeys (reseed, isometric, orbit) are
            // deliberately suspended: you are in the page, so a stray `space`
            // or `i` must not reshape the graph behind it.
            match key {
                WinitKey::Named(WinitNamedKey::Delete) => Some(Action::DeleteFocusedNode),
                WinitKey::Named(WinitNamedKey::ArrowLeft) if self.alt => Some(Action::NavBack),
                WinitKey::Named(WinitNamedKey::ArrowRight) if self.alt => Some(Action::NavForward),
                WinitKey::Character(s) if self.ctrl => match s.as_str() {
                    "l" => Some(Action::OmnibarOpen { command: false }),
                    "k" => Some(Action::OmnibarOpen { command: true }),
                    "r" => Some(Action::Reload),
                    _ => None,
                },
                _ => None,
            }
        } else {
            canvas_shortcut_action(key, self.ctrl, self.alt)
        };
        if let Some(action) = action {
            // Editable omnibar input keeps its direct typed path. Building a
            // command observation is only for the durable shortcut lane.
            if self.app.omnibar.open {
                self.act(action);
            } else {
                let _ = self.act_command(action);
            }
        }
    }
}

/// The canvas command bindings used after focus-specific key routing.
fn canvas_shortcut_action(key: &WinitKey, ctrl: bool, alt: bool) -> Option<Action> {
    match key {
        WinitKey::Named(WinitNamedKey::Space) => Some(Action::ReseedLayout),
        // Delete forgets the focused node (recoverable from the Trail's
        // Removed section).
        WinitKey::Named(WinitNamedKey::Delete) => Some(Action::DeleteFocusedNode),
        // The browser nav chords (the r3-owed row).
        WinitKey::Named(WinitNamedKey::ArrowLeft) if alt => Some(Action::NavBack),
        WinitKey::Named(WinitNamedKey::ArrowRight) if alt => Some(Action::NavForward),
        WinitKey::Character(s) if ctrl => match s.as_str() {
            // The summon chords: Ctrl+L address flavor, Ctrl+K command
            // flavor (pre-seeded `>`).
            "l" => Some(Action::OmnibarOpen { command: false }),
            "k" => Some(Action::OmnibarOpen { command: true }),
            "r" => Some(Action::Reload),
            _ => None,
        },
        WinitKey::Character(s) => match s.as_str() {
            // Plain-key summons beside the Ctrl chords: `/` (the
            // quick-switcher convention) and `>` straight into the
            // actions lane. Chord-free, so synthesized-input drivers
            // can't lose the modifier race either.
            "/" => Some(Action::OmnibarOpen { command: false }),
            ">" => Some(Action::OmnibarOpen { command: true }),
            "i" => Some(Action::ToggleIsometric),
            "q" => Some(Action::OrbitBy(-0.15)),
            "e" => Some(Action::OrbitBy(0.15)),
            "[" => Some(Action::TiltBy(-0.05)),
            "]" => Some(Action::TiltBy(0.05)),
            "h" => Some(Action::ToggleHeightByDegree),
            _ => None,
        },
        _ => None,
    }
}

fn windows_virtual_key(key: &WinitKey) -> u32 {
    match key {
        WinitKey::Named(WinitNamedKey::Backspace) => 0x08,
        WinitKey::Named(WinitNamedKey::Tab) => 0x09,
        WinitKey::Named(WinitNamedKey::Enter) => 0x0D,
        WinitKey::Named(WinitNamedKey::Shift) => 0x10,
        WinitKey::Named(WinitNamedKey::Control) => 0x11,
        WinitKey::Named(WinitNamedKey::Alt) => 0x12,
        WinitKey::Named(WinitNamedKey::Pause) => 0x13,
        WinitKey::Named(WinitNamedKey::CapsLock) => 0x14,
        WinitKey::Named(WinitNamedKey::Escape) => 0x1B,
        WinitKey::Named(WinitNamedKey::Space) => 0x20,
        WinitKey::Named(WinitNamedKey::PageUp) => 0x21,
        WinitKey::Named(WinitNamedKey::PageDown) => 0x22,
        WinitKey::Named(WinitNamedKey::End) => 0x23,
        WinitKey::Named(WinitNamedKey::Home) => 0x24,
        WinitKey::Named(WinitNamedKey::ArrowLeft) => 0x25,
        WinitKey::Named(WinitNamedKey::ArrowUp) => 0x26,
        WinitKey::Named(WinitNamedKey::ArrowRight) => 0x27,
        WinitKey::Named(WinitNamedKey::ArrowDown) => 0x28,
        WinitKey::Named(WinitNamedKey::Insert) => 0x2D,
        WinitKey::Named(WinitNamedKey::Delete) => 0x2E,
        WinitKey::Character(value) => value
            .chars()
            .next()
            .map(|character| character.to_ascii_uppercase() as u32)
            .unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hest_keyboard_command_reaches_the_reducer_and_reports_model_completion() {
        let mut app = crate::app::App::test_stub();
        let action =
            canvas_shortcut_action(&WinitKey::Character("i".into()), false, false).unwrap();
        let invocation = app.command_invocation(&action).unwrap();
        assert_eq!(invocation.id, "Toggle isometric view");
        let before = app.graph_runtimes.is_isometric();
        let (effects, outcome) = app.invoke_command(&invocation);
        assert_eq!(effects, vec![crate::action::Effect::Redraw]);
        assert_eq!(app.graph_runtimes.is_isometric(), !before);
        assert_eq!(outcome, Some(cambium::Outcome::Done));
    }

    /// The content scroll-key mapping: the page-scroll keys map, Space pages
    /// (down, or up with Shift — the browser convention), and a non-scroll key
    /// declines so the caller lets it fall through to the Action path.
    #[test]
    fn content_scroll_keys_map_page_navigation() {
        let named = |n| WinitKey::Named(n);
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::PageDown), false),
            Some(SessionScrollKey::PageDown)
        );
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::PageUp), false),
            Some(SessionScrollKey::PageUp)
        );
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::Home), false),
            Some(SessionScrollKey::Home)
        );
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::End), false),
            Some(SessionScrollKey::End)
        );
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::ArrowDown), false),
            Some(SessionScrollKey::LineDown)
        );
        // Space pages down; Shift+Space pages up.
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::Space), false),
            Some(SessionScrollKey::PageDown)
        );
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::Space), true),
            Some(SessionScrollKey::PageUp)
        );
        // A non-scroll key is not a content-scroll key: it must fall through to
        // become an Action (e.g. Delete forgets the node).
        assert_eq!(
            content_scroll_key(&named(WinitNamedKey::Delete), false),
            None
        );
        assert_eq!(
            content_scroll_key(&WinitKey::Character("i".into()), false),
            None
        );
    }

    /// The page-zoom chords, and the claim that keeps them off the page: a
    /// focused surface must never also receive Ctrl+F or a zoom chord.
    #[test]
    fn page_zoom_chords_bind_the_browser_keys_and_are_claimed() {
        let member = uuid::Uuid::nil();
        let action =
            |value: &str| PageZoomChord::for_character(value).map(|chord| chord.action(member));
        assert_eq!(action("="), Some(Action::PageZoomIn { member }));
        assert_eq!(action("+"), Some(Action::PageZoomIn { member }));
        assert_eq!(action("-"), Some(Action::PageZoomOut { member }));
        assert_eq!(action("_"), Some(Action::PageZoomOut { member }));
        assert_eq!(action("0"), Some(Action::PageZoomReset { member }));
        assert_eq!(action("1"), None);
        assert_eq!(action("f"), None);

        for value in [
            "=", "+", "-", "_", "0", "f", "F", "l", "L", "k", "K", "r", "R",
        ] {
            assert!(
                is_claimed_ctrl_chord(&WinitKey::Character(value.into())),
                "ctrl+{value} is claimed before the page sees it"
            );
        }
        assert!(!is_claimed_ctrl_chord(&WinitKey::Character("1".into())));
        assert!(!is_claimed_ctrl_chord(&WinitKey::Named(
            WinitNamedKey::PageDown
        )));
    }
}
