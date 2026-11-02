// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn commit_gemini_identity(
        &mut self,
        input: crate::ui::GeminiIdentityPrompt,
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

        let bound = match &self.identity {
            Some(root) => self
                .gemini_identities
                .bind(root.as_ref(), &input.identity_url),
            None => Err(crate::denizen::PENDING_IDENTITY.to_string()),
        };
        let origin = match bound {
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
            },
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

    pub(super) fn commit_gemini_trust(
        &mut self,
        input: crate::ui::GeminiTrustPrompt,
    ) -> Vec<Effect> {
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

    pub(in super::super) fn open_address(&mut self, url: String) -> Vec<Effect> {
        let saved = match crate::saved_sources::parse_address(&url) {
            Ok(Some(target)) if !self.saved_source_admitted(&target.key) => {
                return self.saved_source_refusal(
                    "This saved source is not attached in the current session".into(),
                );
            },
            Ok(target) => target,
            Err(error) => return self.saved_source_refusal(error),
        };
        if let Some(effects) = self.retained_clip_control(&url) {
            return effects;
        }
        let mut effects = self.invalidate_micron_submission();
        self.events.push(AppEvent::AddressOpened(url.clone()));
        // Podcast audio is a listening tile, not a reader page. One decision
        // point: a subscribed feed entry answers with its show, GUID and
        // artwork; a bare audio address answers with itself.
        if let Some(episode) = self.redshank_episode_for_address(&url) {
            effects.extend(self.open_redshank_episode(episode));
            return effects;
        }
        // A graph pane owns its selection. Visiting through the compatibility
        // canvas cursor selected the node only until the next render installed
        // that pane's saved selection, at which point an address opened from
        // the omnibar became unfocused and could not spawn or retarget content.
        let pane = self
            .focused_graph_pane()
            .unwrap_or_else(|| self.default_graph_pane());
        if saved.is_some()
            && !self
                .graph_for_pane(pane)
                .and_then(|graph| self.graph_runtimes.get(graph))
                .is_some_and(|runtime| runtime.session == Some(self.session_id))
        {
            return self.saved_source_refusal("The reader view belongs to another session".into());
        }
        if saved.is_some() {
            // A saved citation can originate inside an authored document.
            // Reveal its reader and retire the previous content focus just as
            // the saved-source palette and Answers routes do.
            self.reveal_pane(pane);
            self.focus = FocusTarget::Graph(pane);
        }
        let Some(key) = self.with_graph_pane(pane, |canvas| canvas.visit(&url)) else {
            effects.push(Effect::Redraw);
            return effects;
        };
        self.history.visit(url.clone());
        effects.push(Effect::Redraw);
        if saved.is_some()
            && let Some(node) = self
                .graph_for_pane(pane)
                .and_then(|graph| self.graph_runtimes.canvas(graph))
                .and_then(|canvas| canvas.graph().get_node(key))
                .map(|node| node.id)
        {
            // The internal address is durable reader intent, never a new origin
            // fetch or source capture. Every explicit open revalidates locally.
            self.saved_readers.remove(&node);
            self.browser.entry(node).content_on = true;
            self.content.note_requested(node);
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::SpawnContent { node, url });
            effects.push(Effect::SaveSession);
            return effects;
        }
        if matches!(
            crate::retained_clip::request(&url),
            Ok(Some(crate::retained_clip::ClipRequest::Read(_)))
        ) && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            self.content.note_requested(node);
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::SpawnContent { node, url });
            return effects;
        }
        // A place-held document names the mere that holds it. Outside a
        // place there is nobody to ask, so this is refused at the door with
        // a reason rather than left as a node that spins. The node stays:
        // the person asked for it, and it opens the moment they are in the
        // place that can reach it.
        if let Some((holder_root, _)) = crate::knot_authoring::parse_place_held_address(&url) {
            let mine = matches!(
                &self.place,
                crate::place::PlaceState::Offline { snapshot, .. }
                    if snapshot.personae_root == holder_root
            );
            if !mine && self.place.generation().is_none() {
                self.events.push(AppEvent::PlaceRefused(format!(
                    "{url} is held by another mere; no place is open to reach it"
                )));
                return effects;
            }
        }
        if url::Url::parse(&url).is_ok_and(|parsed| parsed.scheme() == "titan")
            && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            effects.extend(self.begin_smolweb_submission(Some(node), url));
        } else if crate::browse::is_fetchable(&url)
            && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            effects.push(self.fetch_page_effect(node, url.clone(), url));
        }
        effects
    }

    pub(super) fn invalidate_micron_submission(&mut self) -> Vec<Effect> {
        let Some(request) = self.active_smolweb_submission else {
            return Vec::new();
        };
        if self.micron_submission_sources.remove(&request).is_none() {
            return Vec::new();
        }
        self.active_smolweb_submission = None;
        if let crate::ui::OmnibarMode::SmolwebSubmissionResult(result) = &mut self.omnibar.mode {
            result.message = "Cancelled locally; the remote outcome may be unknown".into();
            self.recompute_omnibar_suggestions();
        }
        vec![Effect::CancelMicronSubmission { request }]
    }

    pub(in super::super) fn commit_content_navigation(
        &mut self,
        member: Uuid,
        url: String,
    ) -> Vec<Effect> {
        if let Some(effects) = self.retained_clip_control(&url) {
            return effects;
        }
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            tracing::warn!(%member, %url, "content navigation named an unknown member");
            return Vec::new();
        };
        if self
            .graph_runtimes
            .canvas(graph)
            .and_then(|canvas| canvas.graph().get_node_by_id(member))
            .is_some_and(|(_, node)| node.url() == url)
        {
            // A duplicate observation of the old address can precede the
            // queued traversal. It must not consume that traversal's intent.
            return Vec::new();
        }
        let traversal = self.content.take_surface_traversal(member);
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        // A browser traversal commits only after its actual address matches
        // the requested neighbour. Ordinary links back to an old URL remain
        // new visits, and a refusal never moves the host history cursor.
        let traversed = traversal.is_some_and(|step| {
            if step.target != url
                || !canvas
                    .graph()
                    .get_node_by_id(member)
                    .is_some_and(|(_, node)| node.url() == step.from)
            {
                return false;
            }
            let actual = if step.back {
                canvas.member_history_back(member)
            } else {
                canvas.member_history_forward(member)
            };
            actual.as_deref() == Some(url.as_str())
        });
        if !traversed && !canvas.navigate_member(member, &url) {
            return Vec::new();
        }
        // A retained read belongs to this view's exact address. Dropping it
        // also cancels the pending-read dedup fence when navigating away.
        self.saved_readers.remove(&member);
        self.events
            .push(AppEvent::ContentNavigated { node: member, url });
        let mut effects = self.invalidate_document_find_for(member);
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    pub(in super::super) fn set_content_title(
        &mut self,
        member: Uuid,
        title: String,
    ) -> Vec<Effect> {
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

    pub(crate) fn nav_back(&mut self) -> Vec<Effect> {
        self.navigate_focused_lineage(true)
    }

    pub(crate) fn nav_forward(&mut self) -> Vec<Effect> {
        self.navigate_focused_lineage(false)
    }

    pub(super) fn navigate_focused_lineage(&mut self, back: bool) -> Vec<Effect> {
        let Some((node, _)) = self.browser_command_target() else {
            return vec![Effect::Redraw];
        };
        if self.node_uses_web_surface(node) {
            if self.content.surface_traversal_pending(node) {
                return vec![Effect::Redraw];
            }
            let step = self
                .graph_runtimes
                .graph_containing_member(node)
                .and_then(|graph| self.graph_runtimes.canvas(graph))
                .and_then(|canvas| {
                    let (key, _) = canvas.graph().get_node_by_id(node)?;
                    let history = canvas.graph().node_history_projection(key);
                    let index = if back {
                        history.current_index.checked_sub(1)?
                    } else {
                        history.current_index.checked_add(1)?
                    };
                    Some((
                        history.entries.get(history.current_index)?.clone(),
                        history.entries.get(index)?.clone(),
                    ))
                });
            let Some((from, target)) = step else {
                return vec![Effect::Redraw];
            };
            self.content
                .begin_surface_traversal(node, from, target, back);
            self.content.note_surface_started(node);
            let mut effects = self.invalidate_document_find_for(node);
            effects.push(Effect::ControlContent {
                node,
                control: if back {
                    crate::action::ContentControl::Back
                } else {
                    crate::action::ContentControl::Forward
                },
            });
            effects.push(Effect::Redraw);
            return effects;
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
        self.saved_readers.remove(&node);
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
        let mut effects = self.invalidate_document_find_for(node);
        effects.push(Effect::SaveSession);
        if crate::browse::is_fetchable(&url) {
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

    /// Whether `node`'s live content is a web surface, by the lane its
    /// content reported rather than by engine id, so every surface engine
    /// (Weld, Scry, Servo) takes the web plane's own navigation.
    pub(crate) fn node_uses_web_surface(&self, node: Uuid) -> bool {
        self.content
            .facts(node)
            .is_some_and(|facts| facts.lane == crate::content::ContentLane::Surface)
    }

    pub(in super::super) fn reseed_layout(&mut self) -> Vec<Effect> {
        if self.graph_runtimes.reseed() {
            self.events.push(AppEvent::LayoutReseeded);
            vec![Effect::Redraw]
        } else {
            Vec::new()
        }
    }
}
