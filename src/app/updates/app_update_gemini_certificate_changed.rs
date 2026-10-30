// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn update_gemini_certificate_changed(&mut self, update: Update) -> Vec<Effect> {
        match update {
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
            },
            Update::PageStopped { request, node, url } => {
                if !self.content.stop_fetch(node, request) {
                    return Vec::new();
                }
                self.events.push(AppEvent::ContentState {
                    node,
                    state: format!("stopped: {url}"),
                });
                vec![Effect::Redraw]
            },
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
                self.reading.find.retain(|target, _| target.member != node);
                // Retained engines report their document title at admission,
                // before later title-change events. Give tabs and captions
                // the same identity as the opened document immediately.
                let title = facts
                    .as_ref()
                    .and_then(|facts| facts.structure.as_ref())
                    .and_then(|structure| structure.title.as_ref())
                    .filter(|title| !title.trim().is_empty())
                    .cloned();
                let mut effects = title
                    .map(|title| self.set_content_title(node, title))
                    .unwrap_or_default();
                self.content.note_live(node, facts);
                self.events.push(AppEvent::ContentState {
                    node,
                    state: "live".to_string(),
                });
                effects.push(Effect::Redraw);
                effects
            },
            Update::ReadingFindChanged {
                target,
                query,
                result,
            } => self.read_find_changed(target, query, result),
            Update::DocumentFindChanged {
                node,
                request,
                query,
                result,
            } => self.apply_document_find_changed(node, request, query, result),
            Update::PermissionRequested {
                node,
                id,
                origin,
                descriptors,
            } => self.apply_permission_requested(node, id, origin, descriptors),
            Update::AuthenticationRequested {
                node,
                id,
                host,
                port,
                realm,
                scheme,
                is_proxy,
            } => self.apply_authentication_requested(node, id, host, port, realm, scheme, is_proxy),
            Update::PermissionRequestFinished {
                request,
                answer,
                retention,
                terminal,
                result,
            } => {
                self.apply_permission_request_finished(request, answer, retention, terminal, result)
            },
            Update::AuthenticationRequestFinished {
                request,
                supplied_credentials,
                remember_for_process,
                terminal,
                result,
            } => self.apply_authentication_request_finished(
                request,
                supplied_credentials,
                remember_for_process,
                terminal,
                result,
            ),
            Update::UserAgentRequestWithdrawn {
                request,
                kind,
                reason,
            } => self.apply_user_agent_request_withdrawn(request, kind, reason),
            Update::ContentSourceCaptured {
                node,
                url,
                document,
            } => {
                let current = self
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| browse::still_current(canvas, node, &url));
                if current {
                    let received_bytes = document.bytes.len();
                    self.content
                        .note_fetched(node, url, document, received_bytes);
                }
                Vec::new()
            },
            Update::ContentFailed { node, error } => {
                tracing::warn!(%node, %error, "content spawn failed");
                self.events.push(AppEvent::ContentState {
                    node,
                    state: format!("failed: {error}"),
                });
                self.content.note_failed(node, error);
                vec![Effect::Redraw]
            },
            Update::BinListed { records } => {
                // The bin mirror replaces wholesale — the actor's answer IS
                // the store's truth (never merged with a hand-kept copy).
                self.removed = records;
                vec![Effect::Redraw]
            },
            Update::BinFailed { error } => {
                // Loud and attributable: the Removed section going quiet
                // because the store broke must be visible divergence, not an
                // empty list pretending nothing was deleted.
                tracing::warn!(%error, "recycle bin failed");
                self.events.push(AppEvent::BinFailed(error));
                vec![Effect::Redraw]
            },
            Update::RecallHits { query, hits } => {
                // Superseded answers drop: the omnibar has moved on, and the
                // lane must never show hits for text that is no longer there.
                if query != self.recall_query {
                    return Vec::new();
                }
                self.recall = hits;
                self.recompute_omnibar_suggestions();
                vec![Effect::Redraw]
            },
            Update::RecallFailed { error } => {
                tracing::warn!(%error, "browsing recall failed");
                self.events.push(AppEvent::RecallFailed(error));
                vec![Effect::Redraw]
            },
            Update::SourceDocumentCaptured {
                request, result, ..
            } => self.source_capture_deposited(request, result),
            Update::SourceCaptureRevalidated {
                request, stored, ..
            } => self.revalidate_source_capture(request, stored),
            Update::SourceCaptureRecoveryFailed { error, .. } => {
                self.events.push(AppEvent::InteractionMissed {
                    what: "recover-source-captures",
                    target: error,
                });
                vec![Effect::Redraw]
            },
            Update::SourceCaptureRecovered {
                request, stored, ..
            } => self.recover_source_capture(request, stored),
            Update::SourceCaptureSaved {
                request, result, ..
            } => self.source_capture_saved(request, result),
            Update::PlaceDocumentVisit { .. } => Vec::new(),
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
            },
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
                    },
                    // A refusal changes no state. The place is still open and
                    // still whatever it was; the command simply did not happen.
                    Err(error) => {
                        tracing::warn!(%error, "place command refused");
                        self.events.push(AppEvent::PlaceRefused(error));
                    },
                }
                self.reflow_omnibar();
                vec![Effect::Redraw]
            },
            Update::PlaceCollectionSet {
                session,
                generation,
                result,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                match result {
                    Ok(snapshot) => {
                        if let Some(binding) = self.place.binding().cloned() {
                            self.reconcile_shared_graph(&snapshot.shared);
                            self.place = crate::place::PlaceState::Offline {
                                binding,
                                generation,
                                snapshot,
                            };
                        }
                    },
                    Err(error) => {
                        tracing::warn!(%error, "place collection selection refused");
                        self.events.push(AppEvent::PlaceRefused(error));
                    },
                }
                self.reflow_omnibar();
                vec![Effect::Redraw]
            },
            Update::PlaceFounded {
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
                    },
                    // Founding that failed founded nothing: there is no place
                    // to be degraded about, exactly as with a refused invite.
                    Err(error) => {
                        tracing::warn!(%error, "founding refused");
                        self.events
                            .push(AppEvent::PlaceRefused(format!("founding place: {error}")));
                        crate::place::PlaceState::Failed { error }
                    },
                };
                self.reflow_omnibar();
                vec![Effect::Redraw]
            },
            Update::PlacePrekeyOffered {
                session,
                generation,
                result,
            } => {
                let Some(pending) = self.pending_place_artifact.take_if(|pending| {
                    session == self.session_id && pending.generation == generation
                }) else {
                    return Vec::new();
                };
                match result {
                    Ok(prekey) => {
                        use base64::Engine as _;
                        let Some(root) = self.personae_root() else {
                            return self.refuse_place(crate::denizen::PENDING_IDENTITY);
                        };
                        let offer = crate::place::PlacePrekeyOfferV1 {
                            version: crate::place::PLACE_CARD_VERSION,
                            moot: pending.moot,
                            root: crate::place::hex32(&root),
                            prekey: base64::engine::general_purpose::STANDARD.encode(&prekey),
                        };
                        match serde_json::to_vec_pretty(&offer) {
                            Ok(bytes) => vec![Effect::WritePlaceArtifact {
                                path: pending.path,
                                bytes,
                            }],
                            Err(error) => {
                                self.refuse_place(format!("encode pre-key offer: {error}"))
                            },
                        }
                    },
                    Err(error) => self.refuse_place(format!("offering a pre-key: {error}")),
                }
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }
}
