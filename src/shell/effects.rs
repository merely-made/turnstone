//! The effect runner and session save: the one place effects meet ports.
//!
//! `App::update` returns effects; every port call the app asked for happens
//! here, and every answer comes back as a typed `Update`. Nothing else in the
//! shell talks to a port.

use std::sync::mpsc::Receiver;

use fetch::{FetchCommand, FetchUpdate};
use inker::{EngineProfileBinding, SessionClick, SessionSpawnRequest, SurfaceSpawnRequest};

use crate::action::{Action, Effect, Update};
use crate::browse;
use crate::panes::PaneContent;
use crate::session;

use super::Shell;

/// How many recalled pages the omnibar asks for. The lane sits below the
/// node and go rows, so a handful is what there is room to read.
const RECALL_ROW_LIMIT: usize = 5;

fn project_reader_lineage_facet(
    app: &mut crate::app::App,
    node: uuid::Uuid,
    lineage: Option<&crate::content::ExtractionLineageFacts>,
) {
    let Some(lineage) = lineage else {
        return;
    };
    let value = serde_json::json!({
        "tool": lineage.tool,
        "version": lineage.version,
        "selector": lineage.selector,
        "score": lineage.score,
        "block_count": lineage.block_count,
    });
    if let Err(error) = app.graph_runtimes.facets_mut().set(
        node,
        chartulary::FacetId::new(crate::content::READER_LINEAGE_FACET),
        value,
        &chartulary::AcceptAll,
    ) {
        tracing::warn!(%node, %error, "could not project reader extraction lineage facet");
    }
}

#[cfg(test)]
mod reader_lineage_tests {
    use super::*;
    use crate::action::Action;

    #[test]
    fn reader_lineage_projects_and_reextract_updates_the_node_facet() {
        let mut app = crate::app::App::test_stub();
        app.update(Action::OpenAddress(
            "https://example.test/story".to_string(),
        ));
        let node = app.graph_runtimes.focused_member().unwrap();
        let mut lineage = crate::content::ExtractionLineageFacts {
            tool: "fleece".to_string(),
            version: "0.1.0".to_string(),
            selector: "main".to_string(),
            score: None,
            block_count: 3,
        };
        project_reader_lineage_facet(&mut app, node, Some(&lineage));
        let facet = chartulary::FacetId::new(crate::content::READER_LINEAGE_FACET);
        assert_eq!(
            app.graph_runtimes.facets().get(&node, &facet).unwrap()["version"],
            "0.1.0"
        );
        lineage.version = "0.2.0".to_string();
        lineage.block_count = 4;
        project_reader_lineage_facet(&mut app, node, Some(&lineage));
        let updated = app.graph_runtimes.facets().get(&node, &facet).unwrap();
        assert_eq!(updated["version"], "0.2.0");
        assert_eq!(updated["block_count"], 4);
    }
}

impl Shell {
    /// Hand the app's drained semantic events to their consumers. Navigation
    /// events become trail-memory records for the root persona (owner = the
    /// master public key's hex, the stable key-rooted tag). A bounded described
    /// copy feeds automation without competing for this app-owned stream.
    fn drain_app_events(&mut self) {
        let events = self.app.take_events();
        if events.is_empty() {
            return;
        }
        let owner: String =
            identity::IdentityProvider::master_public_key(self.app.identity.as_ref())
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        for event in events {
            if self.observed_events.len() == 128 {
                self.observed_events.pop_front();
            }
            self.observed_events.push_back(event.describe());
            if let Some((url, transition)) = crate::trail_memory::navigation(&event) {
                self.trail_handle
                    .command(crate::trail_memory::TrailCommand::Record {
                        owner: owner.clone(),
                        url,
                        transition,
                        at_ms,
                    });
            }
        }
    }

    /// The effect runner: the one place effects meet ports.
    pub(super) fn run_effects(&mut self, effects: Vec<Effect>) {
        // Semantic events noted by the update that produced these effects
        // drain to their consumers first (the trail-memory capture today;
        // the scenario log and diagnostics subscribe at this same drain).
        self.drain_app_events();
        for effect in effects {
            let fetch_commands = browse::fetch_commands_for(&effect, &mut self.pending_fetches);
            if !fetch_commands.is_empty() {
                for command in fetch_commands {
                    self.fetch_handle.command(command);
                }
                continue;
            }
            match effect {
                Effect::ReplaceGeminiTrust {
                    node,
                    fetch_url,
                    owner_url,
                    target,
                    pinned,
                    seen,
                } => match self.gemini_trust.accept_change(&target, &pinned, &seen) {
                    Ok(()) => {
                        let fetch = self.app.fetch_page_effect(node, fetch_url, owner_url);
                        for command in browse::fetch_commands_for(&fetch, &mut self.pending_fetches)
                        {
                            self.fetch_handle.command(command);
                        }
                    }
                    Err(error) => {
                        let effects = self.app.apply_update(Update::ContentFailed {
                            node,
                            error: format!("could not replace Gemini trust for {target}: {error}"),
                        });
                        self.run_effects(effects);
                    }
                },
                Effect::SaveSession => self.save_session(),
                Effect::OpenPlace {
                    session,
                    generation,
                    binding,
                } => {
                    self.place_handle
                        .command(crate::place::worker::PlaceWorkerCommand::Open {
                            session,
                            generation,
                            directory: session::session_dir(&self.app.data_root, session),
                            binding,
                        });
                }
                Effect::JoinPlace {
                    session,
                    generation,
                    invite,
                } => {
                    self.place_handle
                        .command(crate::place::worker::PlaceWorkerCommand::Join {
                            session,
                            generation,
                            directory: session::session_dir(&self.app.data_root, session),
                            invite,
                        });
                }
                Effect::RunPlaceCommand {
                    session,
                    generation,
                    request,
                    command,
                } => {
                    self.place_handle
                        .command(crate::place::worker::PlaceWorkerCommand::Author {
                            session,
                            generation,
                            request,
                            command,
                        });
                }
                Effect::ResyncPlace {
                    session,
                    generation,
                } => {
                    self.place_handle
                        .command(crate::place::worker::PlaceWorkerCommand::Resync {
                            session,
                            generation,
                        });
                }
                Effect::ClosePlace { .. } => self.release_place_worker(),
                Effect::StoreImage { hex, bytes } => {
                    session::save_image_blob(&self.app.session_dir(), &hex, &bytes);
                }
                Effect::StoreDownload {
                    node,
                    url,
                    content_type,
                    content_disposition,
                    received_at_ms,
                    bytes,
                } => {
                    self.download_handle
                        .command(crate::download::DownloadCommand {
                            node,
                            url,
                            content_type,
                            content_disposition,
                            received_at_ms,
                            session_dir: self.app.session_dir(),
                            download_dir: crate::download::configured_download_dir(
                                &self.app.data_root,
                            ),
                            bytes,
                        });
                }
                // The bin port: stage the record; the actor answers with the
                // refreshed list (folded on the next wake).
                Effect::RecordDeleted { record } => {
                    self.bin_handle
                        .command(crate::recycle::BinCommand::Record(record));
                }
                Effect::EmptyRecycleBin => {
                    self.bin_handle.command(crate::recycle::BinCommand::Empty);
                }
                // The recall lane: the trail actor answers with hits carrying
                // the query back, and the app drops superseded answers.
                Effect::RecallQuery { query } => {
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Recall {
                            query,
                            limit: RECALL_ROW_LIMIT,
                        });
                }
                // The session switch (rung 6's second half). Ordering is the
                // point of this being an EFFECT: the departing session saves
                // under ITS directory while it is still the live state, the
                // ports tear down (live document sessions die with their
                // windows; lens windows close), and only then does the app
                // adopt the target — whose own effects (content respawns,
                // window reopens) run through the same loop.
                // The close path (overmap O3): release the bin store (its
                // open files block the dir rename on Windows), trash the
                // closing session's directory whole, then adopt the target
                // WITHOUT the departing save — a post-trash save would
                // resurrect the closed session as a zombie directory.
                Effect::TrashSession { closing, next } => {
                    self.release_place_worker();
                    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
                    self.bin_handle
                        .command(crate::recycle::BinCommand::Release(ack_tx));
                    if ack_rx
                        .recv_timeout(std::time::Duration::from_millis(1500))
                        .is_err()
                    {
                        tracing::warn!(
                            "bin release ack timed out; attempting the trash move anyway"
                        );
                    }
                    // The trail-memory store lives in the same session dir,
                    // so it releases (and flushes) under the same handshake.
                    let (trail_ack_tx, trail_ack_rx) = std::sync::mpsc::sync_channel(1);
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Release(trail_ack_tx));
                    if trail_ack_rx
                        .recv_timeout(std::time::Duration::from_millis(1500))
                        .is_err()
                    {
                        tracing::warn!(
                            "trail memory release ack timed out; attempting the trash move anyway"
                        );
                    }
                    self.app.apply_trash(closing);
                    self.content_sessions.clear();
                    self.clear_surface_content();
                    self.pending_surface_spawns.clear();
                    self.lens_windows.clear();
                    self.pending_lens_capture = None;
                    self.lens_divider_drag = None;
                    self.pending_windows.clear();
                    let fx = self.app.adopt_session(next);
                    self.bin_handle.command(crate::recycle::BinCommand::Reopen(
                        crate::recycle::bin_dir(&self.app.session_dir()),
                    ));
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Reopen(
                            crate::trail_memory::memory_dir(&self.app.session_dir()),
                        ));
                    self.run_effects(fx);
                    self.request_redraw();
                }
                Effect::SwitchSession { id } => {
                    self.save_session();
                    self.release_place_worker();
                    self.content_sessions.clear();
                    self.clear_surface_content();
                    self.pending_surface_spawns.clear();
                    self.lens_windows.clear();
                    self.pending_lens_capture = None;
                    self.lens_divider_drag = None;
                    self.pending_windows.clear();
                    let fx = self.app.adopt_session(id);
                    // Re-point the bin actor at the adopted session's store;
                    // it answers with THAT bin's list (the app cleared its
                    // mirror in adopt_session). The trail memory re-points
                    // with it (flushing the departing session's segments).
                    self.bin_handle.command(crate::recycle::BinCommand::Reopen(
                        crate::recycle::bin_dir(&self.app.session_dir()),
                    ));
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Reopen(
                            crate::trail_memory::memory_dir(&self.app.session_dir()),
                        ));
                    self.run_effects(fx);
                    self.request_redraw();
                }
                // The content port (rung 4, live since genet-documents
                // landed): route the address to an engine id, spawn through
                // the registry, hold the session keyed by node id. Every
                // failure — unroutable id, spawn error — surfaces as
                // ContentFailed; a Requested node never silently spins.
                Effect::SpawnContent { node, url } => {
                    let fetched = self.app.content.fetched(node, &url).cloned();
                    let pinned = self
                        .app
                        .browser
                        .get(node)
                        .and_then(|b| b.viewer_override.clone())
                        .or_else(|| {
                            (self
                                .content_engines
                                .contains(crate::knot_authoring::ENGINE_ID)
                                && crate::knot_authoring::is_knot_address(&url))
                            .then(|| crate::knot_authoring::ENGINE_ID.to_string())
                        });
                    let request = inker::EngineRouteRequest {
                        workspace_id: inker::WorkspaceRouteId::new("turnstone"),
                        view: None,
                        node: None,
                        address: url.clone(),
                        content_type: fetched
                            .as_ref()
                            .and_then(|document| document.content_type.clone()),
                        // The settings row: a sidecar viewer override pins the
                        // route, so a respawn lands on the chosen lane.
                        pinned_engine: pinned,
                    };
                    let decision = self.route_policy.route(&request);
                    if decision.engine_id == inker::routing::ENGINE_WELD_CHROMIUM {
                        if self.host.is_none() {
                            self.pending_surface_spawns.push((node, url));
                            continue;
                        }
                        let update = match self.ensure_weld_engine() {
                            Ok(()) => {
                                let profile = self
                                    .app
                                    .data_root
                                    .join("weld")
                                    .join("cef-cache")
                                    .join(node.to_string());
                                match std::fs::create_dir_all(&profile) {
                                    Ok(()) => {
                                        let spawn = SurfaceSpawnRequest {
                                            url: url.clone(),
                                            width: self.width.max(1),
                                            height: self.height.max(1),
                                            profile: EngineProfileBinding {
                                                user_data_dir: profile
                                                    .to_string_lossy()
                                                    .into_owned(),
                                            },
                                            fence_handle: None,
                                        };
                                        match self.surface_engines.spawn(&decision, &spawn) {
                                            Ok(producer) => {
                                                tracing::info!(%node, %url, engine = %decision.engine_id, "surface content live");
                                                self.surface_producers.insert(node, producer);
                                                Update::ContentSpawned {
                                                    node,
                                                    facts: Some(crate::content::ContentFacts {
                                                        engine: decision.engine_id.clone(),
                                                        structure: None,
                                                        lineage: None,
                                                    }),
                                                }
                                            }
                                            Err(error) => Update::ContentFailed {
                                                node,
                                                error: format!(
                                                    "{} ({})",
                                                    error, decision.engine_id
                                                ),
                                            },
                                        }
                                    }
                                    Err(error) => Update::ContentFailed {
                                        node,
                                        error: format!(
                                            "could not create Weld profile {}: {error}",
                                            profile.display()
                                        ),
                                    },
                                }
                            }
                            Err(error) => Update::ContentFailed {
                                node,
                                error: format!("{} ({})", error, decision.engine_id),
                            },
                        };
                        let effects = self.app.apply_update(update);
                        self.run_effects(effects);
                        continue;
                    }
                    // Network document sessions consume the actor's body. If
                    // the toggle arrived before that body, leave the app in
                    // Requested and let PageFetched re-issue this spawn. The
                    // engine must not perform a second top-level request.
                    if fetch::is_fetchable(&url) && fetched.is_none() {
                        if !self.pending_fetches.page_in_flight(&url, node, &url) {
                            let fetch = self.app.fetch_page_effect(node, url.clone(), url.clone());
                            for command in
                                browse::fetch_commands_for(&fetch, &mut self.pending_fetches)
                            {
                                self.fetch_handle.command(command);
                            }
                        }
                        continue;
                    }
                    let mut spawn = SessionSpawnRequest::new(&url)
                        .with_viewport(self.width.max(1), self.height.max(1));
                    if let Some(document) = fetched {
                        spawn = spawn.with_body(document.body);
                        if let Some(content_type) = document.content_type {
                            spawn = spawn.with_content_type(content_type);
                        }
                    }
                    let update = match self.content_engines.spawn(&decision.engine_id, &spawn) {
                        Ok(session) => {
                            tracing::info!(%node, %url, engine = %decision.engine_id, "content session live");
                            // Mirror the spawn-time facts into app truth (the
                            // adapter conversion): the engine id plus the
                            // structural read through the trait accessor —
                            // None stays None (a lane without introspection
                            // is reported, not synthesized).
                            let report = session.inspect();
                            let lineage = report.as_ref().and_then(|report| {
                                report.lineage.as_ref().map(|lineage| {
                                    crate::content::ExtractionLineageFacts {
                                        tool: lineage.tool.clone(),
                                        version: lineage.version.clone(),
                                        selector: lineage.selector.clone(),
                                        score: lineage.score,
                                        block_count: lineage.block_count,
                                    }
                                })
                            });
                            let facts = crate::content::ContentFacts {
                                engine: decision.engine_id.clone(),
                                structure: report.map(|r| crate::content::StructureFacts {
                                    title: r.title,
                                    headings: r.headings.len(),
                                    links: r.links.len(),
                                    outline: r
                                        .outline
                                        .into_iter()
                                        .map(|e| crate::content::OutlineFact {
                                            depth: e.depth,
                                            role: e.role,
                                            name: e.name,
                                        })
                                        .collect(),
                                }),
                                lineage,
                            };
                            project_reader_lineage_facet(
                                &mut self.app,
                                node,
                                facts.lineage.as_ref(),
                            );
                            let subresources = session.subresources();
                            self.content_sessions.insert(node, session);
                            for url in subresources {
                                if self.pending_fetches.note_subresource(&url, node) {
                                    self.fetch_handle
                                        .command(fetch::FetchCommand::Subresource(url));
                                }
                            }
                            Update::ContentSpawned {
                                node,
                                facts: Some(facts),
                            }
                        }
                        Err(err) => {
                            tracing::warn!(%node, %url, engine = %decision.engine_id, %err, "content spawn failed");
                            Update::ContentFailed {
                                node,
                                error: format!("{} ({})", err, decision.engine_id),
                            }
                        }
                    };
                    let effects = self.app.apply_update(update);
                    self.run_effects(effects);
                }
                Effect::UpdateContent { node, url } => {
                    let Some(document) = self.app.content.fetched(node, &url).cloned() else {
                        continue;
                    };
                    let engine = self
                        .app
                        .content
                        .facts(node)
                        .map(|facts| facts.engine.clone())
                        .unwrap_or_else(|| inker::routing::ENGINE_NEMATIC_GEMTEXT.to_string());
                    let Some(session) = self.content_sessions.get_mut(&node) else {
                        self.run_effects(vec![Effect::SpawnContent { node, url }]);
                        continue;
                    };
                    let Some(smolweb) = session
                        .as_any()
                        .downcast_mut::<genet_documents::SmolwebDocumentSession>()
                    else {
                        tracing::debug!(%node, %url, "live engine does not support incremental body replacement");
                        continue;
                    };
                    smolweb.replace_body(&url, &document.body);
                    let facts = crate::content::ContentFacts {
                        engine,
                        structure: session
                            .inspect()
                            .map(|report| crate::content::StructureFacts {
                                title: report.title,
                                headings: report.headings.len(),
                                links: report.links.len(),
                                outline: report
                                    .outline
                                    .into_iter()
                                    .map(|entry| crate::content::OutlineFact {
                                        depth: entry.depth,
                                        role: entry.role,
                                        name: entry.name,
                                    })
                                    .collect(),
                            }),
                        lineage: session.inspect().and_then(|report| {
                            report
                                .lineage
                                .map(|lineage| crate::content::ExtractionLineageFacts {
                                    tool: lineage.tool,
                                    version: lineage.version,
                                    selector: lineage.selector,
                                    score: lineage.score,
                                    block_count: lineage.block_count,
                                })
                        }),
                    };
                    for subresource in session.subresources() {
                        if self.pending_fetches.note_subresource(&subresource, node) {
                            self.fetch_handle
                                .command(fetch::FetchCommand::Subresource(subresource));
                        }
                    }
                    if let Some(host) = self.host.as_ref() {
                        host.core()
                            .invalidate_surface(crate::surface::SurfaceId::content(node).0);
                    }
                    let effects = self.app.apply_update(Update::ContentSpawned {
                        node,
                        facts: Some(facts),
                    });
                    self.run_effects(effects);
                }
                Effect::ControlContent { node, control } => {
                    let result = self
                        .surface_producers
                        .get_mut(&node)
                        .and_then(|producer| producer.as_web_surface())
                        .ok_or_else(|| "content has no web control plane".to_string())
                        .and_then(|web| {
                            match control {
                                crate::action::ContentControl::Back => web.go_back(),
                                crate::action::ContentControl::Forward => web.go_forward(),
                                crate::action::ContentControl::Reload => web.reload(),
                                crate::action::ContentControl::Stop => web.stop(),
                            }
                            .map_err(|error| error.to_string())
                        });
                    if let Err(error) = result {
                        tracing::warn!(%node, ?control, %error, "web content control failed");
                        self.app.content.note_surface_stopped(node);
                    }
                    self.request_redraw();
                }
                Effect::CloseContent { node } => {
                    if self.content_sessions.remove(&node).is_some() {
                        tracing::info!(%node, "content session closed");
                    }
                    if self.surface_producers.remove(&node).is_some() {
                        #[cfg(all(feature = "weld", windows))]
                        self.surface_frames.remove(&node);
                        tracing::info!(%node, "surface content closed");
                    }
                }
                Effect::Redraw => self.request_redraw(),
                // Window creation needs the ActiveEventLoop; note the request
                // and let the event handler in scope drain it.
                Effect::OpenWindow { ordinal } => self.pending_windows.push(ordinal),
                // Fetch-shaped effects were consumed above.
                Effect::FetchPage { .. }
                | Effect::CancelPage { .. }
                | Effect::FetchFeed { .. }
                | Effect::FetchFavicon { .. }
                | Effect::SubmitSmolweb { .. } => {}
            }
        }
    }

    /// Release retained place handles before a session directory is switched,
    /// moved, or left at shutdown.
    pub(super) fn release_place_worker(&mut self) {
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        self.place_handle
            .command(crate::place::worker::PlaceWorkerCommand::Release(ack_tx));
        if ack_rx
            .recv_timeout(std::time::Duration::from_millis(1500))
            .is_err()
        {
            tracing::warn!("place worker release ack timed out");
        }
    }

    /// Persist the live session's whole sidecar set under ITS directory
    /// (`sessions/<id>/`) — the SaveSession effect's body, shared by the
    /// session switch (which must save the DEPARTING session first).
    pub(super) fn save_session(&mut self) {
        let sdir = self.app.session_dir();
        session::save_session_graph(&sdir, self.app.graph_runtimes.graph());
        if let Err(error) = self.app.feeds.save(&sdir) {
            tracing::warn!(%error, "failed to persist feed subscriptions");
        }
        if let Some(binding) = self.app.place.binding() {
            match session::update_place_binding(&sdir, binding) {
                Ok(true) => {}
                // Loud rather than silent: app state holding a binding with no
                // admitted sidecar means something bound a place without going
                // through admission, which is the exact ordering T3a inverts.
                Ok(false) => tracing::warn!(
                    "place state carries a binding with no admitted place.json; \
                     refusing to create one outside admission"
                ),
                Err(error) => tracing::warn!(%error, "failed to persist place binding"),
            }
        }
        let swept = session::gc_orphan_image_blobs(&sdir, self.app.graph_runtimes.graph());
        if swept > 0 {
            tracing::info!(swept, "reclaimed orphaned session image blobs");
        }
        // The pane layout persists to frame.json alongside the graph
        // (rung 5 slice C), so summon/close/divider survive a restart.
        session::save_frisket_layout(&sdir, &self.app.frisket);
        // The workbench tiling persists as platen's canonical pair
        // (rung 5 slice E), so tiles/stacks/fractions survive too.
        if let Some(workbench) = self.app.active_workbench() {
            session::save_workbench(&sdir, workbench);
        }
        // The lens-window spaces (rung 7 depth): torn-out panes
        // survive a restart as windows again.
        session::save_lens_spaces(&sdir, &self.app.lenses);
        // Browser state (rung 6): content-on refreshed from live truth, so a
        // restart respawns what was showing; then the whole live state lands
        // in the facet store (arrangement.* + scene.* + web.*) via the shared
        // refresh (the fork's facet-carry reads the same refreshed store).
        self.app.refresh_browser_states();
        self.app.refresh_facets();
        if let Err(error) = crate::content_classes::reconcile(&mut self.app.graph_runtimes) {
            tracing::warn!(%error, "content-class reconciliation failed");
        }
        session::save_node_facets(&sdir, self.app.graph_runtimes.facets());
        if let Err(error) = self.app.gemini_identities.save(&self.app.data_root) {
            tracing::warn!(%error, "failed to persist Gemini identity bindings");
        }
        if let Some(score) = self.app.graph_runtimes.projection_score() {
            session::save_projection_score(&sdir, score);
        }
        // The score records what the solver produced; this records what the
        // viewer chose, which the score cannot be read backward to recover.
        session::save_view_intent(
            &sdir,
            &session::ViewIntentV1 {
                layout_strategy: self
                    .app
                    .graph_runtimes
                    .layout_strategy()
                    .map(str::to_string),
            },
        );
        // Stamp a derived display name the first time the session has content
        // to name it after (unset -> "Example Domain"), then bump recency so
        // the switcher orders by last-used. Derive before the mutable borrow.
        let id = self.app.session_id;
        let derived = self
            .app
            .sessions
            .get(id)
            .is_some_and(|m| m.display_name.is_none())
            .then(|| self.app.derive_session_name())
            .flatten();
        if self.app.sessions.update(id, |m| {
            if m.display_name.is_none()
                && let Some(name) = derived.clone()
            {
                m.display_name = Some(name);
            }
            m.touch();
        }) {
            let _ = self.app.sessions.flush_dirty();
        }
    }
}
