//! The port answers: every `Update` a port hands back, folded into app truth.
//!
//! The other half of the spine from `update`. Ports never touch state; they
//! answer with one of these and the fold happens here.

use crate::action::{Effect, Update};
use crate::browse;
use crate::observe::AppEvent;

use super::App;

impl App {
    /// Fold one typed service answer into state.
    pub fn apply_update(&mut self, update: Update) -> Vec<Effect> {
        match update {
            Update::FeedFetched { node, url, result } => self.apply_feed_fetched(node, url, result),
            Update::PageFetched { node, url, result } => {
                let current = self
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| browse::still_current(canvas, node, &url));
                if !current {
                    return browse::apply_page(&mut self.graph_runtimes, node, url, result);
                }
                let requested_content = matches!(
                    self.content.get(node),
                    Some(crate::content::NodeContent::Requested)
                );
                if let Ok(fetched) = &result {
                    self.content.note_fetched(
                        node,
                        url.clone(),
                        crate::content::FetchedDocument {
                            content_type: fetched.content_type.clone(),
                            body: fetched.body.clone(),
                        },
                    );
                }
                let failed = result.as_ref().err().cloned();
                let mut effects =
                    browse::apply_page(&mut self.graph_runtimes, node, url.clone(), result);
                if requested_content {
                    match failed {
                        Some(error) => {
                            self.content.note_failed(node, error.clone());
                            self.events.push(AppEvent::ContentState {
                                node,
                                state: format!("failed: {error}"),
                            });
                        }
                        None => effects.push(Effect::SpawnContent { node, url }),
                    }
                }
                effects
            }
            Update::SmolwebSubmitted {
                request,
                source: _,
                target,
                result,
            } => {
                let current = self.active_smolweb_submission == Some(request);
                if current {
                    self.active_smolweb_submission = None;
                }
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
                    }
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
                    }
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
                    }
                }
            }
            Update::SmolwebInputRequested {
                node,
                url,
                input_url,
                prompt,
                sensitive,
            } => {
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
            }
            Update::GeminiIdentityRequested {
                node,
                url,
                identity_url,
                prompt,
            } => {
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
            }
            Update::GeminiCertificateChanged {
                node,
                url,
                fetch_url,
                target,
                pinned,
                seen,
            } => {
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
                    self.content.note_awaiting_trust(node);
                    self.events.push(AppEvent::ContentState {
                        node,
                        state: "awaiting-trust".to_string(),
                    });
                }
                let target_context = self.fallback_shell_context();
                self.shell.begin_omnibar(target_context);
                self.omnibar = crate::ui::OmnibarState {
                    open: true,
                    mode: crate::ui::OmnibarMode::GeminiTrust(crate::ui::GeminiTrustPrompt {
                        node,
                        requested_url: url,
                        fetch_url,
                        target: target.clone(),
                        pinned: pinned.clone(),
                        seen: seen.clone(),
                    }),
                    ..Default::default()
                };
                self.focus = crate::surface::FocusTarget::Chrome;
                self.recompute_omnibar_suggestions();
                self.events.push(AppEvent::GeminiCertificateChanged {
                    node,
                    target,
                    pinned,
                    seen,
                });
                let mut effects = Vec::new();
                if resume_content {
                    effects.push(Effect::CloseContent { node });
                }
                effects.push(Effect::Redraw);
                effects
            }
            Update::FaviconFetched {
                node,
                owner_url,
                bytes,
            } => browse::apply_favicon(&mut self.graph_runtimes, node, &owner_url, &bytes),
            Update::ContentSpawned { node, facts } => {
                self.content.note_live(node, facts);
                self.events.push(AppEvent::ContentState {
                    node,
                    state: "live".to_string(),
                });
                vec![Effect::Redraw]
            }
            Update::ContentFailed { node, error } => {
                tracing::warn!(%node, %error, "content spawn failed");
                self.events.push(AppEvent::ContentState {
                    node,
                    state: format!("failed: {error}"),
                });
                self.content.note_failed(node, error);
                vec![Effect::Redraw]
            }
            Update::BinListed { records } => {
                // The bin mirror replaces wholesale — the actor's answer IS
                // the store's truth (never merged with a hand-kept copy).
                self.removed = records;
                vec![Effect::Redraw]
            }
            Update::BinFailed { error } => {
                // Loud and attributable: the Removed section going quiet
                // because the store broke must be visible divergence, not an
                // empty list pretending nothing was deleted.
                tracing::warn!(%error, "recycle bin failed");
                self.events.push(AppEvent::BinFailed(error));
                vec![Effect::Redraw]
            }
            Update::RecallHits { query, hits } => {
                // Superseded answers drop: the omnibar has moved on, and the
                // lane must never show hits for text that is no longer there.
                if query != self.recall_query {
                    return Vec::new();
                }
                self.recall = hits;
                self.recompute_omnibar_suggestions();
                vec![Effect::Redraw]
            }
            Update::RecallFailed { error } => {
                tracing::warn!(%error, "browsing recall failed");
                self.events.push(AppEvent::RecallFailed(error));
                vec![Effect::Redraw]
            }
            Update::PlaceLanesAdvanced {
                session,
                generation,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                vec![Effect::ResyncPlace {
                    session,
                    generation,
                }]
            }
            Update::PlaceCommandDone {
                session,
                generation,
                request: _,
                result,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                match result {
                    // Authoring changed what the place projects, so the answer
                    // carries the re-folded snapshot rather than leaving the
                    // app with a stale view of state it just changed.
                    Ok(snapshot) => {
                        if let Some(binding) = self.place.binding().cloned() {
                            self.reconcile_shared_graph(&snapshot.shared);
                            self.place = crate::place::PlaceState::Offline {
                                binding,
                                generation,
                                snapshot,
                            };
                        }
                    }
                    // A refusal changes no state. The place is still open and
                    // still whatever it was; the command simply did not happen.
                    Err(error) => {
                        tracing::warn!(%error, "place command refused");
                        self.events.push(AppEvent::PlaceRefused(error));
                    }
                }
                vec![Effect::Redraw]
            }
            Update::PlaceJoined {
                session,
                generation,
                result,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                self.place = match result {
                    Ok((binding, snapshot)) => {
                        self.reconcile_shared_graph(&snapshot.shared);
                        crate::place::PlaceState::Offline {
                            binding,
                            generation,
                            snapshot,
                        }
                    }
                    // A refused invitation is not a degraded place, it is no
                    // place. Landing in `Degraded` would leave app state
                    // holding a binding admission never granted.
                    Err(error) => {
                        tracing::warn!(%error, "invitation refused");
                        crate::place::PlaceState::Failed { error }
                    }
                };
                vec![Effect::Redraw]
            }
            Update::PlaceOpened {
                session,
                generation,
                result,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                let Some(binding) = self.place.binding().cloned() else {
                    return Vec::new();
                };
                self.place = match result {
                    Ok(snapshot) => {
                        self.reconcile_shared_graph(&snapshot.shared);
                        crate::place::PlaceState::Offline {
                            binding,
                            generation,
                            snapshot,
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "place cache failed to open");
                        crate::place::PlaceState::Degraded {
                            binding,
                            generation,
                            error,
                        }
                    }
                };
                vec![Effect::Redraw]
            }
        }
    }
}
