// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Inspect the current chrome projection policy without giving settings
    /// code access to the shell's transcript or provider registry.
    pub fn shell_chrome_config(&self) -> &ShellChromeConfig {
        self.shell.chrome()
    }

    /// Map is navigation over graph members. An inactive retained Workbench
    /// keeps its live producers, without compositing a reader over the map.
    pub(crate) fn map_navigation_active(&self) -> bool {
        let has_reader = self
            .frisket
            .iter_leaves()
            .any(|(_, content, _)| matches!(content, PaneContent::Workbench));
        if !has_reader {
            return false;
        }
        let visible = self
            .frisket
            .outer_workspace()
            .ok()
            .map(|frame| frame.active_ids())
            .unwrap_or_default();
        let reader_visible = visible
            .iter()
            .any(|id| self.pane_content(PaneId(id.0)) == Some(&PaneContent::Workbench));
        !reader_visible
            || self
                .active_pane
                .is_some_and(|pane| self.pane_content(pane) == Some(&PaneContent::Orrery))
    }

    /// Apply a value-facing chrome configuration. The Settings pane/provider
    /// may call this through its live sink; it never reaches into a renderer.
    pub fn set_shell_chrome_config(&mut self, chrome: ShellChromeConfig) {
        self.shell.set_chrome(chrome);
    }

    /// Apply a typed application-settings snapshot to the host-owned chrome
    /// projection. Returns whether a running presentation consumer needs a
    /// redraw. The snapshot is value-only; neither the provider nor its pane
    /// gains a path to a graph runtime or renderer.
    pub(crate) fn apply_chrome_settings_snapshot(
        &mut self,
        settings: &crate::settings_pane::ChromeSettings,
    ) -> bool {
        // Read per cascade, so changing the setting changes the very next one
        // without a restart. Assigned outside the chrome comparison below,
        // which answers "does the shell need relaying out": a budget change
        // needs no relayout, and folding it in would force a spurious one.
        self.cascade_budget = settings.cascade_budget();

        let mut chrome = self.shell_chrome_config().clone();
        settings.apply_to(&mut chrome);
        if chrome == *self.shell_chrome_config() {
            false
        } else {
            self.set_shell_chrome_config(chrome);
            true
        }
    }

    /// The transcript is read as shell data, independently from AppEvent.
    pub fn shell_transcript(&self) -> &crate::shell_services::ShellTranscript {
        self.shell.transcript()
    }

    /// A2's focused-pane router consumes a target requested by a transcript
    /// action. Until that lane lands, opening a target is observable but does
    /// not mutate the singleton canvas.
    pub fn take_requested_shell_context(&mut self) -> Option<ContextSnapshot> {
        self.shell.take_requested_context()
    }

    /// A temporary host-context fallback. It intentionally publishes only the
    /// active pane id and session id; it does not read `App::canvas`. A2
    /// replaces this with a `ContextIndex`/runtime-pool resolution at the same
    /// call site.
    pub(crate) fn fallback_shell_context(&self) -> ContextSnapshot {
        ContextSnapshot {
            pane: self.active_pane,
            context: crate::panes::PaneContext {
                session: Some(self.session_id),
                ..crate::panes::PaneContext::default()
            },
        }
    }

    /// The current session's container id — the root graph's uuid, the key the
    /// saved scene hangs on (the graph is the container node in the one-node
    /// model). `None` if the manifest is somehow absent.
    pub fn container_id(&self) -> Option<uuid::Uuid> {
        self.sessions
            .get(self.session_id)
            .map(|m| *m.root_graph_id.as_uuid())
    }

    /// Drive the active analytic layout strategy for this frame: recompute the
    /// projection when its inputs changed (the canvas's recompute gate) and
    /// buffer the positions into the canvas, which overlays them after the
    /// physics snapshot. The cartography host loop the canvas documents but
    /// no host ran until now (projection-engine proof 1). A no-op under
    /// force-directed. Called by the shell right before `canvas.frame()`.
    pub fn drive_layout_strategy(&mut self, w: u32, h: u32) {
        let graph = self.graph_runtimes.active_graph();
        let runtime = self
            .graph_runtimes
            .get_mut(graph)
            .expect("active graph runtime");
        let restored = std::mem::take(&mut runtime.scene_restore_pending);
        if !restored || !Self::prime_restored_layout(&mut runtime.canvas, w, h) {
            Self::drive_layout_strategy_for(&mut runtime.canvas, w, h);
        }
    }

    /// The first host frame adopts the saved paused geometry as its layout.
    /// Record the actual receiving viewport without replacing that geometry
    /// with a projection measured from fresh, transient selection/footprints.
    /// Subsequent viewport, graph or footprint changes use the normal gate.
    pub(super) fn prime_restored_layout(canvas: &mut mere::canvas::Canvas, w: u32, h: u32) -> bool {
        let Some(id) = canvas.layout_strategy().map(str::to_owned) else {
            return false;
        };
        if !canvas.physics_paused() {
            return false;
        }
        canvas.refresh_community_cache(&id);
        canvas.note_strategy_computed(&id, w, h, canvas.focused_key());
        true
    }

    /// Run the graph-scoped layout strategy while a particular pane has its
    /// view installed. The strategy sees selection only through that pane's
    /// temporary view state; it never chooses a global active pane.
    pub(super) fn drive_layout_strategy_for(
        canvas: &mut mere::canvas::Canvas,
        w: u32,
        h: u32,
    ) -> bool {
        let Some(id) = canvas.layout_strategy().map(str::to_string) else {
            return false;
        };
        canvas.refresh_community_cache(&id);
        let focus = canvas.focused_key();
        if canvas.needs_strategy_recompute(&id, w, h, focus) {
            // The host measures (per-node face footprints), the strategy
            // places — extent-aware spacing per the P2 contract.
            let extents = canvas.strategy_extents();
            let recent_first = canvas.size_by_recency();
            let previous = canvas.projection_score().cloned();
            let strategy = canvas.project_arrangement_for_view(
                &id,
                w,
                h,
                Some(&extents),
                recent_first,
                canvas.camera().zoom,
                previous.as_ref(),
            );
            canvas.apply_strategy_positions(&strategy.positions);
            canvas.set_projection_score(strategy.score);
            canvas.set_strategy_regions(strategy.regions);
            canvas.note_strategy_computed(&id, w, h, focus);
            return true;
        }
        false
    }

    /// Write the LIVE state into the facet store: the active graph's scene as
    /// its saved scene on the container node (positions are not graph truth,
    /// so the graph alone loses the layout; the scene carries it with the
    /// physics, camera and selection) and the browser map as `web.*`. Other
    /// namespaces are untouched. Shared by the shell's save path and the
    /// fork's facet-carry (both need the store to reflect the moment, not
    /// the last save).
    pub fn refresh_facets(&mut self) {
        let browser = self.browser.clone();
        if let Err(error) = self
            .graph_runtimes
            .edit_facets(|facets| pandect::write_web_states(facets, &browser))
        {
            tracing::warn!(%error, "could not project browser state facets");
        }
        let graph = self.graph_runtimes.active_graph();
        let container = self.container_id().unwrap_or(*graph.as_uuid());
        if let Some(runtime) = self.graph_runtimes.get_mut(graph) {
            crate::scene_state::stage_scene(&mut runtime.canvas, container, &runtime.view);
        }
    }

    /// A pane's content by id, in whichever space holds it (primary or a lens).
    pub fn pane_content(&self, pane: PaneId) -> Option<&PaneContent> {
        self.frisket
            .iter_leaves()
            .chain(self.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
            .find(|(id, _, _)| *id == pane)
            .map(|(_, content, _)| content)
    }

    /// The graph a pane is bound to, irrespective of which space hosts it.
    /// Callers that operate on graph truth must resolve this before touching a
    /// runtime; using the pool's compatibility cursor would silently retarget
    /// a command when focus changes.
    pub fn graph_for_pane(&self, pane: PaneId) -> Option<GraphId> {
        self.frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|space| space.iter_leaves()),
            )
            .find(|(id, _, _)| *id == pane)
            .map(|(_, _, graph)| graph)
            .filter(|graph| *graph != GraphId::nil())
    }

    /// The first graph pane in the primary space. This is only the initial
    /// focus choice for legacy actions, not an authority lookup.
    pub fn default_graph_pane(&self) -> PaneId {
        self.frisket
            .iter_leaves()
            .find(|(_, content, _)| matches!(content, PaneContent::Orrery))
            .map(|(id, _, _)| id)
            .unwrap_or(PaneId(0))
    }

    /// The Forme source currently available to a graph-bearing legacy pane.
    /// The old layout has no durable Forme picker yet, so it names the
    /// identity forme of its graph. The runtime boundary is nevertheless
    /// explicit: Workbench state is never keyed by application or window.
    pub fn forme_for_pane(&self, pane: PaneId) -> Option<FormeRuntimeKey> {
        let graph = self.graph_for_pane(pane)?;
        self.pane_content(pane)
            .filter(|content| content.follows_active_graph())?;
        Some(FormeRuntimeKey {
            graph,
            forme: mere::forme::FormeRef::Identity(*graph.as_uuid()),
        })
    }

    /// Register the Forme runtime named by a graph-bearing pane.
    pub fn ensure_identity_forme_for_pane(&mut self, pane: PaneId) -> Option<FormeRuntimeKey> {
        let key = self.forme_for_pane(pane)?;
        self.forme_runtimes.get_or_create(key.graph, key.forme);
        Some(key)
    }

    /// Read the Workbench arrangement named by a pane's graph/Forme source.
    pub fn workbench_for_pane(&self, pane: PaneId) -> Option<&mere::platen::Workbench> {
        let key = self.forme_for_pane(pane)?;
        self.forme_runtimes
            .get(key.graph, key.forme)
            .map(|runtime| &runtime.workbench)
    }

    /// Create or reuse the Workbench arrangement named by a pane's graph/Forme
    /// source. This is the A3 mutation entrance for Workbench state.
    pub fn workbench_for_pane_mut(&mut self, pane: PaneId) -> Option<&mut mere::platen::Workbench> {
        let key = self.ensure_identity_forme_for_pane(pane)?;
        Some(
            &mut self
                .forme_runtimes
                .get_or_create(key.graph, key.forme)
                .workbench,
        )
    }

    pub fn active_workbench(&self) -> Option<&mere::platen::Workbench> {
        self.workbench_owner_pane()
            .and_then(|pane| self.workbench_for_pane(pane))
    }

    /// The last context publisher in `pane`'s space that supplies a graph.
    /// Roster and Inspector follow this explicit context rather than the graph
    /// runtime pool's legacy cursor.
    pub fn follower_context(&self, pane: PaneId) -> Option<PaneContext> {
        if self.has_pane_binding(pane) {
            return self.pane_bound_context(pane);
        }
        self.pane_context.resolve_context(
            pane,
            &crate::panes::ContextBinding::FocusedInOwnSpace,
            SourceSelector::Graph,
        )
    }

    /// Publish a graph pane's own view selection. Only its `PaneId`-local
    /// selection can honestly provide the member field.
    pub fn publish_graph_context(&mut self, pane: PaneId) -> Option<PaneContext> {
        let key = self.forme_for_pane(pane)?;
        let context = PaneContext {
            graph: Some(key.graph),
            forme: Some(key.forme),
            member: self.graph_views.focused_member(pane),
            session: self
                .graph_runtimes
                .get(key.graph)
                .and_then(|runtime| runtime.session),
        };
        self.publish_pane_context(pane, context);
        Some(context)
    }

    /// Publish a Workbench or member pane's active member over its graph/Forme
    /// source. This is context publication, not a canvas selection side effect.
    pub fn publish_member_context(
        &mut self,
        pane: PaneId,
        member: Option<uuid::Uuid>,
    ) -> Option<PaneContext> {
        let key = self.forme_for_pane(pane)?;
        let context = PaneContext {
            graph: Some(key.graph),
            forme: Some(key.forme),
            member,
            session: self
                .graph_runtimes
                .get(key.graph)
                .and_then(|runtime| runtime.session),
        };
        self.publish_pane_context(pane, context);
        Some(context)
    }

    pub(super) fn publish_pane_context(&mut self, pane: PaneId, context: PaneContext) {
        let space = match self.space_of(pane) {
            Some(SpaceRef::Primary) => SpaceId::new("primary"),
            Some(SpaceRef::Lens(ordinal)) => SpaceId::new(format!("lens-{ordinal}")),
            None => return,
        };
        self.pane_context.place(pane, space);
        self.pane_context.publish(pane, context);
        // Rendering refreshes graph context too. It must not displace the
        // Workbench member that actually owns the user's current commands.
        if self.active_pane.is_none_or(|active| active == pane) {
            self.pane_context.focus(pane);
        }
    }

    /// Refresh the runtime index's pane-to-space mapping after a legacy layout
    /// load or topology edit. Context is live state; the Frisket tree remains
    /// the topology authority.
    pub fn index_pane_spaces(&mut self) {
        let panes: Vec<_> = self
            .frisket
            .iter_leaves()
            .map(|(pane, _, _)| (pane, SpaceId::new("primary")))
            .chain(self.lenses.iter().enumerate().flat_map(|(ordinal, space)| {
                space.iter().flat_map(move |layout| {
                    layout
                        .iter_leaves()
                        .map(move |(pane, _, _)| (pane, SpaceId::new(format!("lens-{ordinal}"))))
                })
            }))
            .collect();
        self.pane_context
            .retain_panes(&panes.iter().map(|(pane, _)| *pane).collect());
        for (pane, space) in panes {
            self.pane_context.place(pane, space);
        }
    }

    /// Borrow graph truth for one pane-scoped render or input pass. Installs
    /// and stashes camera and selection, then republishes the pane context.
    pub fn with_graph_pane<R>(
        &mut self,
        pane: PaneId,
        operation: impl FnOnce(&mut mere::canvas::Canvas) -> R,
    ) -> Option<R> {
        let graph = self.graph_for_pane(pane)?;
        self.graph_views.bind_graph(pane, graph);
        if let Some(id) = self.graph_views.installed_forme(pane).or_else(|| {
            self.forme_for_pane(pane).map(|key| match key.forme {
                mere::forme::FormeRef::Identity(id) => id,
                mere::forme::FormeRef::Stored(id) => id.as_uuid(),
            })
        }) {
            let place = crate::scope_navigation::ScopePlace {
                level: crate::scope_navigation::ScopeLevel::Forme(id),
                graph,
                root: None,
                focus: None,
                contexts: None,
            };
            let region = self.scope_forme_region(&place);
            self.graph_views.set_forme_region(pane, region);
            if self.graph_views.forme_scope(pane) == Some(id) {
                let members = self
                    .scope_forme_reader(&place)
                    .and_then(|p| self.workbench_for_pane(p))
                    .map(|w| w.open_members())
                    .unwrap_or_default();
                self.graph_views.set_forme_scope(pane, id, members);
            }
        }
        let result = {
            let canvas = self.graph_runtimes.canvas_mut(graph)?;
            self.graph_views.install(pane, canvas);
            let result = operation(canvas);
            self.graph_views.stash(pane, canvas);
            result
        };
        self.publish_graph_context(pane);
        Some(result)
    }

    /// The window slot whose workspace holds `pane`: the primary's 0, or a
    /// lens's ordinal plus one.
    pub fn pane_window_slot(&self, pane: PaneId) -> usize {
        if self.frisket.iter_leaves().any(|(id, _, _)| id == pane) {
            return 0;
        }
        self.lenses
            .iter()
            .position(|layout| {
                layout
                    .as_ref()
                    .is_some_and(|layout| layout.iter_leaves().any(|(id, _, _)| id == pane))
            })
            .map_or(0, |ordinal| ordinal + 1)
    }

    /// The scale one pane is composed at: its window's device scale. Every
    /// surface is laid out at `physical / scale` and rasterized at `scale`,
    /// and its pointers arrive through `/ scale`, the convention Graphshell's
    /// rootstock host keeps for the same canvas.
    pub fn pane_layout_scale(&self, pane: PaneId) -> f32 {
        self.shell_chrome_config()
            .layout_scale(self.pane_window_slot(pane))
    }

    pub(super) fn logical(&self, pane: PaneId, x: f32, y: f32) -> (f32, f32) {
        let scale = self.pane_layout_scale(pane);
        (x / scale, y / scale)
    }

    /// The Map's theme, resolved from the live appearance; `None` until a
    /// presentation exists, when the canvas keeps its own colours.
    pub fn map_theme(&mut self) -> Option<mere::canvas::CanvasTheme> {
        self.map_theme.resolve(&self.shell.chrome().appearance)
    }
}
