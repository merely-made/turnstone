// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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

fn captured_page_scope(snapshot: &crate::place::OfflinePlaceSnapshot) -> (String, bool) {
    match &snapshot.captured_selection {
        crate::place::CapturedCollectionSelection::AllEffective => {
            ("Captured pages: all shared captures".to_string(), true)
        },
        crate::place::CapturedCollectionSelection::Collection {
            status: crate::place::CapturedCollectionSelectionStatus::Ready { name, .. },
            ..
        } => (format!("Captured pages: {name}"), true),
        crate::place::CapturedCollectionSelection::Collection {
            status: crate::place::CapturedCollectionSelectionStatus::Stale { .. },
            ..
        } => (
            "Captured collection is stale; use its latest version".to_string(),
            false,
        ),
        crate::place::CapturedCollectionSelection::Collection {
            status: crate::place::CapturedCollectionSelectionStatus::Unavailable,
            ..
        } => ("Captured collection is unavailable".to_string(), false),
        crate::place::CapturedCollectionSelection::Collection {
            status: crate::place::CapturedCollectionSelectionStatus::ForeignMoot,
            ..
        } => (
            "Captured collection is unavailable for this place".to_string(),
            false,
        ),
    }
}

impl App {
    pub(super) fn recompute_omnibar_suggestions(&mut self) {
        // Stage 1 of the palette open-lag instrument. This runs on the input
        // edge, so the frame that presents the result cannot time it; the
        // accumulator carries the cost forward to that frame.
        let started = std::time::Instant::now();
        let mut refit = false;
        let chrome = self.shell_chrome_config().clone();
        let row_limit = crate::ui::visible_row_limit(
            chrome.omnibar.row_limit,
            &chrome.omnibar.placement,
            self.viewport.1,
            chrome.appearance.ui_zoom,
        );
        if matches!(self.omnibar.mode, OmnibarMode::CapturedPages) {
            self.recompute_captured_page_suggestions(row_limit);
            self.frame_timings
                .note_suggestions(started.elapsed(), false);
            return;
        }
        if matches!(self.omnibar.mode, OmnibarMode::PlaceStatus) {
            let query = self.omnibar.text.trim().to_lowercase();
            let limit = row_limit.max(1);
            self.omnibar.suggestions = self.place.status_lines().into_iter()
                .filter(|line| line.to_lowercase().contains(&query))
                .take(if limit > 1 { limit - 1 } else { limit })
                .map(Suggestion::Prompt)
                .collect();
            if self.omnibar.suggestions.is_empty() {
                self.omnibar.suggestions.push(Suggestion::Hint("No matching place status; clear the filter"));
            }
            if limit > 1 {
                self.omnibar.suggestions.insert(0, Suggestion::Hint("Place status: type to filter; reopen to refresh"));
            }
            self.frame_timings.note_suggestions(started.elapsed(), false);
            return;
        }
        let actions = self.available_actions();
        recompute_suggestions_with_limit(
            &mut self.omnibar,
            &self.graph_runtimes,
            &actions,
            &self.recall,
            row_limit,
        );
        let review_extra = self
            .omnibar
            .suggestions
            .iter()
            .find(|suggestion| crate::chrome_view::is_install_review(suggestion))
            .map(|suggestion| {
                crate::ui::chrome_review_row_extra_height(
                    &crate::chrome_view::row_text(suggestion),
                    &chrome.appearance,
                )
            })
            .unwrap_or(0.0);
        let fitted_limit = crate::ui::visible_row_limit_with_extra_height(
            chrome.omnibar.row_limit,
            &chrome.omnibar.placement,
            self.viewport.1,
            chrome.appearance.ui_zoom,
            review_extra,
        );
        if fitted_limit < row_limit {
            // The same catalog, ranked a second time for one keystroke.
            // Counted separately because it is avoidable by construction
            // rather than load.
            refit = true;
            recompute_suggestions_with_limit(
                &mut self.omnibar,
                &self.graph_runtimes,
                &actions,
                &self.recall,
                fitted_limit,
            );
        }
        self.frame_timings
            .note_suggestions(started.elapsed(), refit);
    }

    fn recompute_captured_page_suggestions(&mut self, row_limit: usize) {
        self.omnibar.suggestions.clear();
        let row_limit = row_limit.max(1);
        let query = self.omnibar.text.trim();
        let (scope, searchable, hits) = match &self.place {
            crate::place::PlaceState::Offline { snapshot, .. } => {
                let (scope, searchable) = captured_page_scope(snapshot);
                let hits = (searchable && !query.is_empty())
                    .then(|| {
                        snapshot
                            .captured
                            .search(query, row_limit.saturating_sub(1).max(1))
                    })
                    .unwrap_or_default();
                (scope, searchable, hits)
            },
            _ => (
                "Captured pages are unavailable".to_string(),
                false,
                Vec::new(),
            ),
        };
        // A one-row setting must still offer a result. With more room, keep
        // the selected scope visible above the results.
        let show_scope = row_limit > 1 || query.is_empty() || !searchable;
        if show_scope {
            self.omnibar.suggestions.push(Suggestion::Prompt(scope));
        }
        let no_hits = hits.is_empty();
        if query.is_empty() {
            self.omnibar
                .suggestions
                .push(Suggestion::Hint("type to search captured pages"));
        } else if searchable {
            self.omnibar
                .suggestions
                .extend(hits.into_iter().map(|hit| Suggestion::Act {
                    label: format!("Open source: {}", hit.title),
                    action: Action::OpenAddress(hit.source),
                }));
            if no_hits {
                self.omnibar
                    .suggestions
                    .push(Suggestion::Hint("no captured pages match"));
            }
        } else {
            self.omnibar
                .suggestions
                .push(Suggestion::Hint("no captured pages available"));
        }
        self.omnibar.suggestions.truncate(row_limit.max(1));
        self.omnibar.selected = self
            .omnibar
            .suggestions
            .iter()
            .position(|row| matches!(row, Suggestion::Act { .. }))
            .unwrap_or(0);
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

    pub(super) fn open_place_status(&mut self) -> Vec<Effect> {
        let mut effects = self.cancel_smolweb_conversation();
        let target = self.fallback_shell_context();
        self.shell.begin_omnibar(target);
        self.recall.clear();
        self.recall_query.clear();
        self.omnibar = OmnibarState {
            open: true,
            mode: OmnibarMode::PlaceStatus,
            ..OmnibarState::default()
        };
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        if let crate::place::PlaceState::Offline { generation, .. } = self.place {
            effects.push(Effect::ResyncPlace { session: self.session_id, generation });
        }
        self.events.push(AppEvent::OmnibarOpened);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn open_captured_pages(&mut self) -> Vec<Effect> {
        if !matches!(self.place, crate::place::PlaceState::Offline { .. }) {
            return vec![Effect::Redraw];
        }
        let mut effects = self.cancel_smolweb_conversation();
        let target = self.fallback_shell_context();
        self.shell.begin_omnibar(target);
        self.recall.clear();
        self.recall_query.clear();
        self.omnibar = OmnibarState {
            open: true,
            mode: OmnibarMode::CapturedPages,
            ..OmnibarState::default()
        };
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
        if matches!(self.omnibar.mode, OmnibarMode::SmolwebSubmissionResult(_)) {
            self.active_smolweb_submission = None;
            return Vec::new();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(body: &str, source: &str) -> crate::place::OfflinePlaceSnapshot {
        crate::place::OfflinePlaceSnapshot {
            captured: crate::place::CapturedCollectionCache {
                groups: vec![crate::place::CapturedContentGroup {
                    extraction_schema: "fleece/1".into(),
                    normalization: "plain".into(),
                    reader_profile: "reader".into(),
                    canonical_text_hash: "a".repeat(64),
                    canonical_text_iri: "urn:captured:test".into(),
                    canonical_text: body.into(),
                    contributions: vec![crate::place::CapturedContribution {
                        share_operation: [1; 32],
                        annotation_manifest: [2; 32],
                        contributor: [3; 32],
                        source: source.into(),
                        title: "Field notes".into(),
                        shared_at_ms: 1,
                    }],
                }],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn offline(snapshot: crate::place::OfflinePlaceSnapshot) -> crate::place::PlaceState {
        crate::place::PlaceState::Offline {
            binding: crate::place::PlaceBindingV1::new(
                crate::place::PlaceId([1; 32]),
                crate::place::SharedContainerId([2; 32]),
                crate::place::ChatSpaceId([3; 32]),
                "hall",
            )
            .unwrap(),
            generation: 1,
            snapshot,
        }
    }

    #[test]
    fn captured_page_mode_finds_body_text_without_recall_rows() {
        let source = "https://example.test/field-notes";
        let mut app = App::test_stub();
        app.place = offline(snapshot("a body-only phrase", source));
        app.recall.push(crate::action::RecallHit {
            url: "https://recall.test/unrelated".into(),
            title: Some("a body-only phrase".into()),
            at_ms: 1,
        });

        app.update(Action::SearchCapturedPages);
        assert!(app.recall.is_empty() && app.recall_query.is_empty());
        app.update(Action::OmnibarInsert("body-only phrase".into()));

        assert!(app.omnibar.suggestions.iter().any(|row| {
            matches!(
                row,
                Suggestion::Act { label, action }
                    if label == "Open source: Field notes"
                        && action == &Action::OpenAddress(source.into())
            )
        }));
        assert!(
            !app.omnibar
                .suggestions
                .iter()
                .any(|row| matches!(row, Suggestion::Recall { .. })),
            "captured-page search never blends browsing recall"
        );
    }

    #[test]
    fn captured_page_mode_drops_results_after_stale_or_empty_refresh() {
        let source = "https://example.test/field-notes";
        let mut app = App::test_stub();
        app.place = offline(snapshot("a body-only phrase", source));
        app.update(Action::SearchCapturedPages);
        app.update(Action::OmnibarInsert("body-only phrase".into()));
        assert!(
            app.omnibar
                .suggestions
                .iter()
                .any(|row| matches!(row, Suggestion::Act { .. }))
        );

        let requested = crate::place::PlaceCollectionVersion {
            moot: crate::place::PlaceId([1; 32]),
            collection: crate::place::PlaceCollectionId([4; 32]),
            frontier: vec![[5; 32]],
            membership_commitment: [6; 32],
        };
        let mut stale = snapshot("a body-only phrase", source);
        stale.captured_selection = crate::place::CapturedCollectionSelection::Collection {
            requested: requested.clone(),
            status: crate::place::CapturedCollectionSelectionStatus::Stale { current: requested },
        };
        app.place = offline(stale);
        app.recompute_omnibar_suggestions();
        assert!(
            !app.omnibar
                .suggestions
                .iter()
                .any(|row| matches!(row, Suggestion::Act { .. })),
            "a stale scope cannot retain prior hits"
        );

        app.place = offline(snapshot("a body-only phrase", source));
        app.recompute_omnibar_suggestions();
        assert!(
            app.omnibar
                .suggestions
                .iter()
                .any(|row| matches!(row, Suggestion::Act { .. })),
            "the receipt starts with a visible captured result"
        );
        assert_eq!(
            app.apply_update(crate::action::Update::PlaceOpened {
                session: app.session_id,
                generation: 1,
                result: Ok(crate::place::OfflinePlaceSnapshot::default()),
            }),
            vec![Effect::Redraw]
        );
        assert!(
            !app.omnibar
                .suggestions
                .iter()
                .any(|row| matches!(row, Suggestion::Act { .. })),
            "a PlaceOpened replacement reflows the open field without typing"
        );
    }

    #[test]
    fn captured_search_respects_a_single_row_setting() {
        let source = "https://example.test/field-notes";
        let mut app = App::test_stub();
        app.place = offline(snapshot("a body-only phrase", source));
        app.update(Action::SearchCapturedPages);
        app.update(Action::OmnibarInsert("body-only phrase".into()));
        app.recompute_captured_page_suggestions(1);
        assert!(matches!(app.omnibar.suggestions.as_slice(),
            [Suggestion::Act { action: Action::OpenAddress(url), .. }] if url == source));
        app.omnibar.text = "no-matching-content".into();
        app.recompute_captured_page_suggestions(1);
        assert_eq!(
            app.omnibar.suggestions,
            vec![Suggestion::Hint("no captured pages match")]
        );
    }

    #[test]
    fn committing_a_captured_source_opens_its_source_address() {
        let source = "https://example.test/field-notes";
        let mut app = App::test_stub();
        app.place = offline(snapshot("a body-only phrase", source));
        app.update(Action::SearchCapturedPages);
        app.update(Action::OmnibarInsert("body-only phrase".into()));

        let effects = app.update(Action::OmnibarCommit);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::FetchPage { url, .. } if url == source
        )));
        assert_eq!(app.graph_runtimes.focused_url(), Some(source));
        let entry = app
            .shell_transcript()
            .entries()
            .last()
            .expect("captured source navigation enters the shell transcript");
        assert!(matches!(
            &entry.resolved_intent,
            crate::shell_services::ShellIntent::Navigate { url } if url == source
        ));
    }
}
