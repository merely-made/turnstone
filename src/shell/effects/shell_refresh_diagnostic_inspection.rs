// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(crate) fn reactive_frame_scope(&self, slot: usize) -> crate::reactive_render::FrameScope {
        let window = if slot == 0 {
            self.window.as_ref()
        } else {
            self.lens_windows
                .values()
                .find(|window| window.ordinal + 1 == slot)
                .map(|window| &window.window)
        };
        let scope = window.map_or_else(
            || {
                crate::reactive_render::mark_input_gap();
                crate::reactive_render::FrameScope::begin(0)
            },
            crate::reactive_render::FrameScope::native,
        );
        let space = if slot == 0 {
            crate::action::SpaceRef::Primary
        } else {
            crate::action::SpaceRef::Lens(slot - 1)
        };
        let visible = self
            .app
            .space(space)
            .and_then(|layout| layout.outer_workspace().ok())
            .map(|frame| frame.active_ids())
            .unwrap_or_default();
        for id in visible {
            let pane = crate::panes::PaneId(id.0);
            if slot == 0
                && self
                    .app
                    .maximized
                    .is_some_and(|maximized| maximized != pane)
            {
                continue;
            }
            if let Some(pane) = self.renderers.settings.get(&pane) {
                pane.include_reactive_mutations();
            }
        }
        scope
    }

    pub(crate) fn attach_reactive_trace(
        &mut self,
        receiver: crate::reactive_trace::ReactiveTraceReceiver,
        boot: &apparatus_core::SourceBootId,
    ) -> Result<(), crate::diagnostic_observations::ReactiveAttachError> {
        let result = self.diagnostic_observations.attach_reactive(receiver, boot);
        if result.is_err() {
            self.diagnostic_observations.mark_reactive_attach_failure();
        }
        result
    }

    /// Refresh only the value-facing projection; diagnostics authority remains
    /// here, and Gloss reads its own independent immutable batch.
    pub(in super::super) fn refresh_diagnostic_inspection(&mut self) {
        let _ = self.diagnostic_observations.drain_reactive(4096);
        let requested = self.app.frisket.iter_leaves().any(|(_, content, _)| {
            content
                .composition()
                .is_some_and(|config| config.sections.iter().any(|id| id == "diagnostics"))
        });
        self.app.diagnostic_inspection =
            requested.then(|| self.diagnostic_observations.inspection());
    }

    /// Hand the app's drained semantic events to their consumers. Navigation
    /// events become trail-memory records for the root persona (owner = the
    /// master public key's hex, the stable key-rooted tag). A bounded described
    /// copy feeds automation without competing for this app-owned stream.
    pub(super) fn drain_app_events(&mut self) {
        let events = self.app.take_events();
        if events.is_empty() {
            return;
        }
        let owner: Option<String> = self
            .app
            .personae_root()
            .map(|root| root.iter().map(|b| format!("{b:02x}")).collect());
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        for event in events {
            if std::env::var_os("TURNSTONE_SCENARIO").is_some()
                && let Some((kind, address)) = super::super::citation_event_history::event_address(&event)
            {
                let target = crate::saved_sources::parse_address(address).ok().flatten()
                    .filter(|target| self.app.saved_source_admitted(&target.key))
                    .filter(|target| crate::research_scenario_privacy::capture_selected(&self.app.research_search.selected, &target.key))
                    .filter(|_| std::env::var("TURNSTONE_M5_EXPECTED_A1_URI").ok().as_deref() == Some(address));
                match kind {
                    super::super::citation_event_history::OpenEvent::Address => self.address_event_history.record(target),
                    super::super::citation_event_history::OpenEvent::Reader => self.reader_event_history.record(target),
                }
            }
            self.diagnostic_observations.record(&event);
            if self.observed_events.len() == 128 {
                self.observed_events.pop_front();
            }
            self.observed_events.push_back(event.describe());
            if let Some((url, transition)) = crate::trail_memory::navigation(&event) {
                match &owner {
                    Some(owner) => {
                        self.trail_handle
                            .command(crate::trail_memory::TrailCommand::Record {
                                owner: owner.clone(),
                                url,
                                transition,
                                at_ms,
                            });
                    },
                    // The trail is the persona's, keyed by its root, which a
                    // locked vault does not reveal (vault lock ruling 38).
                    // Hold the visit until the root is adopted.
                    None if self.pending_trail.len() < super::super::PENDING_TRAIL_CAP => {
                        self.pending_trail.push((url, transition, at_ms));
                    },
                    None => {},
                }
            }
        }
    }

    /// The effect runner: the one place effects meet ports.
    /// Retire what the shell keeps beside a node's surface: its capture
    /// registration and its imported frames. The caller removes or replaces
    /// the node's content entry.
    pub(super) fn retire_surface_extras(&mut self, node: uuid::Uuid) {
        self.page_captures.remove_surface(node);
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        self.surface_frames.remove(&node);
        #[cfg(all(feature = "scry", windows))]
        self.scry_frame_importers.remove(&node);
    }

    pub(super) fn spawn_browser_surface(
        &mut self,
        node: uuid::Uuid,
        url: &str,
        decision: &inker::routing::EngineRouteDecision,
        source: pelt_core::PeltRouteSource,
    ) -> Update {
        let result = (|| -> Result<crate::content::ContentFacts, String> {
            let scry = decision.engine_id == inker::routing::ENGINE_SCRYING_WEB;
            let servo_profile = if decision.engine_id == inker::routing::ENGINE_GRAFT_SERVO {
                Some(self.ensure_servo_engine()?)
            } else {
                None
            };
            if scry {
                self.ensure_scry_engine()?;
            } else if servo_profile.is_none() {
                self.ensure_weld_engine()?;
            }
            let profile = if let Some(profile) = &servo_profile {
                profile.clone()
            } else if scry {
                self.app
                    .data_root
                    .join("scry")
                    .join("webview2-profiles")
                    .join(node.to_string())
            } else {
                self.app
                    .data_root
                    .join("weld")
                    .join("cef-cache")
                    .join(node.to_string())
            };
            std::fs::create_dir_all(&profile).map_err(|error| {
                format!(
                    "could not create browser profile {}: {error}",
                    profile.display()
                )
            })?;
            let profile = profile.canonicalize().map_err(|error| {
                format!(
                    "could not resolve browser profile {}: {error}",
                    profile.display()
                )
            })?;

            let profile = browser_profile_path(profile)?;

            // Producer epochs belong to one instance. Retire old custody and
            // synchronization before a replacement can emit an equal epoch.
            self.content_sessions.remove(&node);
            self.clear_reader_appearances(node);
            self.retire_surface_semantics(node);
            self.surface_find_requests.remove(&node);
            self.page_captures.remove_surface(node);
            #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
            self.surface_frames.remove(&node);
            #[cfg(all(feature = "scry", windows))]
            self.scry_frame_importers.remove(&node);

            let spawn = SurfaceSpawnRequest {
                url: url.to_owned(),
                width: self.width.max(1),
                height: self.height.max(1),
                profile: EngineProfileBinding {
                    user_data_dir: profile.to_string_lossy().into_owned(),
                },
                // Scry's host wrapper installs its dedicated producer fence
                // in the adapter envelope. Weld's callback importer is separate.
                fence_handle: None,
            };
            let mut producer = self
                .surface_engines
                .spawn(decision, &spawn)
                .map_err(|error| error.to_string())?;
            #[cfg(all(feature = "scry", windows))]
            if scry {
                let fence = self
                    .scry_factory
                    .as_ref()
                    .ok_or_else(|| "Scry factory was not retained by the host".to_owned())?
                    .synchronizer_for_profile(&profile)?;
                let host = self
                    .host
                    .as_ref()
                    .ok_or_else(|| "the Turnstone wgpu host is not ready".to_owned())?;
                self.scry_frame_importers.insert(
                    node,
                    super::super::surface_frames::ScryingFrameImporter::new(
                        host.device(),
                        host.queue(),
                        fence,
                    ),
                );
            }
            let capabilities = producer
                .as_web_surface()
                .map(|web| app_document_capabilities(web.document_capabilities()))
                .unwrap_or_default();
            replay_page_scale(&mut self.app, node, producer.as_mut());
            // The node's content is the surface lane of its Pelt content, so
            // navigation reaches the web plane through the same command API
            // as a document (SC step 4, pass B).
            let route = pelt_core::PeltRoute {
                decision: decision.clone(),
                source,
                state: pelt_core::PeltRouteState::Surface,
            };
            #[cfg(all(feature = "servo", any(target_os = "macos", target_os = "linux")))]
            let content = if servo_profile.is_some() {
                let host = self
                    .host
                    .as_ref()
                    .ok_or("the Turnstone wgpu host is not ready")?;
                let browser = inker::cue_browser::BrowserSurface::new_with_report(
                    producer,
                    inker::webcue::Scope {
                        owner: uuid::Uuid::new_v4().as_u128(),
                        generation: 1,
                    },
                    inker::webcue::Viewport {
                        width: spawn.width,
                        height: spawn.height,
                        scale: 1.,
                    },
                    super::super::servo::cue::capabilities().map_err(|e| e.to_string())?,
                    super::super::servo::cue::frame_reader(
                        host.device().clone(),
                        host.queue().clone(),
                    ),
                )
                .map_err(|e| e.to_string())?;
                pelt_core::PeltContent::from_browser(browser, route)
            } else {
                pelt_core::PeltContent::from_surface(producer, (spawn.width, spawn.height), route)
            };
            #[cfg(not(all(feature = "servo", any(target_os = "macos", target_os = "linux"))))]
            let content =
                pelt_core::PeltContent::from_surface(producer, (spawn.width, spawn.height), route);
            self.content_sessions.insert(node, content);
            self.activate_surface_accessibility(node, false);
            self.page_captures.register_surface(node);
            tracing::info!(%node, %url, engine = %decision.engine_id, "surface content live");
            Ok(crate::content::ContentFacts {
                engine: decision.engine_id.clone(),
                lane: crate::content::ContentLane::Surface,
                structure: None,
                lineage: None,
                capabilities,
            })
        })();
        match result {
            Ok(facts) => Update::ContentSpawned {
                node,
                facts: Some(facts),
            },
            Err(error) => Update::ContentFailed {
                node,
                error: format!("{error} ({})", decision.engine_id),
            },
        }
    }

    /// The effect runner: the one place effects meet ports.
    pub(in super::super) fn run_effects(&mut self, effects: Vec<Effect>) {
        self.app.sync_research_context();
        self.publish_engine_inventory();
        // Semantic events noted by the update that produced these effects
        // drain to their consumers first (the trail-memory capture today;
        // the scenario log and diagnostics subscribe at this same drain).
        self.drain_app_events();
        for effect in effects {
            if self.app.session_load_refused()
                && !matches!(
                    &effect,
                    Effect::SwitchSession { .. }
                        | Effect::Redraw
                        | Effect::ClosePlace { .. }
                        | Effect::SettingsPaneRecordResult { .. }
                )
            {
                continue;
            }
            let fetch_commands = browse::fetch_commands_for(&effect, &mut self.pending_fetches);
            if !fetch_commands.is_empty() {
                for command in fetch_commands {
                    self.command_fetch(command);
                }
                continue;
            }
            match effect {
                Effect::OpenThemeWorkshop
                | Effect::OpenAlembicWorkshop
                | Effect::ReplaceGeminiTrust { .. }
                | Effect::SaveSession
                | Effect::PersistSettingsPaneRecord { .. }
                | Effect::SettingsPaneRecordResult { .. }
                | Effect::SaveChromeLayout(..)
                | Effect::SaveGraphFolds { .. }
                | Effect::CopyText(..)
                | Effect::CreateResearchNote { .. }
                | Effect::ChooseResearchNote { .. }
                | Effect::ChooseKnotDocumentFile { .. }
                | Effect::RetainProducerSelection { .. }
                | Effect::ChooseClipPackageFile
                | Effect::ImportClipPackage(..)
                | Effect::ControlRetainedClip(..)
                | Effect::ChooseClipExportFile { .. }
                | Effect::ExportClipPackage { .. }
                | Effect::OpenPlace { .. }
                | Effect::JoinPlace { .. }
                | Effect::FoundPlace { .. }
                | Effect::OfferPlacePrekey { .. }
                | Effect::InvitePlace { .. }
                | Effect::WritePlaceArtifact { .. }
                | Effect::ReadPlaceArtifact { .. }
                | Effect::LeavePlace { .. }
                | Effect::ReconnectPlace { .. }
                | Effect::ClearPlaceLeftMark { .. }
                | Effect::RunPlaceCommand { .. }
                | Effect::ResyncPlace { .. }
                | Effect::SetPlaceCollection { .. }
                | Effect::ClosePlace { .. }
                | Effect::StoreImage { .. }
                | Effect::StoreDownload { .. }
                | Effect::RecordDeleted { .. }
                | Effect::EmptyRecycleBin
                | Effect::RecallQuery { .. }
                | Effect::RecordPageText { .. } => self.effect_open_theme_workshop(effect),
                Effect::TrashSession { .. } | Effect::SwitchSession { .. } => {
                    self.effect_trash_session(effect)
                },
                Effect::SpawnContent { .. } | Effect::UpdateContent { .. } => {
                    self.effect_spawn_content(effect)
                },
                Effect::ControlContent { .. }
                | Effect::ScaleContent { .. }
                | Effect::CaptureContent { .. }
                | Effect::PersistSourceCapture { .. }
                | Effect::CaptureSourceDocument { .. }
                | Effect::ResearchCompare(..)
                | Effect::ResearchRecords(..)
                | Effect::ResearchSearch(..)
                | Effect::CancelResearchSearch
                | Effect::LoadSavedSources { .. }
                | Effect::ReadSavedSource { .. }
                | Effect::RestoreReadingAnchor { .. }
                | Effect::SaveReadingPreferences
                | Effect::ReadFind { .. }
                | Effect::FindContent { .. }
                | Effect::ClearContentFind { .. }
                | Effect::AnswerPermissionRequest { .. }
                | Effect::AnswerAuthenticationRequest { .. }
                | Effect::CloseContent { .. }
                | Effect::Redraw
                | Effect::SubmitMicron { .. }
                | Effect::CancelMicronSubmission { .. }
                | Effect::OpenWindow { .. }
                | Effect::FetchPage { .. }
                | Effect::CancelPage { .. }
                | Effect::FetchFeed { .. }
                | Effect::FetchFavicon { .. }
                | Effect::SubmitSmolweb { .. } => self.effect_control_content(effect),
            }
        }
    }
}
