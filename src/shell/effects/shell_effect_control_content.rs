// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn effect_control_content(&mut self, effect: Effect) {
        match effect {
            Effect::ResearchRecords(request) => self.research_worker.submit_records(request),
            Effect::ResearchSearch(request) => self.research_worker.submit(request),
            Effect::ResearchCompare(request) => self.research_worker.submit_compare(request),
            Effect::CancelResearchSearch => self.research_worker.cancel(),
            Effect::ControlContent { node, control } => {
                // The surface lane's own command API reaches the engine's
                // web plane. A document's Back, Reload and Stop belong to
                // the App's load and graph history, so only a surface
                // takes them here.
                let result = self
                    .content_sessions
                    .get_mut(&node)
                    .filter(|content| content.lane() == pelt_core::PeltLane::Surface)
                    .ok_or_else(|| "content has no web control plane".to_string())
                    .and_then(|content| {
                        if content.cue_browser_mut().is_some() {
                            let command = match control {
                                crate::action::ContentControl::Back => inker::webcue::Command::Back,
                                crate::action::ContentControl::Forward => {
                                    inker::webcue::Command::Forward
                                },
                                crate::action::ContentControl::Reload => {
                                    inker::webcue::Command::Reload
                                },
                                crate::action::ContentControl::Stop => inker::webcue::Command::Stop,
                            };
                            return content
                                .request_cue_browser(command)
                                .map(Some)
                                .map_err(|e| e.to_string());
                        }
                        let effect = content.command(match control {
                            crate::action::ContentControl::Back => {
                                inker::SessionNavigationCommand::Back
                            },
                            crate::action::ContentControl::Forward => {
                                inker::SessionNavigationCommand::Forward
                            },
                            crate::action::ContentControl::Reload => {
                                inker::SessionNavigationCommand::Reload
                            },
                            crate::action::ContentControl::Stop => {
                                inker::SessionNavigationCommand::Stop
                            },
                        });
                        effect.error.map_or(Ok(None), Err)
                    });
                if let Ok(Some(request)) = &result {
                    if matches!(
                        control,
                        crate::action::ContentControl::Back
                            | crate::action::ContentControl::Forward
                    ) {
                        self.app
                            .content
                            .bind_surface_traversal_request(node, *request);
                    }
                }
                if let Err(error) = result {
                    tracing::warn!(%node, ?control, %error, "web content control failed");
                    self.app.content.note_surface_stopped(node);
                }
                self.request_redraw();
            },
            Effect::ScaleContent { node, scale } => {
                if let Some(content) = self.content_sessions.get_mut(&node)
                    && content.cue_browser_mut().is_some()
                {
                    if let Err(error) =
                        content.request_cue_browser(inker::webcue::Command::Zoom(scale))
                    {
                        tracing::warn!(%node, %error, "shared page zoom refused");
                    }
                    self.request_redraw();
                    return;
                }
                if let Some(producer) = self.content_sessions.surface_mut(&node) {
                    apply_page_scale(&mut self.app, node, scale, producer);
                } else if let Some(session) = self.content_sessions.session_mut(&node) {
                    // The retained lane: the session reflows at the new CSS
                    // viewport on the next frame, and answers with the
                    // level it actually settled on.
                    apply_retained_page_scale(&mut self.app, node, scale, session);
                } else {
                    // Nothing live to scale: content is off. The request is
                    // persisted and the spawn path replays it.
                    tracing::debug!(%node, scale, "page zoom has no live surface");
                }
                self.request_redraw();
            },
            Effect::CaptureContent { node } => {
                if let Err(error) = self.request_page_capture(node) {
                    tracing::warn!(%node, %error, "page capture request refused");
                }
            },
            Effect::PersistSourceCapture { request } => {
                let result = self.app.persist_source_capture(request);
                if let Err(error) = result {
                    let effects = self.app.apply_update(Update::SourceCaptureSaved {
                        capture_generation: self.app.source_capture_generation,
                        request,
                        result: Err(error),
                    });
                    self.run_effects(effects);
                } else {
                    let owner = self.app.source_captures[&request].request.clone();
                    let directory = crate::trail_memory::memory_dir(&crate::session::session_dir(
                        &self.app.data_root,
                        owner.session,
                    ));
                    if !self.trail_handle.command(
                        crate::trail_memory::TrailCommand::AcknowledgeSourceCapture {
                            capture_generation: self.app.source_capture_generation,
                            request: owner,
                            directory,
                        },
                    ) {
                        let effects = self.app.apply_update(Update::SourceCaptureSaved {
                            capture_generation: self.app.source_capture_generation,
                            request,
                            result: Err(
                                "the capture actor could not acknowledge the saved request".into(),
                            ),
                        });
                        self.run_effects(effects);
                    }
                }
            },
            Effect::CaptureSourceDocument {
                capture_generation,
                request,
            } => {
                if capture_generation != self.app.source_capture_generation {
                    return;
                }
                let owner = self
                    .app
                    .source_capture_dispatch_owner(&request, capture_generation);
                let document = self
                    .app
                    .content
                    .source_capture_candidate(request.node, &request.url)
                    .cloned();
                // The actor can replay an exact saved receipt without a live origin.
                let result = owner.and_then(|()| {
                    let directory = crate::trail_memory::memory_dir(&crate::session::session_dir(
                        &self.app.data_root,
                        request.session,
                    ));
                    if self.trail_handle.command(
                        crate::trail_memory::TrailCommand::CaptureSourceDocument {
                            capture_generation,
                            request: request.clone(),
                            directory,
                            document,
                        },
                    ) {
                        Ok(())
                    } else {
                        Err("the session Eidetic actor is unavailable".into())
                    }
                });
                if let Err(error) = result {
                    let effects = self.app.apply_update(Update::SourceDocumentCaptured {
                        capture_generation,
                        request,
                        result: Err(error),
                    });
                    self.run_effects(effects);
                }
            },
            Effect::LoadSavedSources {
                generation,
                request,
                session,
                admissions,
            } => {
                if generation != self.app.source_capture_generation {
                    return;
                }
                let directory = crate::trail_memory::memory_dir(&crate::session::session_dir(
                    &self.app.data_root,
                    session,
                ));
                if !self
                    .trail_handle
                    .command(crate::trail_memory::TrailCommand::LoadSavedSources {
                        generation,
                        request,
                        session,
                        admissions: admissions.clone(),
                        directory,
                    })
                {
                    let effects = self.app.apply_update(Update::SavedSourcesLoaded {
                        generation,
                        request,
                        session,
                        admissions,
                        result: Err("the session Eidetic actor is unavailable".into()),
                    });
                    self.run_effects(effects);
                }
            },
            Effect::ReadSavedSource {
                generation,
                request,
                node,
                url,
                target,
                admission,
            } => {
                if generation != self.app.source_capture_generation {
                    return;
                }
                let directory = crate::trail_memory::memory_dir(&crate::session::session_dir(
                    &self.app.data_root,
                    target.key.session,
                ));
                if !self
                    .trail_handle
                    .command(crate::trail_memory::TrailCommand::ReadSavedSource {
                        generation,
                        request,
                        node,
                        url: url.clone(),
                        target: target.clone(),
                        admission,
                        directory,
                    })
                {
                    let effects = self.app.apply_update(Update::SavedSourceRead {
                        generation,
                        request,
                        node,
                        url,
                        target,
                        result: Err("the session Eidetic actor is unavailable".into()),
                    });
                    self.run_effects(effects);
                }
            },
            Effect::RestoreReadingAnchor { target, anchor } => {
                let keys:Vec<_>=self.content_sessions.pool.keys().copied().filter(|key| matches!(key,super::super::content_pool::ContentKey::Reader{node,appearance} if *node==target.member && (0..=self.app.lenses.len() as u64).any(|window|target.appearance(window)==*appearance))).collect();
                for key in keys {
                    if let Some(controller) = self
                        .content_sessions
                        .pool
                        .content_mut(&key)
                        .and_then(pelt_core::PeltContent::document_mut)
                    {
                        let session = controller.session_mut().as_any();
                        if let Some(reader) =
                            session.downcast_mut::<mere_document_lanes::ReaderDocumentSession>()
                        {
                            reader.restore_reading_anchor(anchor.clone());
                        } else if let Some(doc) =
                            session.downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
                        {
                            doc.document_mut().restore_reading_anchor(anchor.clone());
                        }
                    }
                }
            },
            Effect::SaveReadingPreferences => {
                if let Err(error) = self.app.reading.save(&self.app.session_dir()) {
                    tracing::warn!(%error, "reading preferences were not saved");
                }
            },
            Effect::ReadFind {
                target,
                query,
                step,
                close,
            } => {
                let keys: Vec<_> = self.content_sessions.pool.keys().copied().filter(|key| {
                        matches!(key, super::super::content_pool::ContentKey::Reader { node, appearance }
                            if *node == target.member && (0..=self.app.lenses.len() as u64).any(|window| target.appearance(window) == *appearance)
                                && self.content_sessions.pool.placement(appearance).is_some_and(|placement|placement.visible && placement.content == *key))
                    }).collect();
                let result = keys
                    .first()
                    .and_then(|key| self.content_sessions.pool.content_mut(key))
                    .and_then(pelt_core::PeltContent::document_mut)
                    .map(|controller| {
                        let session = controller.session_mut();
                        if close {
                            session.clear_document_find().map(|_| {
                                inker::DocumentFindState::empty(inker::DocumentFindQuery::new(
                                    &query,
                                ))
                            })
                        } else if let Some(direction) = step {
                            session.document_find_step(direction)
                        } else {
                            session.document_find(&inker::DocumentFindQuery::new(&query))
                        }
                    })
                    .unwrap_or_else(|| {
                        Err(inker::SessionError::Unsupported(
                            "This reading projection has no retained session".into(),
                        ))
                    })
                    .map_err(|error| error.to_string());
                let effects = self
                    .app
                    .apply_update(crate::action::Update::ReadingFindChanged {
                        target,
                        query,
                        result,
                    });
                self.run_effects(effects);
            },
            Effect::FindContent {
                node,
                request,
                query,
                direction,
                find_next,
            } => {
                let engine_direction = match direction {
                    crate::action::DocumentFindDirection::Previous => {
                        inker::DocumentFindDirection::Previous
                    },
                    crate::action::DocumentFindDirection::Next => {
                        inker::DocumentFindDirection::Next
                    },
                };
                let engine_query = DocumentFindQuery::new(query.clone());
                let immediate = if let Some(session) = self.content_sessions.session_mut(&node) {
                    Some(
                        if find_next {
                            session.document_find_step(engine_direction)
                        } else {
                            session.document_find(&engine_query)
                        }
                        .map(app_find_model)
                        .map_err(|error| error.to_string()),
                    )
                } else if let Some(web) = self
                    .content_sessions
                    .surface_mut(&node)
                    .and_then(|producer| producer.as_web_surface())
                {
                    match web.document_find(&engine_query, engine_direction, find_next) {
                        Ok(()) => {
                            self.surface_find_requests
                                .insert(node, (request, query.clone()));
                            None
                        },
                        Err(error) => Some(Err(error.to_string())),
                    }
                } else {
                    Some(Err("content has no document find owner".into()))
                };
                if let Some(result) = immediate {
                    let effects = self.app.apply_update(Update::DocumentFindChanged {
                        node,
                        request,
                        query,
                        result,
                    });
                    self.run_effects(effects);
                }
            },
            Effect::ClearContentFind { node } => {
                self.surface_find_requests.remove(&node);
                if let Some(session) = self.content_sessions.session_mut(&node) {
                    if let Err(error) = session.clear_document_find() {
                        tracing::warn!(%node, %error, "retained document find clear failed");
                    }
                } else if let Some(web) = self
                    .content_sessions
                    .surface_mut(&node)
                    .and_then(|producer| producer.as_web_surface())
                    && let Err(error) = web.clear_document_find()
                {
                    tracing::warn!(%node, %error, "hosted document find clear failed");
                }
                self.request_redraw();
            },
            Effect::AnswerPermissionRequest {
                request,
                answer,
                retention,
            } => {
                let result =
                    self.answer_permission_request(request.node, request.id, answer, retention);
                let terminal = self
                    .web_policy
                    .permission_request(request.node, request.id)
                    .is_none();
                if result.is_err() {
                    tracing::warn!(
                        node = %request.node,
                        request_id = request.id.get(),
                        "permission decision could not be fully applied"
                    );
                }
                let effects = self.app.apply_update(Update::PermissionRequestFinished {
                    request,
                    answer,
                    retention,
                    terminal,
                    result,
                });
                self.run_effects(effects);
                self.sync_ime_allowed();
            },
            Effect::AnswerAuthenticationRequest {
                request,
                credentials,
                remember_for_process,
            } => {
                let supplied_credentials = credentials.is_some();
                let answer = credentials.map_or(
                    inker::HttpAuthenticationAnswer::Cancel,
                    |(username, password)| {
                        inker::HttpAuthenticationAnswer::Credentials(inker::HttpCredentials {
                            username: username.as_str().to_string(),
                            password: password.as_str().to_string(),
                        })
                    },
                );
                let result = self.answer_authentication_request(
                    request.node,
                    request.id,
                    answer,
                    remember_for_process,
                );
                let terminal = self
                    .web_policy
                    .authentication_challenge(request.node, request.id)
                    .is_none();
                if result.is_err() {
                    tracing::warn!(
                        node = %request.node,
                        request_id = request.id.get(),
                        "authentication decision could not be applied"
                    );
                }
                let effects = self
                    .app
                    .apply_update(Update::AuthenticationRequestFinished {
                        request,
                        supplied_credentials,
                        remember_for_process,
                        terminal,
                        result,
                    });
                self.run_effects(effects);
                self.sync_ime_allowed();
            },
            Effect::CloseContent { node } => {
                self.retire_surface_semantics(node);
                self.withdraw_user_agent_requests(node, "surface-closed");
                self.clear_reader_appearances(node);
                self.surface_find_requests.remove(&node);
                match self
                    .content_sessions
                    .remove(&node)
                    .map(|content| content.lane())
                {
                    Some(pelt_core::PeltLane::Document) => {
                        tracing::info!(%node, "content session closed");
                    },
                    Some(pelt_core::PeltLane::Surface) => {
                        self.retire_surface_extras(node);
                        tracing::info!(%node, "surface content closed");
                    },
                    None => {},
                }
            },
            Effect::Redraw => self.request_redraw(),
            Effect::SubmitMicron {
                request,
                source,
                submission,
            } => {
                self.micron_submission_handle.command(
                    super::super::nomadnet_fetch::MicronSubmissionCommand::Submit {
                        request,
                        source,
                        submission,
                    },
                );
            },
            Effect::CancelMicronSubmission { request } => {
                self.micron_submission_handle.command(
                    super::super::nomadnet_fetch::MicronSubmissionCommand::Cancel { request },
                );
            },
            Effect::OpenWindow { ordinal } => self.pending_windows.push(ordinal),
            Effect::FetchPage { .. }
            | Effect::CancelPage { .. }
            | Effect::FetchFeed { .. }
            | Effect::FetchFavicon { .. }
            | Effect::SubmitSmolweb { .. } => {},
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }

    /// Release retained place handles before a session directory is switched,
    /// moved, or left at shutdown.
    /// Ask the place worker to dial the holder of a place-held address, if
    /// this is one and no visit to that holder is open yet.
    ///
    /// Answers whether the spawn was deferred. A place-held address naming
    /// this profile's own root is not deferred: it is this mere's document
    /// and the local vault answers it.
    pub(super) fn request_visit_if_held_elsewhere(&mut self, node: uuid::Uuid, url: &str) -> bool {
        let Some((holder_root, path)) = crate::knot_authoring::parse_place_held_address(url) else {
            return false;
        };
        if let Ok(visits) = self.knot_visits.lock()
            && (visits.is_local(&holder_root) || visits.holder(&holder_root).is_some())
        {
            return false;
        }
        let session = self.app.session_id;
        let Some(generation) = self.app.place.generation() else {
            // No place, no holder to reach. Refused at the door rather than
            // left spinning; the node stays where the person put it.
            self.refuse_visit(
                node,
                url,
                "no place is open, so no mere can be asked for it",
            );
            return true;
        };
        let request = self.next_visit_request;
        self.next_visit_request += 1;
        self.pending_visit_spawns
            .push((holder_root, node, url.to_string()));
        self.place_handle
            .command(crate::place::worker::PlaceWorkerCommand::VisitDocument {
                session,
                generation,
                directory: session::session_dir(&self.app.data_root, session),
                holder_root,
                path,
                request,
            });
        true
    }
}
