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

impl App {
    pub(super) fn recompute_omnibar_suggestions(&mut self) {
        let actions = self.available_actions();
        let row_limit = self.shell_chrome_config().omnibar.row_limit;
        recompute_suggestions_with_limit(
            &mut self.omnibar,
            &self.graph_runtimes,
            &actions,
            row_limit,
        );
    }

    pub(super) fn open_omnibar(&mut self, command: bool) -> Vec<Effect> {
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
        vec![Effect::Redraw]
    }

    pub(super) fn close_omnibar(&mut self) -> Vec<Effect> {
        self.omnibar = OmnibarState::default();
        self.shell.close_omnibar();
        // Chrome relinquishes focus back to the canvas. Content focus
        // is slice B (content takes input); slice A only distinguishes
        // canvas from chrome.
        if self.focus == FocusTarget::Chrome {
            self.focus = FocusTarget::Canvas;
        }
        self.events.push(AppEvent::OmnibarClosed);
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_char(&mut self, c: char) -> Vec<Effect> {
        self.omnibar.insert_str(c.encode_utf8(&mut [0u8; 4]));
        self.omnibar.selected = 0;
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_insert(&mut self, s: String) -> Vec<Effect> {
        self.omnibar.insert_str(&s);
        self.omnibar.selected = 0;
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_backspace(&mut self) -> Vec<Effect> {
        if self.omnibar.backspace() {
            self.omnibar.selected = 0;
            self.recompute_omnibar_suggestions();
        }
        vec![Effect::Redraw]
    }

    pub(super) fn omnibar_delete(&mut self) -> Vec<Effect> {
        if self.omnibar.delete_forward() {
            self.omnibar.selected = 0;
            self.recompute_omnibar_suggestions();
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
