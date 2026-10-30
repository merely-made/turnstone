// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn effect_spawn_content(&mut self, effect: Effect) {
        match effect {
            Effect::SpawnContent { node, url } => {
                // Resolve private saved-source addresses before the ordinary
                // host visit, retained-clip, or fetch paths can claim them.
                let saved_source_namespace = url::Url::parse(&url).ok().is_some_and(|parsed| {
                    parsed.scheme() == "mere" && parsed.host_str() == Some("saved-source")
                }) || url.starts_with("mere://saved-source");
                let saved_target = if saved_source_namespace {
                    match crate::saved_sources::parse_address(&url) {
                        Ok(Some(target)) => Some(target),
                        Ok(None) => {
                            let effects = self.app.apply_update(Update::ContentFailed {
                                node,
                                error: "Invalid saved source address".into(),
                            });
                            self.run_effects(effects);
                            return;
                        },
                        Err(error) => {
                            let effects =
                                self.app.apply_update(Update::ContentFailed { node, error });
                            self.run_effects(effects);
                            return;
                        },
                    }
                } else {
                    None
                };
                let saved_source_document = if let Some(target) = saved_target.as_ref() {
                    match self.app.held_saved_source_document(node, &url, target) {
                        Ok(Some(document)) => Some(document),
                        Ok(None) => {
                            let effects = self.app.request_saved_source_read(
                                node,
                                url.clone(),
                                target.clone(),
                            );
                            self.run_effects(effects);
                            return;
                        },
                        Err(error) => {
                            let effects =
                                self.app.apply_update(Update::ContentFailed { node, error });
                            self.run_effects(effects);
                            return;
                        },
                    }
                } else {
                    None
                };
                // A document another mere holds is dialed before it is
                // opened. The visit is one place-worker round trip; the
                // spawn waits for it and then runs unchanged, so the
                // authoring engine never learns that anything was dialed.
                if saved_target.is_none() && self.request_visit_if_held_elsewhere(node, &url) {
                    return;
                }
                let clip = if let Some(document) = saved_source_document {
                    Some(document)
                } else {
                    match crate::retained_clip::request(&url) {
                        Ok(Some(crate::retained_clip::ClipRequest::Read(id))) => {
                            let resolved = self.app.clip_destination().and_then(|root| {
                                crate::retained_clip::read_sealed(
                                    &root,
                                    id,
                                    crate::retained_clip::limits()?,
                                    self.app.clip_sealer()?,
                                )
                            });
                            match resolved {
                                Ok(clip) => Some(clip.document()),
                                Err(error) => {
                                    let follow_up = self
                                        .app
                                        .apply_update(Update::ContentFailed { node, error });
                                    self.run_effects(follow_up);
                                    return;
                                },
                            }
                        },
                        Ok(None) => None,
                        _ => {
                            let follow_up = self.app.apply_update(Update::ContentFailed {
                                node,
                                error: "Invalid retained clip address".into(),
                            });
                            self.run_effects(follow_up);
                            return;
                        },
                    }
                };
                let fetched = self.app.content.fetched(node, &url).cloned();
                let pinned = if saved_target.is_some() {
                    Some(crate::saved_sources::ENGINE_ID.to_string())
                } else {
                    clip.as_ref()
                        .map(|_| crate::retained_clip::ENGINE_ID.to_string())
                        .or_else(|| {
                            self.app
                                .browser
                                .get(node)
                                .and_then(|b| b.viewer_override.clone())
                        })
                        .or_else(|| {
                            crate::nomadnet::is_micron_address(&url)
                                .then(|| crate::nomadnet::ENGINE_ID.to_string())
                        })
                        .or_else(|| {
                            (self
                                .content_engines
                                .contains(crate::knot_authoring::ENGINE_ID)
                                && crate::knot_authoring::is_knot_address(&url))
                            .then(|| crate::knot_authoring::ENGINE_ID.to_string())
                        })
                };
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
                match dispatch_content_spawn(
                    &decision.engine_id,
                    self.host.is_some(),
                    &mut self.pending_surface_spawns,
                    node,
                    &url,
                ) {
                    ContentSpawnDispatch::PendingBrowser => return,
                    ContentSpawnDispatch::Browser => {
                        let source = if request.pinned_engine.is_some() {
                            pelt_core::PeltRouteSource::UserOverride
                        } else {
                            pelt_core::PeltRouteSource::Automatic
                        };
                        let update = self.spawn_browser_surface(node, &url, &decision, source);
                        let effects = self.app.apply_update(update);
                        self.run_effects(effects);
                        return;
                    },
                    ContentSpawnDispatch::Session => {},
                }
                // Network document sessions consume the actor's body. If
                // the toggle arrived before that body, leave the app in
                // Requested and let PageFetched re-issue this spawn. The
                // engine must not perform a second top-level request.
                if crate::browse::is_fetchable(&url) && fetched.is_none() {
                    if !self.pending_fetches.page_in_flight(&url, node, &url) {
                        let fetch = self.app.fetch_page_effect(node, url.clone(), url.clone());
                        for command in browse::fetch_commands_for(&fetch, &mut self.pending_fetches)
                        {
                            self.command_fetch(command);
                        }
                    }
                    return;
                }
                let prepared = match prepare_content_controller(
                    &self.app,
                    node,
                    &url,
                    &decision.engine_id,
                    fetched,
                    clip,
                    (self.width.max(1), self.height.max(1)),
                ) {
                    Ok(Some(prepared)) => prepared,
                    Ok(None) => return,
                    Err(error) => {
                        let effects = self.app.apply_update(Update::ContentFailed { node, error });
                        self.run_effects(effects);
                        return;
                    },
                };
                let route = pelt_core::PeltRoute {
                    decision: decision.clone(),
                    source: if request.pinned_engine.is_some() {
                        pelt_core::PeltRouteSource::UserOverride
                    } else {
                        pelt_core::PeltRouteSource::Automatic
                    },
                    state: pelt_core::PeltRouteState::Document,
                };
                let opened = open_prepared_content_controller(
                    &self.app,
                    prepared,
                    std::sync::Arc::clone(&self.content_engines),
                    std::sync::Arc::clone(&self.pelt_surface_engines),
                    super::super::ShellClock(self.epoch),
                );
                let update = match opened {
                    Ok(Some(OpenedContentController {
                        mut controller,
                        source,
                    })) => {
                        if let Some(source) = source {
                            let effects = self.app.apply_update(source);
                            self.run_effects(effects);
                        }
                        let session = controller.session_mut();
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
                            lane: crate::content::ContentLane::Document,
                            capabilities: app_document_capabilities(
                                session.document_capabilities(),
                            ),
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
                        project_reader_lineage_facet(&mut self.app, node, facts.lineage.as_ref());
                        // The one seed point for a retained session's
                        // requested page scale: first spawn, restart replay
                        // and the engine-switch respawn all arrive here.
                        // The effective level it answers with is kept; a
                        // refusal is reported and dropped, and what the
                        // node asked for stays persisted either way.
                        replay_retained_page_scale(&mut self.app, node, session);
                        let subresources = session.subresources();
                        self.clear_reader_appearances(node);
                        // A document replacing a live surface takes the
                        // node's one content entry, so what the shell
                        // kept beside the surface retires with it.
                        if self.content_sessions.has_surface(&node) {
                            self.retire_surface_semantics(node);
                            self.surface_find_requests.remove(&node);
                            self.retire_surface_extras(node);
                        }
                        self.content_sessions.insert(
                            node,
                            pelt_core::PeltContent::from_controller(controller, route),
                        );
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
                    },
                    Ok(None) => return,
                    Err(err) => {
                        tracing::warn!(%node, %url, engine = %decision.engine_id, %err, "content spawn failed");
                        Update::ContentFailed {
                            node,
                            error: format!("{} ({})", err, decision.engine_id),
                        }
                    },
                };
                let effects = self.app.apply_update(update);
                self.run_effects(effects);
            },
            Effect::UpdateContent { node, url } => {
                let Some(document) = self.app.content.fetched(node, &url).cloned() else {
                    return;
                };
                let engine = self
                    .app
                    .content
                    .facts(node)
                    .map(|facts| facts.engine.clone())
                    .unwrap_or_else(|| inker::routing::ENGINE_NEMATIC_GEMTEXT.to_string());
                if engine == crate::nomadnet::ENGINE_ID {
                    // Generic smolweb body replacement selects by URL scheme.
                    // Re-enter the explicit Micron parser for this native address.
                    self.run_effects(vec![Effect::SpawnContent { node, url }]);
                    return;
                }
                let Some(session) = self.content_sessions.session_mut(&node) else {
                    self.run_effects(vec![Effect::SpawnContent { node, url }]);
                    return;
                };
                let Some(smolweb) = session
                    .as_any()
                    .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
                else {
                    tracing::debug!(%node, %url, "live engine does not support incremental body replacement");
                    return;
                };
                replace_fetched_smolweb_body(smolweb, &url, &document);
                let facts = crate::content::ContentFacts {
                    engine,
                    lane: crate::content::ContentLane::Document,
                    capabilities: app_document_capabilities(session.document_capabilities()),
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
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }
}
