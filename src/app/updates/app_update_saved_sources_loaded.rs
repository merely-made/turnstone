// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn update_saved_sources_loaded(&mut self, update: Update) -> Vec<Effect> {
        match update {
            Update::SavedSourcesLoaded {
                generation,
                request,
                session,
                admissions,
                result,
            } => {
                if generation != self.source_capture_generation {
                    return Vec::new();
                }
                self.saved_sources_loaded(request, session, admissions, result)
            },
            Update::SavedSourceRead {
                generation,
                request,
                node,
                url,
                target,
                result,
            } => {
                if generation != self.source_capture_generation {
                    return Vec::new();
                }
                self.saved_source_read(request, node, url, target, result)
            },
            Update::FeedFetched { node, url, result } => self.apply_feed_fetched(node, url, result),
            Update::PageStreamed {
                request,
                node,
                url,
                response_url,
                content_type,
                bytes,
            } => {
                if !self.content.is_active_fetch(node, request) {
                    return Vec::new();
                }
                let current = self
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| browse::still_current(canvas, node, &url));
                let state = self.content.get(node).cloned();
                let renderable = content_type
                    .as_deref()
                    .unwrap_or("text/gemini")
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().starts_with("text/"));
                if !current || !renderable {
                    return Vec::new();
                }
                let received = self.content.note_streamed(
                    node,
                    url.clone(),
                    response_url,
                    content_type,
                    &bytes,
                );
                self.events.push(AppEvent::ContentState {
                    node,
                    state: format!("streaming bytes={received}"),
                });
                match state {
                    Some(crate::content::NodeContent::Live) => {
                        vec![Effect::UpdateContent { node, url }]
                    },
                    Some(crate::content::NodeContent::Requested) => {
                        vec![Effect::SpawnContent { node, url }]
                    },
                    _ => Vec::new(),
                }
            },
            Update::PageFetched {
                request,
                node,
                url,
                result,
            } => {
                if !self.content.finish_fetch(node, request) {
                    return Vec::new();
                }
                let current = self
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| browse::still_current(canvas, node, &url));
                if !current {
                    return browse::apply_page(&mut self.graph_runtimes, node, url, result);
                }
                if result
                    .as_ref()
                    .is_ok_and(crate::download::is_download_response)
                {
                    return self.begin_download(node, url, result.expect("checked success"));
                }
                let requested_content = matches!(
                    self.content.get(node),
                    Some(crate::content::NodeContent::Requested)
                );
                let live_content = matches!(
                    self.content.get(node),
                    Some(crate::content::NodeContent::Live)
                );
                if let Ok(fetched) = &result {
                    // A retained response from a previous request is not
                    // acquisition context for this terminal answer. Only this
                    // request's progress can supply its actual final URL.
                    let observed = matches!(
                        self.content.fetch_phase(node),
                        Some(crate::content::PageFetchPhase::Streaming { .. })
                    )
                    .then(|| self.content.fetched(node, &url))
                    .flatten();
                    let effective_url =
                        observed.and_then(|previous| previous.effective_url.clone());
                    let content_type = fetched
                        .content_type
                        .clone()
                        .or_else(|| observed.and_then(|previous| previous.content_type.clone()));
                    let acquired_at_ms = observed
                        .map(|previous| previous.acquired_at_ms)
                        .filter(|observed_at| *observed_at != 0)
                        .unwrap_or_else(|| {
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|duration| duration.as_millis() as u64)
                                .unwrap_or(0)
                        });
                    self.content.note_fetched(
                        node,
                        url.clone(),
                        crate::content::FetchedDocument {
                            bytes: fetched.bytes.clone(),
                            content_type,
                            body: fetched.body.clone(),
                            effective_url,
                            acquired_at_ms,
                        },
                        fetched.bytes.len(),
                    );
                    if requested_content || live_content {
                        self.events.push(AppEvent::ContentState {
                            node,
                            state: format!("settled bytes={}", fetched.bytes.len()),
                        });
                    }
                }
                let failed = result.as_ref().err().cloned();
                let mut effects =
                    browse::apply_page(&mut self.graph_runtimes, node, url.clone(), result);
                if requested_content || live_content {
                    match failed {
                        Some(error) => {
                            self.content.note_failed(node, error.clone());
                            self.events.push(AppEvent::ContentState {
                                node,
                                state: format!("failed: {error}"),
                            });
                            if live_content {
                                effects.push(Effect::CloseContent { node });
                            }
                        },
                        None if live_content => effects.push(Effect::UpdateContent { node, url }),
                        None => effects.push(Effect::SpawnContent { node, url }),
                    }
                }
                effects
            },
            Update::SmolwebSubmitted {
                request,
                source: _,
                target,
                result,
            } => {
                let micron_source = self.micron_submission_sources.remove(&request);
                let source_is_current = micron_source.is_none_or(|(source, url, body)| {
                    self.graph_runtimes
                        .graph()
                        .get_node_by_id(source)
                        .is_some_and(|(_, node)| node.url() == url)
                        && self
                            .content
                            .fetched(source, &url)
                            .is_some_and(|document| document.body == body)
                });
                let active = self.active_smolweb_submission == Some(request);
                if active {
                    self.active_smolweb_submission = None;
                }
                let current = active && source_is_current;
                match result {
                    Ok(crate::action::SmolwebSubmissionReceipt::Redirect(destination)) => {
                        self.events.push(AppEvent::SmolwebSubmissionSucceeded {
                            target,
                            outcome: format!("redirect {destination}"),
                        });
                        if !current {
                            return Vec::new();
                        }
                        self.omnibar = crate::ui::OmnibarState::default();
                        self.shell.close_omnibar();
                        self.focus = crate::surface::FocusTarget::Graph(self.default_graph_pane());
                        self.update(crate::action::Action::OpenAddress(destination))
                    },
                    Ok(crate::action::SmolwebSubmissionReceipt::Success(page)) => {
                        self.events.push(AppEvent::SmolwebSubmissionSucceeded {
                            target: target.clone(),
                            outcome: "success".to_string(),
                        });
                        if !current {
                            return Vec::new();
                        }
                        let preview = page.body.split_whitespace().collect::<Vec<_>>().join(" ");
                        let preview = if preview.is_empty() {
                            page.content_type.unwrap_or_else(|| "success".to_string())
                        } else {
                            preview.chars().take(240).collect()
                        };
                        self.omnibar = crate::ui::OmnibarState {
                            open: true,
                            mode: crate::ui::OmnibarMode::SmolwebSubmissionResult(
                                crate::ui::SmolwebSubmissionResult {
                                    target,
                                    message: preview,
                                },
                            ),
                            ..Default::default()
                        };
                        self.focus = crate::surface::FocusTarget::Chrome;
                        self.recompute_omnibar_suggestions();
                        vec![Effect::Redraw]
                    },
                    Err(error) => {
                        self.events.push(AppEvent::SmolwebSubmissionFailed {
                            target: target.clone(),
                            error: error.clone(),
                        });
                        if !current {
                            return Vec::new();
                        }
                        self.omnibar = crate::ui::OmnibarState {
                            open: true,
                            mode: crate::ui::OmnibarMode::SmolwebSubmissionResult(
                                crate::ui::SmolwebSubmissionResult {
                                    target,
                                    message: format!("failed: {error}"),
                                },
                            ),
                            ..Default::default()
                        };
                        self.focus = crate::surface::FocusTarget::Chrome;
                        self.recompute_omnibar_suggestions();
                        vec![Effect::Redraw]
                    },
                }
            },
            Update::SmolwebInputRequested {
                request,
                node,
                url,
                input_url,
                prompt,
                sensitive,
            } => {
                if !self.content.settle_fetch(node, request) {
                    return Vec::new();
                }
                let current = self
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| browse::still_current(canvas, node, &url));
                if !current {
                    return Vec::new();
                }
                let resume_content = matches!(
                    self.content.get(node),
                    Some(
                        crate::content::NodeContent::Requested | crate::content::NodeContent::Live
                    )
                );
                if resume_content {
                    self.content.note_awaiting_input(node);
                    self.events.push(AppEvent::ContentState {
                        node,
                        state: "awaiting-input".to_string(),
                    });
                }
                let target = self.fallback_shell_context();
                self.shell.begin_omnibar(target);
                self.omnibar = crate::ui::OmnibarState {
                    open: true,
                    mode: crate::ui::OmnibarMode::SmolwebInput(crate::ui::SmolwebInputPrompt {
                        node,
                        requested_url: url,
                        input_url,
                        prompt: prompt.clone(),
                        sensitive,
                    }),
                    ..Default::default()
                };
                self.focus = crate::surface::FocusTarget::Chrome;
                self.recompute_omnibar_suggestions();
                self.events.push(AppEvent::SmolwebInputRequested {
                    node,
                    prompt,
                    sensitive,
                });
                let mut effects = Vec::new();
                if resume_content {
                    effects.push(Effect::CloseContent { node });
                }
                effects.push(Effect::Redraw);
                effects
            },
            Update::GeminiIdentityRequested {
                request,
                node,
                url,
                identity_url,
                prompt,
            } => {
                if !self.content.settle_fetch(node, request) {
                    return Vec::new();
                }
                let current = self
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| browse::still_current(canvas, node, &url));
                if !current {
                    return Vec::new();
                }
                let resume_content = matches!(
                    self.content.get(node),
                    Some(
                        crate::content::NodeContent::Requested | crate::content::NodeContent::Live
                    )
                );
                if resume_content {
                    self.content.note_awaiting_identity(node);
                    self.events.push(AppEvent::ContentState {
                        node,
                        state: "awaiting-identity".to_string(),
                    });
                }
                let origin = crate::gemini_identity::capsule_origin(&identity_url)
                    .unwrap_or_else(|_| identity_url.clone());
                let target = self.fallback_shell_context();
                self.shell.begin_omnibar(target);
                self.omnibar = crate::ui::OmnibarState {
                    open: true,
                    mode: crate::ui::OmnibarMode::GeminiIdentity(crate::ui::GeminiIdentityPrompt {
                        node,
                        requested_url: url,
                        identity_url,
                        prompt: prompt.clone(),
                    }),
                    ..Default::default()
                };
                self.focus = crate::surface::FocusTarget::Chrome;
                self.recompute_omnibar_suggestions();
                self.events.push(AppEvent::GeminiIdentityRequested {
                    node,
                    origin,
                    prompt,
                });
                let mut effects = Vec::new();
                if resume_content {
                    effects.push(Effect::CloseContent { node });
                }
                effects.push(Effect::Redraw);
                effects
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }
}
