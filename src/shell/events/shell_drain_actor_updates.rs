// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn drain_actor_updates(&mut self, event_loop: &ActiveEventLoop) {
        self.service_private_custody();
        let started = std::time::Instant::now();
        while let Ok(raw) = self
            .fetch_rx
            .try_recv()
            .or_else(|_| self.nomadnet_rx.try_recv())
        {
            let raw = match raw {
                FetchUpdate::Subresource(outcome) => {
                    let requesters = self.pending_fetches.take_subresources(&outcome.url);
                    if requesters.is_empty() {
                        tracing::warn!(url = %outcome.url, "subresource completion without a pending session; dropped");
                        continue;
                    }
                    match outcome.result {
                        Ok(bytes) => {
                            for node in requesters {
                                if let Some(session) = self.content_sessions.session_mut(&node) {
                                    session.provide_subresource(&outcome.url, &bytes);
                                }
                            }
                        },
                        Err(error) => {
                            tracing::warn!(url = %outcome.url, %error, "content subresource failed");
                        },
                    }
                    continue;
                },
                other => other,
            };
            // The port adapter converts the service's types at the boundary;
            // the app only ever sees the app-owned vocabulary.
            if let Some(update) = browse::update_from_fetch(raw, &mut self.pending_fetches) {
                let effects = self.app.apply_update(update);
                self.run_effects(effects);
            }
        }
        while let Ok(update) = self.bin_rx.try_recv() {
            // The bin actor already speaks the app-owned vocabulary.
            let effects = self.app.apply_update(update);
            self.run_effects(effects);
        }
        while let Ok(update) = self.download_rx.try_recv() {
            let effects = self.app.apply_update(update);
            self.run_effects(effects);
        }
        while let Some(response) = self.research_worker.take() {
            self.service_private_custody();
            let effects = self.app.apply_update(crate::action::Update::ResearchSearchFinished { request: response.request, result: response.result });
            self.run_effects(effects);
        }
        while let Ok(update) = self.trail_rx.try_recv() {
            // Recall answers (RecallHits / RecallFailed), already app-owned.
            self.service_private_custody();
            let refresh_workshop_sources = matches!(
                &update,
                crate::action::Update::SavedSourcesLoaded {
                    generation,
                    request,
                    session,
                    ..
                } if *generation == self.app.source_capture_generation
                    && *session == self.app.session_id
                    && self.app.saved_sources.request == Some(*request)
            );
            let private_success = match &update {
                crate::action::Update::SavedSourcesLoaded {
                    result: Ok(entries),
                    ..
                } => entries.iter().any(|entry| entry.content.is_some()),
                crate::action::Update::SavedSourceRead { result: Ok(_), .. }
                | crate::action::Update::SourceDocumentCaptured { result: Ok(_), .. }
                | crate::action::Update::SourceCaptureSaved { result: Ok(()), .. }
                | crate::action::Update::SourceCaptureRecovered { .. }
                | crate::action::Update::SourceCaptureRevalidated { .. } => true,
                _ => false,
            };
            let effects = self.app.apply_update(update);
            self.service_private_custody();
            if refresh_workshop_sources {
                self.refresh_alembic_workshop_sources();
            }
            if !private_success || self.app.research.is_available() {
                self.run_effects(effects);
            }
        }
        while let Ok(update) = self.place_rx.try_recv() {
            // One place answer the app never sees: a prepared dial is a
            // shell-owned thing. The app learns of a refusal as an event and
            // of a visit only as the document that then opens.
            if let crate::action::Update::PlaceDocumentVisit {
                holder_root,
                result,
                ..
            } = update
            {
                self.apply_document_visit(holder_root, result);
                continue;
            }
            let effects = self.app.apply_update(update);
            self.run_effects(effects);
        }
        self.refresh_knot_documents();
        while let Ok(update) = self.micron_submission_rx.try_recv() {
            let effects = self.app.apply_update(update);
            self.run_effects(effects);
        }
        // Any cookie a fetch or a script set since the last drain goes to disk
        // now; the flush is dirty-gated, so a quiet drain writes nothing.
        self.cookie_custody.flush();
        self.drain_surface_web_events();
        self.drain_surface_accessibility();
        // Semantic callbacks publish on their own wake, before any pixel
        // frame acquisition or queued assistive action delivery.
        self.push_a11y_tree();
        self.drain_a11y_actions();
        self.drain_pending_windows(event_loop);
        self.request_redraw();
        tracing::debug!(target: "turnstone::selfdrive", micros = started.elapsed().as_micros(), "native actor batch completed");
    }

    /// Refresh changes need not move the window or change its scale. Sample
    /// on the existing host cadence, with unchanged rates suppressed by the
    /// runtime pool. No extra event-loop wake is needed while the host is idle.
    pub(super) fn observe_physics_display_rate(&mut self, force: bool) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let now = std::time::Instant::now();
        if !force
            && self
                .physics_display_rate_last_observed
                .is_some_and(|last| now.duration_since(last) < std::time::Duration::from_secs(1))
        {
            return;
        }
        self.physics_display_rate_last_observed = Some(now);
        self.app.graph_runtimes.set_physics_display_rate(
            window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz()),
        );
    }

    /// The Redshank command loop, one turn per event batch: take what the
    /// mounted docks issued, apply it through the app's one listening
    /// authority, then show every dock what the model and runtime now say.
    pub(in super::super) fn pump_research(&mut self) {
        self.app.sync_research_context();
        for intent in self.app.research.drain() {
            let effects = match intent {
                crate::research::Intent::Selection {
                    context,
                    expected,
                    chosen,
                } => self.app.update(crate::action::Action::SetAnswerSelection {
                    generation: context.generation,
                    expected,
                    chosen,
                }),
                crate::research::Intent::Citation { context, target } => {
                    if context.session != self.app.session_id
                        || context.generation != self.app.source_capture_generation
                    {
                        continue;
                    }
                    match target {
                        crate::research::Target::Capture(target) => self
                            .app
                            .update(crate::action::Action::OpenSavedSource(target)),
                        crate::research::Target::Clip(target) => {
                            self.app.open_research_clip(target)
                        },
                    }
                },
            };
            self.run_effects(effects);
        }
    }

    pub(super) fn pump_redshank(&mut self) {
        let commands = self.redshank_docks.drain();
        if !commands.is_empty() {
            let effects = self.app.apply_redshank_commands(commands);
            self.run_effects(effects);
        }
        for item in self.redshank_docks.items() {
            let projection = self.app.redshank_projection(&item);
            self.redshank_docks.project(&item, projection);
        }
    }
}
