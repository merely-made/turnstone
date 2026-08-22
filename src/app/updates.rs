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
                    }
                    Some(crate::content::NodeContent::Requested) => {
                        vec![Effect::SpawnContent { node, url }]
                    }
                    _ => Vec::new(),
                }
            }
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
                    self.content.note_fetched(
                        node,
                        url.clone(),
                        crate::content::FetchedDocument {
                            content_type: fetched.content_type.clone(),
                            body: fetched.body.clone(),
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
                        }
                        None if live_content => effects.push(Effect::UpdateContent { node, url }),
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
            }
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
            }
            Update::GeminiCertificateChanged {
                request,
                node,
                url,
                fetch_url,
                target,
                pinned,
                seen,
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
            Update::PageStopped { request, node, url } => {
                if !self.content.stop_fetch(node, request) {
                    return Vec::new();
                }
                self.events.push(AppEvent::ContentState {
                    node,
                    state: format!("stopped: {url}"),
                });
                vec![Effect::Redraw]
            }
            Update::FaviconFetched {
                node,
                owner_url,
                bytes,
            } => browse::apply_favicon(&mut self.graph_runtimes, node, &owner_url, &bytes),
            Update::DownloadStored {
                node,
                url,
                content_type,
                content_disposition,
                received_at_ms,
                byte_size,
                result,
            } => self.finish_download(
                node,
                url,
                content_type,
                content_disposition,
                received_at_ms,
                byte_size,
                result,
            ),
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

    fn begin_download(
        &mut self,
        node: uuid::Uuid,
        url: String,
        fetched: crate::action::FetchedPage,
    ) -> Vec<Effect> {
        self.content.note_closed(node);
        let received_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let byte_size = fetched.bytes.len() as u64;
        let media_type = fetched
            .content_type
            .as_deref()
            .and_then(|value| value.split(';').next())
            .map(|value| value.trim().to_ascii_lowercase());
        let title =
            crate::download::suggested_filename(&url, fetched.content_disposition.as_deref());
        let Some(graph) = self.graph_runtimes.graph_containing_member(node) else {
            return Vec::new();
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        canvas.set_node_mime_hint_for(node, media_type.clone());
        canvas.set_node_title_for(node, title);
        if let Err(error) = crate::content_classes::set_download_record(
            canvas,
            node,
            crate::content_classes::DownloadFacetRecord {
                source_url: &url,
                received_at_ms,
                byte_size,
                status: "storing",
                media_type: media_type.as_deref(),
                content_disposition: fetched.content_disposition.as_deref(),
                destination_path: None,
                content_hash: None,
                error: None,
            },
        ) {
            let error = format!("could not record download custody: {error}");
            self.events.push(AppEvent::DownloadFailed { node, error });
            return vec![Effect::Redraw];
        }
        self.events.push(AppEvent::DownloadStarted {
            node,
            url: url.clone(),
            bytes: byte_size,
        });
        vec![
            Effect::StoreDownload {
                node,
                url,
                content_type: fetched.content_type,
                content_disposition: fetched.content_disposition,
                received_at_ms,
                bytes: fetched.bytes,
            },
            Effect::Redraw,
        ]
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_download(
        &mut self,
        node: uuid::Uuid,
        url: String,
        content_type: Option<String>,
        content_disposition: Option<String>,
        received_at_ms: u64,
        byte_size: u64,
        result: Result<crate::action::StoredDownload, String>,
    ) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(node) else {
            return Vec::new();
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        if !browse::still_current(canvas, node, &url) {
            return Vec::new();
        }
        let media_type = content_type
            .as_deref()
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        match result {
            Ok(stored) => {
                let hash = stored.content.to_hex();
                let already_attached = canvas
                    .graph()
                    .get_node_by_id(node)
                    .is_some_and(|(_, graph_node)| graph_node.content == Some(stored.content));
                if !already_attached && !canvas.set_node_content_for(node, Some(stored.content)) {
                    let error = "download representation could not attach to its graph node";
                    tracing::warn!(%node, %url, error);
                    let _ = crate::content_classes::set_download_record(
                        canvas,
                        node,
                        crate::content_classes::DownloadFacetRecord {
                            source_url: &url,
                            received_at_ms,
                            byte_size,
                            status: "failed",
                            media_type,
                            content_disposition: content_disposition.as_deref(),
                            destination_path: Some(&stored.destination),
                            content_hash: Some(&hash),
                            error: Some(error),
                        },
                    );
                    self.events.push(AppEvent::DownloadFailed {
                        node,
                        error: error.to_string(),
                    });
                    return vec![Effect::SaveSession, Effect::Redraw];
                }
                if let Err(error) = crate::content_classes::set_download_record(
                    canvas,
                    node,
                    crate::content_classes::DownloadFacetRecord {
                        source_url: &url,
                        received_at_ms,
                        byte_size,
                        status: "completed",
                        media_type,
                        content_disposition: content_disposition.as_deref(),
                        destination_path: Some(&stored.destination),
                        content_hash: Some(&hash),
                        error: None,
                    },
                ) {
                    tracing::warn!(%node, %url, %error, "download metadata could not be completed");
                    self.events.push(AppEvent::DownloadFailed {
                        node,
                        error: error.to_string(),
                    });
                } else {
                    self.events.push(AppEvent::DownloadCompleted {
                        node,
                        destination: stored.destination,
                        content_hash: hash,
                    });
                }
            }
            Err(error) => {
                tracing::warn!(%node, %url, %error, "download storage failed");
                if let Err(facet_error) = crate::content_classes::set_download_record(
                    canvas,
                    node,
                    crate::content_classes::DownloadFacetRecord {
                        source_url: &url,
                        received_at_ms,
                        byte_size,
                        status: "failed",
                        media_type,
                        content_disposition: content_disposition.as_deref(),
                        destination_path: None,
                        content_hash: None,
                        error: Some(&error),
                    },
                ) {
                    tracing::warn!(%node, %facet_error, "download failure metadata could not be recorded");
                }
                self.events.push(AppEvent::DownloadFailed { node, error });
            }
        }
        vec![Effect::SaveSession, Effect::Redraw]
    }
}
