// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(in super::super) fn commit_omnibar(&mut self) -> Vec<Effect> {
        if let crate::ui::OmnibarMode::MicronForm(prompt) = self.omnibar.mode.clone() {
            return self.commit_micron_form(prompt);
        }
        if let crate::ui::OmnibarMode::SmolwebSubmission(submission) = self.omnibar.mode.clone() {
            return self.commit_smolweb_submission(submission);
        }
        if matches!(
            self.omnibar.mode,
            crate::ui::OmnibarMode::SmolwebSubmissionResult(_)
        ) {
            return self.close_omnibar();
        }
        if let crate::ui::OmnibarMode::GeminiTrust(input) = self.omnibar.mode.clone() {
            return self.commit_gemini_trust(input);
        }
        if let crate::ui::OmnibarMode::GeminiIdentity(input) = self.omnibar.mode.clone() {
            return self.commit_gemini_identity(input);
        }
        if let crate::ui::OmnibarMode::SmolwebInput(input) = self.omnibar.mode.clone() {
            return self.commit_smolweb_input(input);
        }
        // Place prompts capture the whole line, exactly like rename.
        if let crate::ui::OmnibarMode::Place(prompt) = self.omnibar.mode {
            return self.commit_place_prompt(prompt);
        }
        // Rename mode captures the whole line as the new name and
        // commits it, bypassing the find/go/actions lanes.
        if let crate::ui::OmnibarMode::RenameSession(id) = self.omnibar.mode {
            let name = self.omnibar.text.clone();
            let target = self.fallback_shell_context();
            let entry = self.shell.record_omnibar(
                ShellInput::Omnibar(name.clone()),
                ShellIntent::Command {
                    label: "Rename session".into(),
                    action: Action::RenameSession {
                        id,
                        name: name.clone(),
                    },
                },
                target,
                EntryPrivacy::Ordinary,
            );
            self.omnibar = OmnibarState::default();
            if self.focus == FocusTarget::Chrome {
                self.focus = FocusTarget::Graph(self.default_graph_pane());
            }
            let mut fx = self.update(Action::RenameSession { id, name });
            self.shell.complete(
                entry,
                ShellOutcome::Completed {
                    summary: "renamed session".into(),
                },
            );
            fx.push(Effect::Redraw);
            return fx;
        }
        // "All commands…" expands the lane rather than running a command, so
        // the palette stays open and keeps focus.
        if matches!(
            self.omnibar.selection(),
            Some(Suggestion::Act {
                action: Action::OmnibarShowAllCommands,
                ..
            })
        ) {
            return self.show_all_commands();
        }
        let location_graph = self
            .reading
            .scope
            .editing_location
            .then(|| self.scope_place().graph);
        let reading = std::mem::take(&mut self.reading.scope.editing_location)
            || self
                .active_pane
                .is_some_and(|pane| self.pane_content(pane) == Some(&PaneContent::Workbench));
        // Commit always ends with the omnibar closed, so chrome hands
        // focus back to the canvas. (A committed OpenAddress may later
        // spawn content; routing focus onto it is slice B.)
        if self.focus == FocusTarget::Chrome {
            self.focus = FocusTarget::Graph(self.default_graph_pane());
        }
        let committed = self.omnibar.selection().cloned().or_else(|| {
            normalize_address(self.omnibar.text.trim()).map(|url| Suggestion::Go { url })
        });
        if let Some(s) = committed.as_ref() {
            self.events
                .push(AppEvent::OmnibarCommitted(crate::observe::suggestion_line(
                    s,
                )));
        }
        let mut effects = match committed {
            Some(Suggestion::Node { url, .. }) => {
                // Find lane: select the existing node; never refetch.
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::SelectNode { url: url.clone() },
                    target,
                    EntryPrivacy::Ordinary,
                );
                let pane = location_graph
                    .and_then(|graph| self.graph_pane_for_navigation(graph))
                    .or_else(|| self.focused_graph_pane())
                    .unwrap_or_else(|| self.default_graph_pane());
                let mut effects = vec![Effect::Redraw];
                if reading {
                    // Resolve without selecting first: Read captures the current
                    // reading before it changes graph focus or the active tab.
                    let target = self.graph_for_pane(pane).and_then(|graph| {
                        self.graph_runtimes
                            .canvas(graph)
                            .and_then(|canvas| canvas.graph().get_node_by_url(&url))
                            .map(|(_, node)| (graph, node.id))
                    });
                    if let Some((graph, member)) = target {
                        effects.extend(self.read_member_in_graph(graph, member));
                    }
                } else {
                    self.with_graph_pane(pane, |canvas| canvas.select_by_url(&url));
                }
                self.shell.complete(
                    entry,
                    ShellOutcome::Completed {
                        summary: format!("selected {url}"),
                    },
                );
                effects
            },
            Some(Suggestion::Go { url }) => {
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::Navigate { url: url.clone() },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.open_omnibar_address(url, reading);
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: "opened address".into(),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            },
            Some(Suggestion::Recall { url, .. }) => {
                // The recall lane: a page out of browsing memory opens
                // exactly as a typed address does. Its transcript intent is
                // Navigate for the same reason — where the row came from is
                // provenance, and the act is still opening an address.
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::Navigate { url: url.clone() },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.open_omnibar_address(url, reading);
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: "opened a recalled address".into(),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            },
            Some(Suggestion::Act { label, action }) => {
                self.record_command_use(&label);
                // An action row normally records a command. A row that opens
                // an address, including captured-page sources, remains
                // navigation in the transcript so replay keeps its meaning.
                let target = self.fallback_shell_context();
                let (intent, summary) = match &action {
                    Action::OpenAddress(url) => (
                        ShellIntent::Navigate { url: url.clone() },
                        format!("opened {url}"),
                    ),
                    _ => (
                        ShellIntent::Command {
                            label: label.clone(),
                            action: action.clone(),
                        },
                        format!("ran {label}"),
                    ),
                };
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    intent,
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.update(action);
                    self.shell
                        .complete(entry, ShellOutcome::Completed { summary });
                    fx.push(Effect::Redraw);
                    fx
                };
            },
            Some(Suggestion::Hint(_) | Suggestion::Prompt(_)) | None => vec![Effect::Redraw],
        };
        self.omnibar = OmnibarState::default();
        self.shell.close_omnibar();
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn commit_smolweb_input(
        &mut self,
        input: crate::ui::SmolwebInputPrompt,
    ) -> Vec<Effect> {
        let current = self
            .graph_runtimes
            .graph_containing_member(input.node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| {
                crate::browse::still_current(canvas, input.node, &input.requested_url)
            });
        if !current {
            self.omnibar = OmnibarState::default();
            self.shell.close_omnibar();
            self.focus = FocusTarget::Graph(self.default_graph_pane());
            return vec![Effect::Redraw];
        }
        let Some(request_url) = smolweb_query_url(&input.input_url, &self.omnibar.text) else {
            return vec![Effect::Redraw];
        };
        let resume_content = matches!(
            self.content.get(input.node),
            Some(crate::content::NodeContent::AwaitingInput)
        );

        // Ordinary search input is addressable browsing state. Status 11 is
        // password-like: the query goes over the wire but never enters graph
        // truth, history, observation, or the interaction transcript.
        let owner_url = if input.sensitive {
            input.requested_url.clone()
        } else {
            let Some(graph) = self.graph_runtimes.graph_containing_member(input.node) else {
                return vec![Effect::Redraw];
            };
            let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
                return vec![Effect::Redraw];
            };
            if !canvas.navigate_member(input.node, &request_url) {
                return vec![Effect::Redraw];
            }
            self.history.visit(request_url.clone());
            self.events.push(AppEvent::ContentNavigated {
                node: input.node,
                url: request_url.clone(),
            });
            request_url.clone()
        };
        self.content.forget_fetched(input.node);
        if resume_content {
            self.content.note_requested(input.node);
            self.events.push(AppEvent::ContentState {
                node: input.node,
                state: "requested".to_string(),
            });
        }
        self.events.push(AppEvent::SmolwebInputSubmitted {
            node: input.node,
            sensitive: input.sensitive,
        });
        self.omnibar = OmnibarState::default();
        self.recall.clear();
        self.recall_query.clear();
        self.shell.close_omnibar();
        self.focus = FocusTarget::Graph(self.default_graph_pane());

        vec![
            self.fetch_page_effect(input.node, request_url, owner_url),
            Effect::SaveSession,
            Effect::Redraw,
        ]
    }

    pub(super) fn commit_smolweb_submission(
        &mut self,
        mut submission: crate::ui::SmolwebSubmissionPrompt,
    ) -> Vec<Effect> {
        use crate::ui::{SmolwebSubmissionProtocol as Protocol, SmolwebSubmissionStage as Stage};

        match submission.stage {
            Stage::Body => {
                if submission.file_name.is_none() || !self.omnibar.text.is_empty() {
                    submission.body = self.omnibar.text.as_bytes().to_vec();
                    submission.file_name = None;
                }
                submission.stage = match submission.protocol {
                    Protocol::Titan => Stage::Mime,
                    Protocol::Spartan => Stage::Confirm,
                };
                self.omnibar.text = if submission.stage == Stage::Mime {
                    submission.mime.clone()
                } else {
                    String::new()
                };
                self.omnibar.cursor = self.omnibar.text.len();
                self.omnibar.mode = crate::ui::OmnibarMode::SmolwebSubmission(submission);
                self.recompute_omnibar_suggestions();
                return vec![Effect::Redraw];
            },
            Stage::Mime => {
                let mime = self.omnibar.text.trim();
                submission.mime = if mime.is_empty() {
                    "application/octet-stream".to_string()
                } else {
                    mime.to_string()
                };
                submission.stage = Stage::Token;
                self.omnibar.text.clear();
                self.omnibar.cursor = 0;
                self.omnibar.mode = crate::ui::OmnibarMode::SmolwebSubmission(submission);
                self.recompute_omnibar_suggestions();
                return vec![Effect::Redraw];
            },
            Stage::Token => {
                submission.token = (!self.omnibar.text.is_empty())
                    .then(|| crate::action::SensitiveString::new(self.omnibar.text.clone()));
                submission.stage = Stage::Confirm;
                self.omnibar.text.clear();
                self.omnibar.cursor = 0;
                self.omnibar.mode = crate::ui::OmnibarMode::SmolwebSubmission(submission);
                self.recompute_omnibar_suggestions();
                return vec![Effect::Redraw];
            },
            Stage::Confirm if !self.omnibar.text.trim().eq_ignore_ascii_case("send") => {
                return vec![Effect::Redraw];
            },
            Stage::Confirm => {},
        }

        self.next_smolweb_submission = self.next_smolweb_submission.wrapping_add(1);
        let request = self.next_smolweb_submission;
        self.active_smolweb_submission = Some(request);
        let identity = if submission.protocol == Protocol::Titan {
            self.gemini_identity_for(&submission.target)
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "failed to project Titan client identity");
                    None
                })
        } else {
            None
        };
        self.events.push(AppEvent::SmolwebSubmissionStarted {
            target: submission.target.clone(),
            bytes: submission.body.len(),
        });
        self.omnibar = crate::ui::OmnibarState {
            open: true,
            mode: crate::ui::OmnibarMode::SmolwebSubmissionResult(
                crate::ui::SmolwebSubmissionResult {
                    target: submission.target.clone(),
                    message: "sending".to_string(),
                },
            ),
            ..Default::default()
        };
        self.recompute_omnibar_suggestions();
        vec![
            Effect::SubmitSmolweb {
                request,
                source: submission.source,
                target: submission.target,
                protocol: submission.protocol,
                body: submission.body,
                mime: submission.mime,
                token: submission.token,
                identity,
            },
            Effect::Redraw,
        ]
    }

    pub(in super::super) fn compose_focused_smolweb_submission(&mut self) -> Vec<Effect> {
        let Some(source) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        let Some(target) = self.focused_address() else {
            return vec![Effect::Redraw];
        };
        self.begin_smolweb_submission(Some(source), target)
    }
}
