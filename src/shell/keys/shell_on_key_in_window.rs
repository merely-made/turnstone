// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn on_key_in_window(
        &mut self,
        slot: usize,
        size: (u32, u32),
        key: &WinitKey,
    ) {
        let peek_keys = slot == 0
            && !self.app.omnibar.open
            && !self.app.user_agent_decision.is_open()
            && self.chrome.focus_cards_visible()
            && self
                .app
                .reading
                .cards
                .peek
                .as_ref()
                .is_some_and(|t| self.app.valid_focus_card(t));
        if peek_keys && matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
            self.act(Action::FocusCard(crate::focus_card::Command::DismissPeek));
            return;
        }
        if peek_keys
            && matches!(
                key,
                WinitKey::Named(WinitNamedKey::Tab | WinitNamedKey::Enter | WinitNamedKey::Space)
            )
        {
            if let Some(event) = self.cambium_key(key) {
                let intents = self.chrome.focus_card_key(event);
                for intent in intents {
                    super::super::input::dispatch_primary_chrome_press(self, Some(intent));
                }
                self.request_redraw();
                return;
            }
        }
        if slot == 0 {
            let menu_key = match key {
                WinitKey::Named(WinitNamedKey::Escape) => Some("escape"),
                WinitKey::Named(WinitNamedKey::Enter) => Some("enter"),
                WinitKey::Named(WinitNamedKey::ArrowUp) => Some("up"),
                WinitKey::Named(WinitNamedKey::ArrowDown) => Some("down"),
                WinitKey::Named(WinitNamedKey::ArrowLeft) => Some("left"),
                WinitKey::Named(WinitNamedKey::ArrowRight) => Some("right"),
                _ => None,
            };
            if let Some(intents) = menu_key.and_then(|key| self.chrome.menu_key(key)) {
                for intent in intents {
                    super::super::input::dispatch_primary_chrome_press(self, Some(intent));
                }
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
        if !self.app.omnibar.open
            && !self.app.document_find.open
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
            && let Some(target) = self
                .app
                .focused_read_target()
                .filter(|target| self.app.reading.find.get(target).is_some_and(|ui| ui.open))
        {
            self.act(Action::Reading(crate::reading_view::ReadAction::Command(
                target,
                cambium::ReadCommand::CloseFind,
            )));
            return;
        }
        if matches!(key, WinitKey::Named(WinitNamedKey::Escape))
            && !self.app.omnibar.open
            && !self.app.document_find.open
            && let Some(target) = self.app.focused_read_target().filter(|target| {
                self.app
                    .reading
                    .find
                    .get(target)
                    .is_some_and(|ui| ui.appearance_open)
            })
        {
            self.act(Action::Reading(crate::reading_view::ReadAction::Command(
                target,
                cambium::ReadCommand::Appearance,
            )));
            return;
        }
        if self
            .cambium_key(key)
            .as_ref()
            .is_some_and(cambium::is_find_shortcut)
        {
            if let Some(target) = self.app.focused_read_target() {
                self.act(Action::Reading(crate::reading_view::ReadAction::Command(
                    target,
                    cambium::ReadCommand::Find,
                )));
            } else {
                self.act(Action::OpenDocumentFind);
            }
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
        if !self.app.omnibar.open
            && !self.ctrl
            && !self.alt
            && !self.meta
            && !self.shift
            && matches!(key, WinitKey::Named(WinitNamedKey::Enter))
            && let crate::surface::FocusTarget::Graph(pane) = self.app.focus
            && let Some(member) = self.app.graph_pane_focused_member(pane)
            && let Some(graph) = self.app.graph_for_pane(pane)
        {
            let effects = self.app.read_member_in_graph(graph, member);
            self.run_effects(effects);
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
