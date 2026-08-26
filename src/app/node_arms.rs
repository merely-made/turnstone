//! Node and view arms: what a node IS and how it is shown.
//!
//! Deleting stages into the recycle bin and recovery restores the ORIGINAL
//! id, which is the whole identity contract the bin exists for.

use uuid::Uuid;

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::panes::PaneContent;
use crate::shell_services::{EntryPrivacy, ShellInput, ShellIntent, ShellOutcome};
use crate::surface::FocusTarget;
use crate::ui::{OmnibarState, Suggestion, normalize_address};

use super::App;

/// Replace a smolweb input target's query with one UTF-8, percent-encoded
/// answer. Form-style `+` encoding is wrong here: Gemini input is a URL query,
/// where a space is `%20`.
pub(crate) fn smolweb_query_url(input_url: &str, answer: &str) -> Option<String> {
    use std::fmt::Write as _;

    let mut parsed = url::Url::parse(input_url).ok()?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    let mut target = parsed.to_string();
    target.push('?');
    for byte in answer.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            target.push(*byte as char);
        } else {
            let _ = write!(target, "%{byte:02X}");
        }
    }
    Some(target)
}

impl App {
    pub(crate) fn focused_can_back(&self) -> bool {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return false;
        };
        self.graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| canvas.member_can_back(node))
    }

    pub(crate) fn focused_can_forward(&self) -> bool {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return false;
        };
        self.graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| canvas.member_can_forward(node))
    }

    /// Whether one exact member carries the durable keep tag. This is graph
    /// truth, shared by chrome, the command catalog, feeds, and persistence.
    pub(crate) fn node_is_kept(&self, member: Uuid) -> bool {
        self.graph_runtimes
            .graph_containing_member(member)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .and_then(|canvas| {
                let (key, _) = canvas.graph().get_node_by_id(member)?;
                canvas.graph().node_tags(key)
            })
            .is_some_and(|tags| tags.contains(crate::feed::KEEP_TAG))
    }

    /// Promote one captured member into durable kept state. Idempotence makes
    /// repeated UI or automation delivery harmless. Release belongs to the
    /// retention-policy lane rather than being an accidental toggle here.
    pub(super) fn keep_node(&mut self, member: Uuid) -> Vec<Effect> {
        if self.node_is_kept(member) {
            return vec![Effect::Redraw];
        }
        let changed = self
            .graph_runtimes
            .graph_containing_member(member)
            .and_then(|graph| self.graph_runtimes.canvas_mut(graph))
            .is_some_and(|canvas| canvas.tag_node(member, crate::feed::KEEP_TAG));
        if !changed {
            return vec![Effect::Redraw];
        }
        self.events.push(AppEvent::NodeKept(member));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(crate) fn set_link_preview(&mut self, preview: Option<String>) -> bool {
        if self.link_preview == preview {
            false
        } else {
            self.link_preview = preview;
            true
        }
    }

    pub(super) fn delete_focused_node(&mut self) -> Vec<Effect> {
        // Build the bin record off the LIVING node (identity, url,
        // title, tags — everything recovery restores), then drop the
        // node and reap what hung off it: the live content session
        // and any workbench tile. The record stages through the bin
        // port (Effect::RecordDeleted); the actor answers with the
        // refreshed list, so `removed` mirrors the store, never a
        // hand-kept copy.
        let record = self.graph_runtimes.focused_member().and_then(|m| {
            let graph = self.graph_runtimes.graph();
            let (key, node) = graph.get_node_by_id(m)?;
            let title = node.title.trim();
            // The node's whole character rides the tombstone: its
            // borne world (by id) and its facet bundle, so recovery
            // restores residency/arrangement/web state, not just
            // identity.
            let facets = self.graph_runtimes.facets().facets_of(&m).map(|f| {
                serde_json::Value::Object(
                    f.iter()
                        .map(|(id, value)| (id.as_str().to_string(), value.clone()))
                        .collect(),
                )
            });
            Some(crate::action::RemovedRecord {
                node_id: node.id,
                url: node.url().to_string(),
                title: (!title.is_empty() && title != node.url()).then(|| title.to_string()),
                tags: graph
                    .node_tags(key)
                    .map(|t| t.iter().cloned().collect())
                    .unwrap_or_default(),
                deleted_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                nested: node.nested.as_ref().map(|log| log.as_str().to_string()),
                facets,
            })
        });
        let Some(record) = record else {
            return vec![Effect::Redraw];
        };
        let feed_entries = self.feeds.entry_members(record.node_id);
        // Archive-never-orphan: the world's file moves to the archive
        // slot BEFORE the bearing node leaves; a failed archive
        // aborts the delete (the node stays, nothing is lost).
        if let Some(log_id) = &record.nested
            && let Err(err) = crate::denizen::archive_world(&self.session_dir(), log_id)
        {
            tracing::warn!(%err, log_id, "world archive failed; delete aborted");
            return vec![Effect::Redraw];
        }
        let Some(member) = self.graph_runtimes.remove_focused() else {
            // The node did not leave after all: put the world back.
            if let Some(log_id) = &record.nested {
                let _ = crate::denizen::unarchive_world(&self.session_dir(), log_id);
            }
            return vec![Effect::Redraw];
        };
        self.feeds.forget_member(member);
        for entry in feed_entries {
            if let Some(graph) = self.graph_runtimes.graph_containing_member(entry)
                && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
            {
                canvas.untag_node(entry, crate::feed::UNREAD_TAG);
            }
        }
        // The record is the archive now: the live facets go, and a
        // denizen's runtime entry goes with its node.
        if self.denizens.residents.remove(&member).is_some() {
            let sdir = self.session_dir();
            self.denizens = crate::denizen::rebuild(
                self.graph_runtimes.facets(),
                self.graph_runtimes.graph(),
                &sdir,
                self.identity.as_ref(),
            );
        }
        for runtime in self.forme_runtimes.iter_mut() {
            runtime.workbench.close_tile(member);
        }
        self.content.forget_node(member);
        self.events.push(AppEvent::NodeRemoved(record.url.clone()));
        vec![
            Effect::RecordDeleted { record },
            Effect::CloseContent { node: member },
            Effect::SaveSession,
            Effect::Redraw,
        ]
    }

    pub(super) fn recover_deleted_node(&mut self, id: Uuid) -> Vec<Effect> {
        // Recover from the bin mirror BY IDENTITY: the node re-mints
        // under its ORIGINAL id with its recorded title/tags (the
        // canvas guards idempotency), gets selected + centered, joins
        // the visit history, and refetches. The bin record stays in
        // the store (append-only until athanor's pass); the Trail's
        // Removed section derives it away because the node is present
        // again.
        let Some(record) = self.removed.iter().find(|r| r.node_id == id).cloned() else {
            return vec![Effect::Redraw];
        };
        let member = self.graph_runtimes.recover_node(
            record.node_id,
            &record.url,
            record.title.as_deref(),
            &record.tags,
        );
        // Residency came back; its standing subscriptions have to come with
        // it, or a behavior silently stops waking after a reload.
        (
            self.watches,
            self.app_watches,
            self.time_watches,
            self.deadbands,
        ) = crate::denizen::load_watches(&self.session_dir());
        // Restore the node's character from the tombstone: the facet
        // bundle whole, then the borne world (file back to the live
        // slot, pointer re-borne through the spine), then the denizen
        // runtime so a recovered resident resides again.
        if let Some(serde_json::Value::Object(map)) = &record.facets {
            for (facet_id, value) in map {
                let _ = self.graph_runtimes.facets_mut().set(
                    member,
                    chartulary::FacetId::new(facet_id.as_str()),
                    value.clone(),
                    &chartulary::AcceptAll,
                );
            }
        }
        if let Some(log_id) = &record.nested {
            let sdir = self.session_dir();
            if let Err(err) = crate::denizen::unarchive_world(&sdir, log_id) {
                tracing::warn!(%err, log_id, "world unarchive failed; recovering empty");
            }
            let _ = self.graph_runtimes.set_node_nested_for(
                member,
                Some(mere::kernel::graph::LogId::new(log_id.clone())),
            );
            self.denizens = crate::denizen::rebuild(
                self.graph_runtimes.facets(),
                self.graph_runtimes.graph(),
                &sdir,
                self.identity.as_ref(),
            );
            // Residency came back; its standing subscriptions come with it, or
            // a recovered behavior silently stops waking.
            (
                self.watches,
                self.app_watches,
                self.time_watches,
                self.deadbands,
            ) = crate::denizen::load_watches(&sdir);
        }
        self.graph_runtimes.center_on_selected();
        self.history.visit(record.url.clone());
        self.events
            .push(AppEvent::NodeRecovered(record.url.clone()));
        let mut effects = vec![Effect::SaveSession, Effect::Redraw];
        if fetch::is_fetchable(&record.url) {
            effects.push(self.fetch_page_effect(member, record.url.clone(), record.url.clone()));
        }
        effects
    }

    pub(super) fn set_viewer_override(
        &mut self,
        member: Uuid,
        viewer: Option<String>,
    ) -> Vec<Effect> {
        let viewer = super::canonical_viewer_override(viewer);
        self.browser.entry(member).viewer_override = viewer.clone();
        self.events.push(AppEvent::ViewerChanged {
            node: member,
            viewer: viewer.clone().unwrap_or_else(|| "auto".to_string()),
        });
        let mut effects = Vec::new();
        // Live (or in-flight) content respawns through the now-pinned
        // route, so the setting is seen applying (the Reload shape).
        if matches!(
            self.content.get(member),
            Some(crate::content::NodeContent::Live | crate::content::NodeContent::Requested)
        ) && let Some(url) = self
            .graph_runtimes
            .graph()
            .nodes()
            .find(|(_, n)| n.id == member)
            .map(|(_, n)| n.url().to_string())
        {
            self.content.note_requested(member);
            self.events.push(AppEvent::ContentState {
                node: member,
                state: "requested".to_string(),
            });
            effects.push(Effect::CloseContent { node: member });
            effects.push(Effect::SpawnContent { node: member, url });
        }
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn toggle_node_content(&mut self) -> Vec<Effect> {
        // The flip targets the focused node; no focus, no-op (the
        // caption chip tells the user what would flip).
        // Resolve the node by MEMBER, not by URL round-trip: two
        // nodes may share a URL (the sample graph + an open), and
        // get_node_by_url picks arbitrarily between them.
        let Some(target) = self
            .graph_runtimes
            .focused_member()
            .zip(self.graph_runtimes.focused_url().map(str::to_string))
        else {
            return Vec::new();
        };
        let (node, url) = target;
        if self.content.flip_spawns(node) {
            self.content.note_requested(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "requested".to_string(),
            });
            vec![Effect::SpawnContent { node, url }, Effect::Redraw]
        } else {
            self.content.note_closed(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "closed".to_string(),
            });
            vec![Effect::CloseContent { node }, Effect::Redraw]
        }
    }

    pub(super) fn reload_focused(&mut self) -> Vec<Effect> {
        let Some(target) = self
            .graph_runtimes
            .focused_member()
            .zip(self.graph_runtimes.focused_url().map(str::to_string))
        else {
            return vec![Effect::Redraw];
        };
        let (node, url) = target;
        self.events.push(AppEvent::Reloaded(url.clone()));
        self.link_preview = None;

        if self.node_uses_web_surface(node) {
            self.content.note_surface_started(node);
            return vec![
                Effect::ControlContent {
                    node,
                    control: crate::action::ContentControl::Reload,
                },
                Effect::Redraw,
            ];
        }

        self.content.forget_fetched(node);
        let mut effects = Vec::new();
        if fetch::is_fetchable(&url) {
            effects.push(self.fetch_page_effect(node, url.clone(), url.clone()));
        }
        // A live (or in-flight) session respawns fresh; a node
        // without content stays without (reload is not a spawn).
        if matches!(
            self.content.get(node),
            Some(crate::content::NodeContent::Live | crate::content::NodeContent::Requested)
        ) {
            self.content.note_requested(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "requested".to_string(),
            });
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::SpawnContent { node, url });
        }
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn stop_focused(&mut self) -> Vec<Effect> {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        self.link_preview = None;
        if let Some(request) = self.content.active_fetch(node) {
            if self.content.stop_fetch(node, request) {
                self.events.push(AppEvent::ContentState {
                    node,
                    state: "stopped".to_string(),
                });
                return vec![Effect::CancelPage { request, node }, Effect::Redraw];
            }
        }
        if self.node_uses_web_surface(node) && self.content.fetch_in_progress(node) {
            self.content.note_surface_stopped(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "stopped".to_string(),
            });
            return vec![
                Effect::ControlContent {
                    node,
                    control: crate::action::ContentControl::Stop,
                },
                Effect::Redraw,
            ];
        }
        vec![Effect::Redraw]
    }

    pub(super) fn commit_omnibar(&mut self) -> Vec<Effect> {
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
                self.graph_runtimes.select_by_url(&url);
                self.shell.complete(
                    entry,
                    ShellOutcome::Completed {
                        summary: format!("selected {url}"),
                    },
                );
                vec![Effect::Redraw]
            }
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
                    let mut fx = self.update(Action::OpenAddress(url));
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: "opened address".into(),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            }
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
                    let mut fx = self.update(Action::OpenAddress(url));
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: "opened a recalled address".into(),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            }
            Some(Suggestion::Act { label, action }) => {
                // The actions lane: the committed registry entry is
                // an ordinary Action; lower it through the same
                // spine everything else uses.
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::Command {
                        label: label.clone(),
                        action: action.clone(),
                    },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.update(action);
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: format!("ran {label}"),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            }
            Some(Suggestion::Hint(_) | Suggestion::Prompt(_)) | None => vec![Effect::Redraw],
        };
        self.omnibar = OmnibarState::default();
        self.shell.close_omnibar();
        effects.push(Effect::Redraw);
        effects
    }

    fn commit_smolweb_input(&mut self, input: crate::ui::SmolwebInputPrompt) -> Vec<Effect> {
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

    fn commit_smolweb_submission(
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
            }
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
            }
            Stage::Token => {
                submission.token = (!self.omnibar.text.is_empty())
                    .then(|| crate::action::SensitiveString::new(self.omnibar.text.clone()));
                submission.stage = Stage::Confirm;
                self.omnibar.text.clear();
                self.omnibar.cursor = 0;
                self.omnibar.mode = crate::ui::OmnibarMode::SmolwebSubmission(submission);
                self.recompute_omnibar_suggestions();
                return vec![Effect::Redraw];
            }
            Stage::Confirm if !self.omnibar.text.trim().eq_ignore_ascii_case("send") => {
                return vec![Effect::Redraw];
            }
            Stage::Confirm => {}
        }

        self.next_smolweb_submission = self.next_smolweb_submission.wrapping_add(1);
        let request = self.next_smolweb_submission;
        self.active_smolweb_submission = Some(request);
        let identity = if submission.protocol == Protocol::Titan {
            self.gemini_identities
                .identity_for(self.identity.as_ref(), &submission.target)
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

    pub(super) fn compose_focused_smolweb_submission(&mut self) -> Vec<Effect> {
        let Some(source) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        let Some(target) = self.focused_address() else {
            return vec![Effect::Redraw];
        };
        self.begin_smolweb_submission(Some(source), target)
    }

    pub(super) fn begin_smolweb_submission(
        &mut self,
        source: Option<Uuid>,
        target: String,
    ) -> Vec<Effect> {
        let base = source.and_then(|member| {
            self.graph_runtimes
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url().to_string())
        });
        let resolved = url::Url::parse(&target).or_else(|_| {
            base.as_deref()
                .ok_or(url::ParseError::RelativeUrlWithoutBase)
                .and_then(|base| url::Url::parse(base)?.join(&target))
        });
        let Ok(resolved) = resolved else {
            self.events.push(AppEvent::SmolwebSubmissionFailed {
                target,
                error: "invalid submission target".to_string(),
            });
            return vec![Effect::Redraw];
        };
        let protocol = match resolved.scheme() {
            "titan" => crate::ui::SmolwebSubmissionProtocol::Titan,
            "spartan" => crate::ui::SmolwebSubmissionProtocol::Spartan,
            _ => {
                self.events.push(AppEvent::SmolwebSubmissionFailed {
                    target: resolved.to_string(),
                    error: "submission target must use titan:// or spartan://".to_string(),
                });
                return vec![Effect::Redraw];
            }
        };
        let target_context = self.fallback_shell_context();
        self.shell.begin_omnibar(target_context);
        self.omnibar = crate::ui::OmnibarState {
            open: true,
            mode: crate::ui::OmnibarMode::SmolwebSubmission(crate::ui::SmolwebSubmissionPrompt {
                source,
                target: resolved.to_string(),
                protocol,
                stage: crate::ui::SmolwebSubmissionStage::Body,
                body: Vec::new(),
                file_name: None,
                mime: "text/gemini".to_string(),
                token: None,
            }),
            ..Default::default()
        };
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        self.events.push(AppEvent::SmolwebSubmissionComposed {
            target: resolved.to_string(),
        });
        vec![Effect::Redraw]
    }

    pub(super) fn set_smolweb_submission_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        suggested_mime: String,
    ) -> Vec<Effect> {
        const CAP: usize = 64 * 1024 * 1024;
        let crate::ui::OmnibarMode::SmolwebSubmission(mut submission) = self.omnibar.mode.clone()
        else {
            return vec![Effect::Redraw];
        };
        if submission.stage != crate::ui::SmolwebSubmissionStage::Body || bytes.len() > CAP {
            return vec![Effect::Redraw];
        }
        submission.body = bytes;
        submission.file_name = Some(name);
        submission.mime = suggested_mime;
        self.omnibar.text.clear();
        self.omnibar.cursor = 0;
        self.omnibar.mode = crate::ui::OmnibarMode::SmolwebSubmission(submission);
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    fn commit_gemini_identity(&mut self, input: crate::ui::GeminiIdentityPrompt) -> Vec<Effect> {
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

        let origin = match self
            .gemini_identities
            .bind(self.identity.as_ref(), &input.identity_url)
        {
            Ok(origin) => origin,
            Err(error) => {
                self.omnibar = OmnibarState::default();
                self.shell.close_omnibar();
                self.focus = FocusTarget::Graph(self.default_graph_pane());
                if matches!(
                    self.content.get(input.node),
                    Some(crate::content::NodeContent::AwaitingIdentity)
                ) {
                    self.content.note_failed(input.node, error.clone());
                    self.events.push(AppEvent::ContentState {
                        node: input.node,
                        state: format!("failed: {error}"),
                    });
                }
                return vec![Effect::CloseContent { node: input.node }, Effect::Redraw];
            }
        };
        let fetch =
            self.fetch_page_effect(input.node, input.identity_url.clone(), input.requested_url);
        if !matches!(
            &fetch,
            Effect::FetchPage {
                identity: Some(_),
                ..
            }
        ) {
            let error = "could not mint Gemini client identity".to_string();
            self.content.note_failed(input.node, error.clone());
            self.events.push(AppEvent::ContentState {
                node: input.node,
                state: format!("failed: {error}"),
            });
            self.omnibar = OmnibarState::default();
            self.shell.close_omnibar();
            self.focus = FocusTarget::Graph(self.default_graph_pane());
            return vec![Effect::CloseContent { node: input.node }, Effect::Redraw];
        }

        if matches!(
            self.content.get(input.node),
            Some(crate::content::NodeContent::AwaitingIdentity)
        ) {
            self.content.note_requested(input.node);
            self.events.push(AppEvent::ContentState {
                node: input.node,
                state: "requested".to_string(),
            });
        }
        self.events.push(AppEvent::GeminiIdentityBound {
            node: input.node,
            origin,
        });
        self.omnibar = OmnibarState::default();
        self.recall.clear();
        self.recall_query.clear();
        self.shell.close_omnibar();
        self.focus = FocusTarget::Graph(self.default_graph_pane());
        vec![fetch, Effect::SaveSession, Effect::Redraw]
    }

    fn commit_gemini_trust(&mut self, input: crate::ui::GeminiTrustPrompt) -> Vec<Effect> {
        if !self.omnibar.text.trim().eq_ignore_ascii_case("trust") {
            return vec![Effect::Redraw];
        }
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

        if matches!(
            self.content.get(input.node),
            Some(crate::content::NodeContent::AwaitingTrust)
        ) {
            self.content.note_requested(input.node);
            self.events.push(AppEvent::ContentState {
                node: input.node,
                state: "requested".to_string(),
            });
        }
        self.omnibar = OmnibarState::default();
        self.recall.clear();
        self.recall_query.clear();
        self.shell.close_omnibar();
        self.focus = FocusTarget::Graph(self.default_graph_pane());
        vec![
            Effect::ReplaceGeminiTrust {
                node: input.node,
                fetch_url: input.fetch_url,
                owner_url: input.requested_url,
                target: input.target,
                pinned: input.pinned,
                seen: input.seen,
            },
            Effect::Redraw,
        ]
    }

    pub(super) fn open_address(&mut self, url: String) -> Vec<Effect> {
        self.events.push(AppEvent::AddressOpened(url.clone()));
        // A graph pane owns its selection. Visiting through the compatibility
        // canvas cursor selected the node only until the next render installed
        // that pane's saved selection, at which point an address opened from
        // the omnibar became unfocused and could not spawn or retarget content.
        let pane = self
            .focused_graph_pane()
            .unwrap_or_else(|| self.default_graph_pane());
        let Some(key) = self.with_graph_pane(pane, |canvas| canvas.visit(&url)) else {
            return vec![Effect::Redraw];
        };
        self.history.visit(url.clone());
        let mut effects = vec![Effect::Redraw];
        if url::Url::parse(&url).is_ok_and(|parsed| parsed.scheme() == "titan")
            && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            effects.extend(self.begin_smolweb_submission(Some(node), url));
        } else if fetch::is_fetchable(&url)
            && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            effects.push(self.fetch_page_effect(node, url.clone(), url));
        }
        effects
    }

    pub(super) fn commit_content_navigation(&mut self, member: Uuid, url: String) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            tracing::warn!(%member, %url, "content navigation named an unknown member");
            return Vec::new();
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        let already_current = canvas
            .graph()
            .get_node_by_id(member)
            .is_some_and(|(_, node)| node.url() == url);
        if already_current || !canvas.navigate_member(member, &url) {
            return Vec::new();
        }
        self.events
            .push(AppEvent::ContentNavigated { node: member, url });
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn set_content_title(&mut self, member: Uuid, title: String) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            tracing::warn!(%member, "content title named an unknown member");
            return Vec::new();
        };
        let changed = self
            .graph_runtimes
            .canvas_mut(graph)
            .is_some_and(|canvas| canvas.set_node_title_for(member, title.clone()));
        if !changed {
            return Vec::new();
        }
        self.events.push(AppEvent::ContentTitleChanged {
            node: member,
            title,
        });
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn nav_back(&mut self) -> Vec<Effect> {
        self.navigate_focused_lineage(true)
    }

    pub(super) fn nav_forward(&mut self) -> Vec<Effect> {
        self.navigate_focused_lineage(false)
    }

    fn navigate_focused_lineage(&mut self, back: bool) -> Vec<Effect> {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        if self.node_uses_web_surface(node) {
            let allowed = self
                .graph_runtimes
                .graph_containing_member(node)
                .and_then(|graph| self.graph_runtimes.canvas(graph))
                .is_some_and(|canvas| {
                    if back {
                        canvas.member_can_back(node)
                    } else {
                        canvas.member_can_forward(node)
                    }
                });
            if !allowed {
                return vec![Effect::Redraw];
            }
            self.content.note_surface_started(node);
            return vec![
                Effect::ControlContent {
                    node,
                    control: if back {
                        crate::action::ContentControl::Back
                    } else {
                        crate::action::ContentControl::Forward
                    },
                },
                Effect::Redraw,
            ];
        }

        let Some(graph) = self.graph_runtimes.graph_containing_member(node) else {
            return vec![Effect::Redraw];
        };
        let Some(url) = self.graph_runtimes.canvas_mut(graph).and_then(|canvas| {
            if back {
                canvas.member_history_back(node)
            } else {
                canvas.member_history_forward(node)
            }
        }) else {
            return vec![Effect::Redraw];
        };
        if back {
            self.events.push(AppEvent::NavigatedBack(url.clone()));
        } else {
            self.events.push(AppEvent::NavigatedForward(url.clone()));
        }
        self.link_preview = None;
        self.content.forget_fetched(node);
        let content_on = matches!(
            self.content.get(node),
            Some(crate::content::NodeContent::Live | crate::content::NodeContent::Requested)
        );
        let mut effects = vec![Effect::SaveSession];
        if fetch::is_fetchable(&url) {
            effects.push(self.fetch_page_effect(node, url.clone(), url.clone()));
        }
        if content_on {
            self.content.note_requested(node);
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::SpawnContent { node, url });
        }
        effects.push(Effect::Redraw);
        effects
    }

    fn node_uses_web_surface(&self, node: Uuid) -> bool {
        self.content
            .facts(node)
            .is_some_and(|facts| facts.engine == inker::routing::ENGINE_WELD_CHROMIUM)
    }

    pub(super) fn reseed_layout(&mut self) -> Vec<Effect> {
        if self.graph_runtimes.reseed() {
            self.events.push(AppEvent::LayoutReseeded);
            vec![Effect::Redraw]
        } else {
            Vec::new()
        }
    }

    pub(super) fn set_layout_strategy(&mut self, id: Option<&'static str>) -> Vec<Effect> {
        self.graph_runtimes
            .set_layout_strategy(id.map(str::to_string));
        if id != Some("phyllotaxis.default") {
            self.graph_runtimes.set_projection_score(None);
        }
        // The projection itself is computed on the next frame by
        // `drive_layout_strategy` (it needs the surface viewport).
        vec![Effect::Redraw]
    }

    pub(super) fn toggle_size_by_recency(&mut self) -> Vec<Effect> {
        let on = !self.graph_runtimes.size_by_recency();
        self.graph_runtimes.set_size_by_recency(on);
        // A size change moves extents and the recency ordering, so the
        // active analytic layout must recompute; re-selecting the same
        // strategy drops its input cache (last_strategy_inputs = None).
        let active = self.graph_runtimes.layout_strategy().map(str::to_string);
        self.graph_runtimes.set_layout_strategy(active);
        vec![Effect::Redraw]
    }

    pub(super) fn set_node_sprite(
        &mut self,
        member: Uuid,
        data_uri: String,
        hull: Vec<(f32, f32)>,
    ) -> Vec<Effect> {
        self.graph_runtimes.set_node_sprite(member, data_uri);
        // The traced collider: the node collides at its picture. Under
        // 3 points the tracer found no opaque region — keep the
        // silhouette collider rather than installing a degenerate one.
        if hull.len() >= 3 {
            self.graph_runtimes.set_node_sprite_hull(member, hull);
        }
        self.events.push(AppEvent::NodeSpriteSet(member));
        vec![Effect::SaveSession, Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use crate::action::{Action, Effect};
    use crate::app::App;

    #[test]
    fn opening_an_address_keeps_the_new_selection_through_pane_rendering() {
        let mut app = App::test_stub();
        let pane = app.default_graph_pane();

        app.update(Action::OpenAddress("https://example.com/".into()));
        let selected = app
            .graph_runtimes
            .focused_member()
            .expect("opening an address selects its node");

        // A graph frame restores pane-local state. The selection must survive
        // that restore or the next content action has no node to target.
        app.graph_pane_frame(pane, 800, 600)
            .expect("the default pane owns the active graph");
        assert_eq!(app.graph_pane_focused_member(pane), Some(selected));
        assert_eq!(app.graph_runtimes.focused_member(), Some(selected));
    }

    #[test]
    fn committed_content_navigation_grows_the_same_members_lineage() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.com/".into()));
        let member = app.graph_runtimes.focused_member().unwrap();
        let before = app.graph_runtimes.graph().node_count();

        let effects = app.update(Action::ContentNavigationCommitted {
            member,
            url: "https://www.iana.org/help/example-domains".into(),
        });

        assert_eq!(app.graph_runtimes.graph().node_count(), before);
        assert_eq!(
            app.graph_runtimes
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url()),
            Some("https://www.iana.org/help/example-domains")
        );
        let key = app.graph_runtimes.graph().get_node_by_id(member).unwrap().0;
        assert_eq!(
            app.graph_runtimes
                .graph()
                .node_history_projection(key)
                .entries,
            vec![
                "https://example.com/".to_string(),
                "https://www.iana.org/help/example-domains".to_string(),
            ]
        );
        assert!(effects.contains(&Effect::SaveSession));
    }
}
