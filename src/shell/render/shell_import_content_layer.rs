// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Both windows import on the primary render core and keep one producer cache.
    #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
    pub(in super::super) fn import_content_layer(
        &mut self,
        node: uuid::Uuid,
        surface: &crate::surface::Surface,
        result: Result<Option<inker::SurfaceFrame>, String>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Option<CompositeLayer>, String> {
        match result {
            Ok(Some(frame)) => {
                let cached = self.surface_frames.entry(node).or_insert(None);
                if let Err(error) = super::super::surface_frames::update_imported_frame(
                    cached,
                    frame,
                    device,
                    queue,
                    #[cfg(feature = "scry")]
                    self.scry_frame_importers.get(&node),
                ) {
                    tracing::warn!(%node, %error, "surface frame import failed");
                    return Err(format!("browser frame import failed: {error}"));
                }
            },
            Ok(None) => {},
            Err(error) => {
                tracing::warn!(%node, %error, "surface resize/capture failed");
                return Err(format!("browser resize/capture failed: {error}"));
            },
        }
        Ok(self
            .surface_frames
            .get(&node)
            .and_then(|cached| cached.as_ref())
            .map(|frame| CompositeLayer {
                id: surface.id.0,
                changed: true,
                dimensions: frame.dimensions(),
                alpha: frame.presentation_alpha(),
                kind: surface.kind,
                view: frame.view(),
                raster: None,
                raster_scale: 1.0,
                // The importer sources carry explicit premultiplied output
                // conventions; legacy direct composition must preserve them
                // just as the shared staging lane does.
                placement: ExternalTexturePlacement::new(surface.rect.dest())
                    .with_alpha(netrender::SourceAlpha::Premultiplied),
            }))
    }

    /// Acquisition/import refusal withdraws content custody in either window.
    #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
    pub(in super::super) fn fail_surface_frames(
        &mut self,
        failures: Vec<(uuid::Uuid, String)>,
    ) -> bool {
        if failures.is_empty() {
            return false;
        }
        for (node, error) in failures {
            self.run_effects(vec![crate::action::Effect::CloseContent { node }]);
            let effects = self
                .app
                .apply_update(crate::action::Update::ContentFailed { node, error });
            self.run_effects(effects);
        }
        self.request_redraw();
        true
    }

    /// Start one target-bound hosted capture. The pending target is recorded
    /// before the engine is called, and the request id remains spent even when
    /// the engine refuses admission.
    pub(in super::super) fn request_page_capture(
        &mut self,
        node: uuid::Uuid,
    ) -> Result<inker::PageCaptureRequestId, String> {
        let session = *self.app.session_id.as_uuid();
        let pending = self
            .page_captures
            .begin(session, node)
            .map_err(|error| format!("capture target refused: {error:?}"))?;
        let result = self
            .content_sessions
            .surface_mut(&node)
            .and_then(|producer| producer.as_web_surface())
            .ok_or_else(|| "capture target has no hosted web surface".to_string())
            .and_then(|surface| {
                surface
                    .request_page_capture(inker::PageCaptureRequest::viewport(pending.request))
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = result {
            let _ = self.page_captures.finish(session, node, pending.request);
            return Err(error);
        }
        Ok(pending.request)
    }

    /// Drain each producer's one host-facing stream before building a frame.
    /// The bounded batch prevents a faulty producer from monopolising the UI
    /// thread; another redraw continues the drain.
    pub(in super::super) fn drain_surface_web_events(&mut self) {
        const MAX_EVENTS_PER_SURFACE: usize = 256;
        let mut pending = Vec::new();
        let shared = self
            .content_sessions
            .pool
            .iter()
            .filter_map(|(key, content)| {
                let super::super::content_pool::ContentKey::Node(node) = *key else {
                    return None;
                };
                Some((node, *key, content.cue_surface()?.capabilities().clone()))
            })
            .collect::<Vec<_>>();
        for (node, key, report) in shared {
            self.app.content.note_cue_report(node, report);
            if let Err(error) = self
                .content_sessions
                .pool
                .advance_cue_browser(&key, self.epoch.elapsed())
            {
                tracing::warn!(%node, %error, "shared browser advance refused");
            }
            for _ in 0..MAX_EVENTS_PER_SURFACE {
                let Some(envelope) = self.content_sessions.pool.poll_cue_event(&key) else {
                    break;
                };
                use inker::webcue::Event;
                let event = match envelope.event {
                    Event::NavigationStarted { address } => {
                        Some(inker::WebSurfaceEvent::Navigation(
                            inker::NavigationEvent::Started { url: address },
                        ))
                    },
                    Event::AddressChanged(url) => {
                        Some(inker::WebSurfaceEvent::AddressChanged { url })
                    },
                    Event::TitleChanged(title) => {
                        Some(inker::WebSurfaceEvent::TitleChanged { title })
                    },
                    Event::NavigationFinished {
                        address,
                        success: true,
                    } => Some(inker::WebSurfaceEvent::Navigation(
                        inker::NavigationEvent::Finished {
                            url: address,
                            title: None,
                        },
                    )),
                    Event::NavigationFinished {
                        address,
                        success: false,
                    } => Some(inker::WebSurfaceEvent::Navigation(
                        inker::NavigationEvent::Failed {
                            url: address,
                            reason: "shared browser reported navigation failure".into(),
                        },
                    )),
                    Event::CursorChanged(cursor) => {
                        if let Some(shape) = super::super::cue_content::cursor_shape(&cursor) {
                            self.content_sessions.cue_cursors.insert(node, shape);
                        }
                        None
                    },
                    Event::Message(message) => {
                        serde_json::from_str::<inker::WebSurfaceEvent>(&message)
                            .ok()
                            .or_else(|| {
                                serde_json::from_str::<inker::WebMessage>(&message)
                                    .ok()
                                    .map(inker::WebSurfaceEvent::WebMessage)
                            })
                    },
                    Event::Completed {
                        result: Ok(inker::webcue::Reply::Zoom(zoom)),
                        ..
                    } => {
                        self.app
                            .content
                            .note_page_zoom(node, super::super::effects::app_page_zoom_facts(zoom));
                        None
                    },
                    Event::Completed {
                        request,
                        result: Err(error),
                    } => {
                        self.app
                            .content
                            .refuse_surface_traversal_request(node, request);
                        tracing::warn!(%node, ?request, %error, "shared browser command refused");
                        None
                    },
                    _ => None,
                };
                if let Some(event) = event {
                    pending.push((node, event));
                }
            }
        }
        for (key, content) in self.content_sessions.pool.surfaces_mut() {
            let super::super::content_pool::ContentKey::Node(node) = *key else {
                continue;
            };
            if content.cue_surface().is_some() {
                continue;
            }
            if let Some(web) = content
                .surface_producer_mut()
                .and_then(|p| p.as_web_surface())
            {
                for _ in 0..MAX_EVENTS_PER_SURFACE {
                    let Some(event) = web.poll_web_event() else {
                        break;
                    };
                    pending.push((node, event));
                }
            }
        }
        for (node, event) in pending {
            self.consume_surface_web_event(node, event);
        }
        self.expire_user_agent_requests();
        self.apply_pending_surface_cursor();
    }

    pub(super) fn consume_surface_web_event(
        &mut self,
        node: uuid::Uuid,
        event: inker::WebSurfaceEvent,
    ) {
        match event {
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Started { .. }) => {
                self.resync_surface_accessibility(node);
                self.page_captures.advance_document(node);
                self.withdraw_user_agent_requests(node, "navigation-started");
                self.app.content.note_surface_started(node);
                self.surface_find_requests.remove(&node);
                let effects = self.app.invalidate_document_find_for(node);
                self.run_effects(effects);
                self.request_redraw();
            },
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Committed { url })
            | inker::WebSurfaceEvent::AddressChanged { url } => {
                self.act(Action::ContentNavigationCommitted { member: node, url });
            },
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Finished {
                title, ..
            }) => {
                self.app.content.note_surface_settled(node);
                if let Some(title) = title {
                    self.act(Action::ContentTitleChanged {
                        member: node,
                        title,
                    });
                } else {
                    self.request_redraw();
                }
            },
            inker::WebSurfaceEvent::TitleChanged { title } => {
                self.act(Action::ContentTitleChanged {
                    member: node,
                    title,
                });
            },
            inker::WebSurfaceEvent::LoadProgress { value } => {
                self.app.content.note_surface_progress(node, value);
                self.request_redraw();
            },
            inker::WebSurfaceEvent::PageCaptureCompleted { id, result } => {
                let session = *self.app.session_id.as_uuid();
                match self.page_captures.finish(session, node, id) {
                    Ok(pending) => match result {
                        Ok(output) => tracing::info!(
                            %node,
                            request_id = id.get(),
                            generation = pending.target.document_generation,
                            bytes = match &output.image { inker::PageCaptureImageArtifact::Png(bytes) => bytes.len() },
                            "page capture correlated; durable deposit is not implemented"
                        ),
                        Err(error) => {
                            tracing::warn!(%node, request_id = id.get(), %error, "page capture failed")
                        },
                    },
                    Err(refusal) => {
                        tracing::warn!(%node, request_id = id.get(), ?refusal, "page capture completion refused")
                    },
                }
            },
            inker::WebSurfaceEvent::DocumentFindChanged(state) => {
                let Some((request, query)) = self.surface_find_requests.get(&node).cloned() else {
                    return;
                };
                if state.query.text != query {
                    return;
                }
                if state.complete {
                    self.surface_find_requests.remove(&node);
                }
                let effects = self
                    .app
                    .apply_update(crate::action::Update::DocumentFindChanged {
                        node,
                        request,
                        query,
                        result: Ok(super::super::effects::app_find_model(state)),
                    });
                self.run_effects(effects);
            },
            inker::WebSurfaceEvent::NewWindowRequested { url } => {
                self.app
                    .note(crate::observe::AppEvent::AuxiliaryNavigableRequested { node, url });
                // This event observes a request rather than lowering a graph
                // mutation. Publish it now so its order against adjacent
                // producer events is retained.
                self.run_effects(Vec::new());
            },
            inker::WebSurfaceEvent::PageDragStarted {
                data_transfer,
                position,
            } => {
                let types = data_transfer
                    .items
                    .iter()
                    .map(|item| match item {
                        inker::DataTransferItem::String { mime_type, .. } => mime_type.clone(),
                        inker::DataTransferItem::File { mime_type, .. } if mime_type.is_empty() => {
                            "file".into()
                        },
                        inker::DataTransferItem::File { mime_type, .. } => {
                            format!("file:{mime_type}")
                        },
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                self.app
                    .note(crate::observe::AppEvent::PageDragRequested { node, types });
                // Winit has no native API that can start an OS drag from a
                // windowless CEF surface. Answer CEF explicitly so its source
                // does not remain in a stuck drag state; capability reporting
                // keeps this path Partial until a toolkit drag carrier lands.
                if let Some(producer) = self.content_sessions.surface_mut(&node)
                    && let Err(error) =
                        producer.finish_drag_source(position, inker::DragOperationSet::NONE)
                {
                    tracing::warn!(%node, %error, "surface page drag cancellation failed");
                }
                self.run_effects(Vec::new());
            },
            inker::WebSurfaceEvent::PermissionRequested(request) => {
                let disposition = self.web_policy.receive_permission(
                    node,
                    request.clone(),
                    std::time::Instant::now(),
                );
                self.project_policy_summary(node);
                match disposition {
                    crate::web_policy::PermissionDisposition::Answer(answer) => {
                        if let Err(error) = self.answer_surface_permission(node, request.id, answer)
                        {
                            tracing::warn!(%node, request_id = request.id.get(), %error, "stored permission decision could not be applied");
                        }
                    },
                    crate::web_policy::PermissionDisposition::Pending => {
                        let effects =
                            self.app
                                .apply_update(crate::action::Update::PermissionRequested {
                                    node,
                                    id: request.id,
                                    origin: request.origin,
                                    descriptors: request.descriptors,
                                });
                        self.run_effects(effects);
                        self.sync_ime_allowed();
                    },
                }
            },
            inker::WebSurfaceEvent::AuthenticationRequested(challenge) => {
                let disposition = self.web_policy.receive_authentication(
                    node,
                    challenge.clone(),
                    std::time::Instant::now(),
                );
                self.project_policy_summary(node);
                match disposition {
                    crate::web_policy::AuthenticationDisposition::Answer(answer) => {
                        if let Err(error) =
                            self.answer_surface_authentication(node, challenge.id, &answer)
                        {
                            tracing::warn!(%node, request_id = challenge.id.get(), %error, "credential-provider answer could not be applied");
                        }
                    },
                    crate::web_policy::AuthenticationDisposition::Pending => {
                        let effects =
                            self.app
                                .apply_update(crate::action::Update::AuthenticationRequested {
                                    node,
                                    id: challenge.id,
                                    host: challenge.protection_space.host,
                                    port: challenge.protection_space.port,
                                    realm: challenge.protection_space.realm,
                                    scheme: challenge.protection_space.scheme,
                                    is_proxy: challenge.protection_space.is_proxy,
                                });
                        self.run_effects(effects);
                        self.sync_ime_allowed();
                    },
                }
            },
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Failed { url, reason }) => {
                self.app.content.note_surface_stopped(node);
                tracing::warn!(%node, %url, %reason, "surface navigation failed");
                self.request_redraw();
            },
            inker::WebSurfaceEvent::ProcessCrashed { reason } => {
                self.withdraw_user_agent_requests(node, "surface-crashed");
                tracing::warn!(%node, %reason, "surface content process crashed");
            },
            inker::WebSurfaceEvent::BackendDiagnostic { severity, message } => {
                tracing::debug!(%node, %severity, %message, "surface backend diagnostic");
            },
            other => {
                // S0 makes every producer event observable through one stream.
                // Later gates give policy/download/representation events typed
                // request ids and host actions; until then they remain loud in
                // diagnostics rather than disappearing in a filtered poller.
                tracing::debug!(%node, event = ?other, "surface event awaits host projection");
            },
        }
    }
}
