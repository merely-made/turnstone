//! Omnibar arms: the one line's open/close, editing, and row selection.
//!
//! Editing recomputes the suggestion list against the catalog every keystroke,
//! so what the lane offers always matches app truth.

use crate::action::{Action, CaretMove, Effect};
use crate::observe::AppEvent;
use crate::shell_services::{ShellIntent, ShellOutcome};
use crate::surface::FocusTarget;
use crate::ui::{OmnibarMode, OmnibarState, Suggestion, recompute_suggestions_with_limit};

use super::App;

/// Shortest needle the recall lane answers. One character matches most of a
/// trail, which is noise rather than recall.
const MIN_RECALL_CHARS: usize = 2;

impl App {
    pub(super) fn recompute_omnibar_suggestions(&mut self) {
        let actions = self.available_actions();
        let chrome = self.shell_chrome_config();
        let row_limit = crate::ui::visible_row_limit(
            chrome.omnibar.row_limit,
            &chrome.omnibar.placement,
            self.viewport.1,
            chrome.appearance.ui_zoom,
        );
        recompute_suggestions_with_limit(
            &mut self.omnibar,
            &self.graph_runtimes,
            &actions,
            &self.recall,
            row_limit,
        );
    }

    /// Re-project an open omnibar after the window changed size: how many
    /// rows fit is viewport-derived, so a resize is a reason to recount them.
    /// Silent when the line is closed.
    pub fn reflow_omnibar(&mut self) {
        if self.omnibar.open {
            self.recompute_omnibar_suggestions();
        }
    }

    /// Recompute the rows and ask the trail port for recall when the needle
    /// changed. Cached hits keep showing until the answer lands, so the lane
    /// does not flicker empty between keystrokes.
    pub(super) fn refresh_omnibar(&mut self) -> Vec<Effect> {
        // Resolve the needle FIRST: a line narrowed below the floor drops its
        // cached hits there, and the rows must be recomputed after that drop
        // or the lane keeps offering pages for text that is gone.
        let asked = self.pending_recall_query();
        self.recompute_omnibar_suggestions();
        match asked {
            Some(query) => vec![Effect::Redraw, Effect::RecallQuery { query }],
            None => vec![Effect::Redraw],
        }
    }

    /// The needle recall should answer, when it differs from the last one
    /// asked. The `>` lane never recalls (it searches intents, not pages),
    /// and a needle under two characters matches too much to be an answer.
    fn pending_recall_query(&mut self) -> Option<String> {
        let text = self.omnibar.text.trim().to_string();
        let eligible = self.omnibar.open
            && matches!(&self.omnibar.mode, OmnibarMode::Address)
            && !text.starts_with('>')
            && text.chars().count() >= MIN_RECALL_CHARS;
        if !eligible {
            // A line narrowed back to nothing stops offering pages it is no
            // longer about.
            self.recall.clear();
            self.recall_query.clear();
            return None;
        }
        if text == self.recall_query {
            return None;
        }
        self.recall_query = text.clone();
        Some(text)
    }

    pub(super) fn open_omnibar(&mut self, command: bool) -> Vec<Effect> {
        let mut effects = self.cancel_smolweb_conversation();
        let target = self.fallback_shell_context();
        self.shell.begin_omnibar(target);
        self.omnibar = OmnibarState {
            open: true,
            text: if command {
                ">".to_string()
            } else {
                String::new()
            },
            ..OmnibarState::default()
        };
        self.omnibar.cursor = self.omnibar.text.len();
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        self.events.push(AppEvent::OmnibarOpened);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn close_omnibar(&mut self) -> Vec<Effect> {
        let mut effects = self.cancel_smolweb_conversation();
        self.omnibar = OmnibarState::default();
        // Drop the recall cache with the line it answered; a reopened omnibar
        // must not flash the last search's pages before its own answer lands.
        self.recall.clear();
        self.recall_query.clear();
        self.shell.close_omnibar();
        // Chrome relinquishes focus back to the canvas. Content focus
        // is slice B (content takes input); slice A only distinguishes
        // canvas from chrome.
        if self.focus == FocusTarget::Chrome {
            self.focus = FocusTarget::Graph(self.default_graph_pane());
        }
        self.events.push(AppEvent::OmnibarClosed);
        effects.push(Effect::Redraw);
        effects
    }

    fn cancel_smolweb_conversation(&mut self) -> Vec<Effect> {
        let (node, awaiting, reason) = match self.omnibar.mode.clone() {
            OmnibarMode::SmolwebInput(input) => (
                input.node,
                crate::content::NodeContent::AwaitingInput,
                "input cancelled",
            ),
            OmnibarMode::GeminiIdentity(input) => (
                input.node,
                crate::content::NodeContent::AwaitingIdentity,
                "identity creation cancelled",
            ),
            OmnibarMode::GeminiTrust(input) => (
                input.node,
                crate::content::NodeContent::AwaitingTrust,
                "certificate change rejected",
            ),
            _ => return Vec::new(),
        };
        if self.content.get(node) == Some(&awaiting) {
            self.content.note_failed(node, reason.to_string());
            self.events.push(AppEvent::ContentState {
                node,
                state: format!("failed: {reason}"),
            });
            vec![Effect::CloseContent { node }]
        } else {
            Vec::new()
        }
    }

    pub(super) fn omnibar_char(&mut self, c: char) -> Vec<Effect> {
        self.omnibar.insert_str(c.encode_utf8(&mut [0u8; 4]));
        self.omnibar.selected = 0;
        self.refresh_omnibar()
    }

    pub(super) fn omnibar_insert(&mut self, s: String) -> Vec<Effect> {
        self.omnibar.insert_str(&s);
        self.omnibar.selected = 0;
        self.refresh_omnibar()
    }

    pub(super) fn omnibar_backspace(&mut self) -> Vec<Effect> {
        if self.omnibar.backspace() {
            self.omnibar.selected = 0;
            return self.refresh_omnibar();
        }
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_delete(&mut self) -> Vec<Effect> {
        if self.omnibar.delete_forward() {
            self.omnibar.selected = 0;
            return self.refresh_omnibar();
        }
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_move(&mut self, delta: i32) -> Vec<Effect> {
        let len = self.omnibar.suggestions.len();
        if len > 0 {
            let cur = self.omnibar.selected as i32;
            self.omnibar.selected = (cur + delta).rem_euclid(len as i32) as usize;
        }
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_commit_row(&mut self, index: usize) -> Vec<Effect> {
        // A row click: select that row, then the ordinary commit path
        // (one commit vocabulary, whatever pointed at the row).
        if !self.omnibar.open || index >= self.omnibar.suggestions.len() {
            return vec![Effect::Redraw];
        }
        self.omnibar.selected = index;
        return self.update(Action::OmnibarCommit);
    }

    pub(super) fn repeat_shell_entry(
        &mut self,
        original: crate::shell_services::ShellEntryId,
    ) -> Vec<Effect> {
        let Some((entry, replay)) = self.shell.repeat(original) else {
            return vec![Effect::Redraw];
        };
        let (mut effects, outcome) = match replay.intent {
            ShellIntent::SelectNode { url } => {
                let selected = self.graph_runtimes.select_by_url(&url);
                (
                    vec![Effect::Redraw],
                    ShellOutcome::Completed {
                        summary: if selected {
                            format!("selected {url}")
                        } else {
                            format!("node no longer exists: {url}")
                        },
                    },
                )
            }
            ShellIntent::Navigate { url } => {
                let effects = self.update(Action::OpenAddress(url.clone()));
                (
                    effects,
                    ShellOutcome::Completed {
                        summary: format!("opened {url}"),
                    },
                )
            }
            ShellIntent::Command { label, action } => {
                let effects = self.update(action);
                (
                    effects,
                    ShellOutcome::Completed {
                        summary: format!("ran {label}"),
                    },
                )
            }
        };
        self.shell.complete(entry, outcome);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn open_shell_entry_target(
        &mut self,
        entry: crate::shell_services::ShellEntryId,
    ) -> Vec<Effect> {
        self.shell.request_target(entry);
        vec![Effect::Redraw]
    }
}
