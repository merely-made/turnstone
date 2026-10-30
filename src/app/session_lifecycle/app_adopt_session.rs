// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Adopt `id`'s persisted state wholesale — the load half of a boot and
    /// the whole of a switch. Rebuilds canvas / panes / workbench / browser /
    /// content from `sessions/<id>/` (missing files start fresh), reseeds
    /// history and the focus restore, and returns the adoption's effects
    /// (content respawns + lens-window reopens). Session-scoped view state
    /// (omnibar, active pane, maximize) resets.
    pub fn adopt_session(&mut self, id: crate::panes::SessionId) -> Vec<Effect> {
        // Invalidate old retained-source admissions before any fallible owner
        // open. Keep the healthy custody binding available for a successful
        // session projection after its new admissions are loaded.
        self.research.invalidate_projection();
        self.behavior_binding = None;
        self.session_id = id;
        self.await_source_capture_revalidation(id);
        self.saved_sources = Default::default();
        self.saved_readers.clear();
        self.physics_refusal = None;
        let graph = self
            .sessions
            .get(id)
            .map(|manifest| manifest.root_graph_id)
            .unwrap_or_else(GraphId::nil);
        let sdir = self.session_dir();
        let already_open = self.graph_runtimes.get(graph).is_some_and(|runtime| {
            runtime.session == Some(id) && runtime.persistence.owns_directory(&sdir)
        });
        if already_open {
            // Preserve the canonical in-memory graph and its lease on a
            // same-session re-adoption or switch-back.
            self.graph_runtimes.activate(graph);
        } else {
            self.graph_runtimes
                .activate_or_insert(graph, Some(id), Canvas::new());
            self.graph_runtimes.suspend_recording(graph);
            let writer = match crate::session_persistence::SessionWriteLease::acquire(&sdir) {
                Ok(writer) => writer,
                Err(error) => {
                    let error = format!("session writer lease refused: {error}");
                    tracing::warn!(%error, "refusing session adoption; disk state remains protected");
                    self.refuse_session_adoption(graph, sdir, error);
                    return vec![Effect::Redraw];
                },
            };
            match session::try_load_session_graph_locked(&sdir) {
                Ok(input) => {
                    let (graph, placement) =
                        input
                            .map(|input| (input.graph, input.placement))
                            .unwrap_or_else(|| {
                                (mere::kernel::graph::Graph::new(),
                            Some(pandect::graph_placement::PlacementProfile::RecordedStrataV1))
                            });
                    self.graph_runtimes.adopt_graph(
                        graph,
                        crate::session_persistence::SessionPersistence::writable_with_lease(
                            sdir.clone(),
                            placement,
                            writer,
                        ),
                    );
                },
                Err(error) => {
                    tracing::warn!(%error, "refusing session adoption; disk state remains protected");
                    self.refuse_session_adoption(graph, sdir, error);
                    return vec![Effect::Redraw];
                },
            }
        }
        self.graph_runtimes.suspend_recording(graph);
        session::record_current_session(&self.data_root, id);
        if self.sessions.update(id, |m| m.touch()) {
            if let Err(err) = self.sessions.flush_dirty() {
                tracing::warn!(%err, "failed to touch the adopted session's manifest");
            }
        }
        // This is the sole recovery boundary. Do not reload this store around
        // nested `update` calls: the live table is what prevents overlap.
        self.resident_runs =
            crate::resident_runs::ResidentRuns::with_limits(self.resident_run_storage_limits);
        self.resident_run_error = None;
        match crate::resident_runs::load_or_empty_with_limits(
            &sdir,
            self.resident_run_storage_limits,
        ) {
            Ok(mut runs) => {
                let recovered = runs
                    .validate_session(*id.as_uuid())
                    .and_then(|()| runs.mark_interrupted(crate::denizen::now_ms()))
                    .and_then(|()| crate::resident_runs::save(&sdir, &runs));
                match recovered {
                    Ok(()) => self.resident_runs = runs,
                    Err(error) => {
                        self.resident_run_error =
                            Some(format!("cannot recover resident run state: {error}"));
                    },
                }
            },
            Err(error) => {
                self.resident_run_error = Some(error);
            },
        }
        let mut effects = Vec::new();
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        let place_generation = self.next_place_generation;
        // A shared session opens its private graph cache immediately while the
        // worker materializes the retained Gemot, Commons, chat, and group
        // state. The worker answer is accepted only for this generation.
        // A left mark beside the binding means this session must not open
        // lanes or reconnect on its own: the retained counts come from the
        // marker `leave` wrote, cheap to read and exactly what `Left`'s own
        // status row already shows, rather than opening the retained stores
        // here just to recompute them before the person has even asked to
        // rejoin. Read once, ahead of the binding match below, so a marker
        // read failure and a marker hit share one branch instead of reading
        // the file twice.
        let left_mark = session::load_place_left(&sdir);
        self.place = match session::load_place_binding(&sdir) {
            Ok(Some(binding)) if !matches!(left_mark, Ok(None)) => {
                effects.push(Effect::ClosePlace {
                    session: id,
                    generation: place_generation,
                });
                match left_mark {
                    Ok(Some(retained)) => crate::place::PlaceState::Left { binding, retained },
                    Err(error) => {
                        tracing::warn!(%error, "place-left mark failed to load");
                        crate::place::PlaceState::Failed {
                            error: error.to_string(),
                        }
                    },
                    Ok(None) => unreachable!("guarded above"),
                }
            },
            Ok(Some(binding)) => {
                effects.push(Effect::OpenPlace {
                    session: id,
                    generation: place_generation,
                    binding: binding.clone(),
                });
                crate::place::PlaceState::Opening {
                    binding,
                    generation: place_generation,
                }
            },
            Ok(None) => {
                effects.push(Effect::ClosePlace {
                    session: id,
                    generation: place_generation,
                });
                crate::place::PlaceState::Personal
            },
            Err(error) => {
                tracing::warn!(%error, "place binding failed to load");
                effects.push(Effect::ClosePlace {
                    session: id,
                    generation: place_generation,
                });
                crate::place::PlaceState::Failed {
                    error: error.to_string(),
                }
            },
        };
        // The graph: restored, else fresh — swapped IN PLACE through the
        // canvas's own session-switch seam (mere's MG2 `set_graph`: physics
        // actor and node pool stay alive, every node parks at the origin and
        // halts; the saved layout is applied from the facet store next).
        self.feeds = crate::feed::FeedSubscriptions::load(&sdir);
        self.feeds.reconcile(self.graph_runtimes.graph());
        // The listening model rides the session directory like the feeds do.
        // A session with no audio output still reopens its library, progress
        // and notes; only the runtime is absent.
        // The new session keeps the shell's output and fetch handle.
        self.redshank = self.redshank.reopen(&sdir);
        self.redshank_members.clear();
        self.reconcile_redshank_fields();
        if let Some(score) = session::load_projection_score(&sdir) {
            self.graph_runtimes.restore_projection_score(score);
        }
        // Restore the chosen arrangement beside the score it produced, so a
        // reopened session comes back to the view the user left rather than to
        // the surface default. Set through the runtimes directly: the app-level
        // arm clears the score for a non-Spiral strategy, which is right for a
        // fresh choice and wrong for a restore of the pair.
        let pending_scope = self
            .reading
            .scope
            .pending
            .is_some()
            .then(|| std::mem::take(&mut self.reading.scope));
        self.reading = crate::reading_view::ReadingState::load(&sdir);
        if let Some(scope) = pending_scope {
            self.reading.scope = scope;
        }
        // The folds and the command menu ride the view intent; the scene
        // itself is restored below, once the facets are reconciled.
        let mut opened_view = {
            let graph = self.graph_runtimes.active_graph();
            let runtime = self.graph_runtimes.get_mut(graph).expect("active graph");
            crate::scene_state::open_view(&sdir, graph, &mut runtime.canvas)
        };
        self.command_choices = opened_view.commands.clone();
        // Preview imagery lives out of the graph now. The first paint queues
        // only visible cache misses; the shell resolves those after the frame,
        // so adoption does not decode the entire session into memory.
        // The facet store (`facets.json`): pruned to the live graph's nodes
        // (a deleted node's facets go with it), then the arrangement.* family
        // re-dresses the canvas — the durable layout, since the graph itself
        // is position-free. A session with no facets keeps the origin park and
        // settles fresh on the first nudge. Order per the canvas seams:
        // positions seed first (halting physics), sprites before their hulls,
        // faces after sprites (so a switched-off sprite face stays switched).
        // The removed-sessions cache (overmap O3): derived from the manifest
        // trash, refreshed here and on close/recover.
        self.trash = self.sessions.list_trash();
        // Canonical facets were read fallibly and overlaid during graph adoption.
        // Current member-keyed sidecar state wins over cached feed projections.
        self.reconcile_feed_tags();
        // A profile saved before the nil-GraphId heal keyed its scene.* facets
        // by the nil uuid; move them onto the healed container id once.
        if let Some(container) = self.container_id() {
            let nil = uuid::Uuid::nil();
            if container != nil && self.graph_runtimes.facets().facets_of(&nil).is_some() {
                let donor = self.graph_runtimes.facets().clone();
                if let Err(error) = self.graph_runtimes.edit_facets(|facets| {
                    pandect::copy_scene_facets(&donor, facets, nil, container);
                    facets.remove_node(&nil);
                }) {
                    tracing::warn!(%error, "nil-container scene facets could not be migrated");
                }
            }
        }
        let mut present: std::collections::BTreeSet<uuid::Uuid> = self
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(_, n)| n.id)
            .collect();
        // Keep the container's `scene.*` facets through the reconcile: the
        // container id is not a leaf graph node, so without this the prune
        // would sweep the scene settings away.
        if let Some(container) = self.container_id() {
            present.insert(container);
        }
        if let Err(error) = self
            .graph_runtimes
            .edit_facets(|facets| pandect::retain_present_nodes(facets, &present))
        {
            tracing::warn!(%error, "orphaned graph facets could not be pruned");
        }
        match crate::content_classes::reconcile(&mut self.graph_runtimes) {
            Ok(changed) if changed > 0 => {
                tracing::info!(
                    changed,
                    "founded built-in content classes on restored nodes"
                );
                effects.push(Effect::SaveSession);
            },
            Ok(_) => {},
            Err(error) => tracing::warn!(%error, "content-class reconciliation failed"),
        }
        let container = self.container_id();
        let restored = crate::scene_state::restore_scene(
            &mut self.graph_runtimes,
            container,
            &mut opened_view,
        );
        for result in restored.refusals {
            self.record_physics_result(result);
        }
        let has_saved_positions = restored.positions;
        let physics_paused = restored.paused;
        {
            let graph = self.graph_runtimes.active_graph();
            self.graph_runtimes
                .get_mut(graph)
                .expect("active graph")
                .view = opened_view.state;
        }
        // The participant runtime derives from the binding facets (agency) + the
        // graph's `Node.nested` pointers (structure) + the nested logs.
        self.pending_install = None;
        self.denizens = crate::denizen::rebuild(
            self.graph_runtimes.facets(),
            self.graph_runtimes.graph(),
            &sdir,
            self.personae_root(),
        );
        // Residency came back; its standing subscriptions have to come with
        // it, or a behavior silently stops waking after a reload.
        (
            self.watches,
            self.app_watches,
            self.time_watches,
            self.deadbands,
        ) = crate::denizen::load_watches(&self.session_dir());
        // One-time heal for bindings written before the containment ruling:
        // move the world pointer onto the node (journaled through the spine)
        // and rewrite the facet without it.
        for (member, log_id) in std::mem::take(&mut self.denizens.legacy_heals) {
            let _ = self
                .graph_runtimes
                .set_node_nested_for(member, Some(mere::kernel::graph::LogId::new(log_id)));
            if let Some(binding) =
                pandect::read_denizen_binding(self.graph_runtimes.facets(), member)
            {
                if let Err(error) = self
                    .graph_runtimes
                    .edit_facets(|facets| pandect::write_denizen_binding(facets, member, &binding))
                {
                    tracing::warn!(%error, %member, "participant binding could not be migrated");
                }
            }
        }
        // Session-scoped view state resets.
        self.omnibar = OmnibarState::default();
        self.document_find = crate::document_find::DocumentFindState::default();
        self.user_agent_decision = crate::user_agent_decision::UserAgentDecisionState::default();
        self.focus = FocusTarget::Graph(self.default_graph_pane());
        self.active_pane = None;
        self.maximized = None;
        self.roster_tab = 0;
        // The pane layout, and the lens-window spaces: each live slot gets
        // its window reopened through the ordinary OpenWindow effect — the
        // same port a fresh tear-out uses, so a restored window is spawned
        // truth, not painted memory. The id ceiling spans EVERY space.
        self.frisket = session::load_frisket_layout(&sdir).unwrap_or_default();
        self.lenses = session::load_lens_spaces(&sdir);
        // A5's live bridge is intentionally not written into the legacy
        // frame sidecars. A session adopt therefore starts from those durable
        // Frisket payloads and re-promotes a space only on its next float.
        self.primary_blueprint = None;
        self.lens_blueprints = vec![None; self.lenses.len()];
        let valid_graphs = self
            .sessions
            .iter()
            .map(|(_, manifest)| manifest.root_graph_id)
            .collect();
        self.frisket.retag_graph_bound_invalid(&valid_graphs, graph);
        for lens in self.lenses.iter_mut().flatten() {
            lens.retag_graph_bound_invalid(&valid_graphs, graph);
        }
        self.index_pane_spaces();
        self.focus = FocusTarget::Graph(self.default_graph_pane());
        for (ordinal, space) in self.lenses.iter().enumerate() {
            if space.is_some() {
                effects.push(Effect::OpenWindow { ordinal });
            }
        }
        self.next_pane_id = self
            .frisket
            .iter_leaves()
            .map(|(id, _, _)| id.0)
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|s| s.iter_leaves().map(|(id, _, _)| id.0).collect::<Vec<_>>()),
            )
            .max()
            .unwrap_or(0)
            + 1;
        // The legacy workbench sidecar belongs to this session's identity
        // forme. Other Forme runtimes are lazy and therefore cannot overwrite
        // this restored arrangement by merely appearing in another pane.
        let present = self
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(_, n)| n.id)
            .collect();
        let primary_graph_pane = self.default_graph_pane();
        if let Some(workbench) = self.workbench_for_pane_mut(primary_graph_pane) {
            *workbench = session::load_workbench(&sdir, &present);
        }
        // The history seeds from wherever the session opens (the focused
        // node's url, or an empty sentinel Back can never step past).
        self.history = chrome::nav::History::new(
            self.graph_runtimes
                .focused_url()
                .map(str::to_string)
                .unwrap_or_default(),
        );
        // Restore WHERE the user was (rung 6): re-select the most recently
        // visited node when nothing is selected (restored live content
        // composes for the FOCUSED node), and CENTER the camera on it — the
        // adopted session opens looking at its focus, not at whatever the
        // default origin happens to crop.
        if self.graph_runtimes.focused_member().is_none()
            && let Some(last) = crate::session::recent_visited_urls(self.graph_runtimes.graph(), 1)
                .into_iter()
                .next()
        {
            self.graph_runtimes.select_by_url(&last);
        }
        self.graph_runtimes.center_on_selected();
        // Seed the primary graph pane's published context after restore. Its
        // view is first installed lazily, so a restored graph selection is
        // captured into that pane rather than becoming a global follower
        // source.
        let primary_graph_pane = self.default_graph_pane();
        let active_graph = self.graph_runtimes.active_graph();
        if let Some(canvas) = self.graph_runtimes.canvas_mut(active_graph) {
            self.graph_views
                .bind_graph(primary_graph_pane, active_graph);
            self.graph_views.install(primary_graph_pane, canvas);
        }
        self.publish_graph_context(primary_graph_pane);
        // Browser state + content-state restore: read from the web.* facets
        // (the converged home); a pre-convergence profile's browser_nodes.json
        // seeds nodes the facets don't know (one-time legacy absorb — the next
        // save writes facets only, and the stale file is left inert). Every
        // node whose content was ON respawns through the ordinary port, so
        // `Live` here is spawned truth, never a painted memory.
        self.browser = pandect::read_web_states(self.graph_runtimes.facets());
        for (id, legacy) in session::load_legacy_browser_nodes(&sdir).nodes {
            self.browser.nodes.entry(id).or_insert(legacy);
        }
        if super::super::migrate_retired_viewer_overrides(&mut self.browser) {
            effects.push(Effect::SaveSession);
        }
        // The bin mirror empties until the reopened session store answers
        // (the shell re-points the bin actor on switch; BinListed refills).
        self.removed.clear();
        self.content = ContentStates::default();
        self.link_preview = None;
        for (_, node) in self.graph_runtimes.graph().nodes() {
            if self.browser.get(node.id).is_some_and(|b| b.content_on) {
                self.content.note_requested(node.id);
                effects.push(Effect::SpawnContent {
                    node: node.id,
                    url: node.url().to_string(),
                });
            }
        }
        // Apply the explicit pause after all layout/scene restoration, which
        // can seed or reconcile physics. Never let the first frame move a
        // session the person saved paused.
        if let Some(paused) = physics_paused {
            self.graph_runtimes.set_physics_paused(paused);
        }
        let graph = self.graph_runtimes.active_graph();
        let runtime = self
            .graph_runtimes
            .get_mut(graph)
            .expect("adopted graph runtime");
        runtime.scene_restore_pending = physics_paused == Some(true)
            && has_saved_positions
            && runtime.canvas.layout_strategy().is_some();
        self.window_count = 1;
        if let Err(error) = self.bind_behavior_journal() {
            self.refuse_behavior(error);
        }
        let label = self.session_label(id);
        self.events.push(AppEvent::SessionSwitched(label));
        effects.extend(self.restore_scope_after_session());
        effects.push(Effect::Redraw);
        effects
    }

    pub(crate) fn session_load_refused(&self) -> bool {
        matches!(
            self.graph_runtimes.active_persistence(),
            crate::session_persistence::SessionPersistence::Refused { .. }
        )
    }
}
