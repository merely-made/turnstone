//! Turnstone's core state and the two update functions — the middle of the
//! spine: `Action -> update -> Effects` and `Update -> apply_update ->
//! Effects`. Holds data, never handles: the ports (actors, stores, the
//! window) live in the shell, which runs the effects this module returns.

use std::path::PathBuf;

use crate::panes::{
    ContextIndex, FrisketLayout, GraphId, InsertSide, PaneContent, PaneContext, PaneId, PaneNode,
    SourceSelector, SpaceBlueprint, SpaceId,
};
use crate::shell_services::{ContextSnapshot, ShellChromeConfig, ShellServices};
use crate::ui::{OmnibarState, Suggestion, normalize_address};

use crate::action::{Action, Effect, SpaceRef, Update};
use crate::content::ContentStates;
use crate::observe::AppEvent;
use crate::surface::FocusTarget;
use crate::{browse, session};

mod contributed_pane_arms;
mod document_find_arms;
mod runtime_pool;

pub use runtime_pool::{
    FormeRuntime, FormeRuntimeKey, FormeRuntimePool, GraphPaneViews, GraphRuntime, GraphRuntimePool,
};

/// The window size a fresh App assumes until the shell resizes it: an
/// ordinary desktop window, so a headless projection (tests, the remote lens)
/// offers the same rows a real one would rather than collapsing to the floor.
pub const DEFAULT_VIEWPORT: (f32, f32) = (1280.0, 800.0);

/// Canonicalize a persisted or requested viewer id after retiring Turnstone's
/// incumbent static HTML lane. The old id described the same product rung, so
/// preserving the pin means moving it to Livery rather than clearing it.
fn canonical_viewer_override(viewer: Option<String>) -> Option<String> {
    viewer.map(|engine_id| {
        if engine_id == inker::routing::ENGINE_GENET_WEB {
            inker::routing::ENGINE_GENET_LIVERY.to_string()
        } else {
            engine_id
        }
    })
}

/// Upgrade all legacy `genet.web` pins in one loaded session. Returns whether
/// the caller should persist the changed facet state.
fn migrate_retired_viewer_overrides(
    states: &mut pandect::browser_node_state::BrowserNodeStates,
) -> bool {
    let mut migrated = false;
    for state in states.nodes.values_mut() {
        if state.viewer_override.as_deref() == Some(inker::routing::ENGINE_GENET_WEB) {
            state.viewer_override = Some(inker::routing::ENGINE_GENET_LIVERY.to_string());
            migrated = true;
        }
    }
    migrated
}

/// The at-rest "where am I" caption: the focused node's display label (and
/// host, when it adds information), or `None` with nothing focused.
pub fn focused_caption(canvas: &mere::canvas::Canvas) -> Option<String> {
    let url = canvas.focused_url()?.to_string();
    let graph = canvas.graph();
    let (key, node) = graph.get_node_by_url(&url)?;
    let label = graph.node_display_label(key);
    let host = url::Url::parse(node.url())
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned));
    match host.as_deref() {
        Some(host) if !label.contains(host) => Some(format!("{label}  \u{00b7}  {host}")),
        _ => Some(label),
    }
}

/// A composable pane's name for its palette rows ("Gloss", "Overmap"): the
/// pane's own tag, title-cased. Derived rather than tabled, so a pane that
/// gains a composition names itself.
fn pane_label(content: &PaneContent) -> String {
    crate::panes::pane_definition(content.kind_id().as_str())
        .map(|definition| definition.display_name.to_string())
        .unwrap_or_else(|| content.tag().to_string())
}

/// The application state: hosted graph runtimes, chrome state, and session
/// persistence. A graph pane resolves one runtime; the application itself
/// does not own one singleton graph surface.
pub struct App {
    /// Live graph authority, keyed by `GraphId`. The active cursor supports
    /// legacy callers until their PaneId routing is migrated; it is not a
    /// window- or space-level graph owner.
    pub graph_runtimes: GraphRuntimePool,
    /// Per-Graph-pane view state, never stored in a graph runtime.
    pub graph_views: GraphPaneViews,
    /// Arrangement runtime scopes, keyed independently from graph truth and
    /// pane view state.
    pub forme_runtimes: FormeRuntimePool,
    /// Context published by graph, workbench, and member panes. Followers
    /// resolve it in their own space instead of consulting a canvas cursor.
    pub pane_context: ContextIndex,
    /// The summonable omnibar (rung 3): find over graph truth, go through
    /// OpenAddress, `>` for the actions lane.
    pub omnibar: OmnibarState,
    /// Find in the captured active document. This is separate from the
    /// omnibar's graph recall and command lanes.
    pub document_find: crate::document_find::DocumentFindState,
    /// Per-frame stage costs for the chrome path. Data only, like the rest of
    /// App: the shell writes its own stages into it during `render` and clears
    /// it once the frame is reported. It lives here rather than on Shell
    /// because suggestion recomputation — the first stage the palette open-lag
    /// note names — happens on App's input edge, outside any frame.
    pub frame_timings: crate::frame_timing::FrameTimings,
    /// Shell-owned services projected by chrome: provider registration,
    /// interaction transcript, and configurable chrome policy. It carries no
    /// `Canvas` or platform handle. A2 replaces the fallback context supplied
    /// by this host with focused-pane context from the runtime pool.
    shell: ShellServices,
    /// The per-user data root. Each session's sidecars live under its own
    /// `sessions/<id>/` (rung 6's second half); the root also carries the
    /// manifest set and the current-session marker.
    pub data_root: PathBuf,
    /// The manifest set: one durable record per session, ManifestStore's
    /// on-disk layout under `sessions/`.
    pub sessions: pandect::ManifestStore,
    /// The live session — the one whose directory every save/load targets.
    pub session_id: crate::panes::SessionId,
    /// Per-node content lifecycle (rung 4). Data only: the live session
    /// handles live in the shell's content port, keyed by the same ids.
    pub content: ContentStates,
    /// Durable feed schedules and duplicate-suppression state for this
    /// session. Entry nodes and unread markers themselves remain graph truth.
    pub feeds: crate::feed::FeedSubscriptions,
    /// This session's public shared-place binding and product-visible status.
    /// Domain stores, joined lanes, transport handles, and key state belong to
    /// the shell-owned place worker; this field remains data only.
    pub place: crate::place::PlaceState,
    /// Monotonic identity for place-worker opens. It is never reset on a
    /// session switch, so a late answer cannot alias a later visit.
    pub(crate) next_place_generation: u64,
    /// Monotonic id for authored place commands, so a late answer is
    /// attributable to the command that asked for it.
    pub(crate) next_place_request: u64,
    /// Correlation id for explicit smolweb writes. Never reused within the
    /// process, so a late receipt cannot attach to a later composer.
    pub(crate) next_smolweb_submission: u64,
    pub(crate) active_smolweb_submission: Option<u64>,
    /// Which surface receives semantic input (rung 5 slice A). The explicit
    /// replacement for the old `omnibar.open` routing boolean: a third surface
    /// class (panes) joins by adding a `FocusTarget` variant rather than
    /// threading another bool through the shell. `omnibar.open` stays the
    /// omnibar's own display state; opening/closing it keeps this in sync.
    pub focus: FocusTarget,
    /// Prospective graph member under the pointer. Ephemeral presentation
    /// state: a click still goes through the ordinary navigation action.
    pub link_preview: Option<String>,
    /// The pane tree (rung 5 slice C): frisket's split tree of `PaneContent`
    /// leaves. The Orrery leaf is the graph canvas; summoning a pane splits it.
    /// Persisted to `frame.json` through the session port.
    pub frisket: FrisketLayout,
    /// Cross-node visit memory for recall/trail suggestions. Browser Back and
    /// Forward use each focused node's own durable lineage instead.
    pub history: chrome::nav::History,
    /// The active pane — the anchor a summon splits from and a close removes.
    /// `None` means the canvas (the Orrery leaf).
    pub active_pane: Option<PaneId>,
    /// The browser-state sidecar (rung 6): per-node browser handling (viewer
    /// override, compat mode, content-on), persisted at `browser_nodes.json`.
    /// The graph stays correct without it (the sidecar's charter).
    pub browser: pandect::browser_node_state::BrowserNodeStates,
    /// Linear damping for the layout physics (the "inertia" setting). Held here
    /// — the canvas is the sink, the host the durable owner — and persisted as
    /// the `scene.physics_damping` container facet (it left the app-wide
    /// settings store, being scene-scoped, not app-scoped).
    pub physics_damping: f32,
    /// A maximized pane takes the whole pane area (a host view state; frisket
    /// has no maximize op). Not persisted; resets on restart.
    pub maximized: Option<PaneId>,
    /// How many windows are open (rung 7). A MIRROR like `roster_tab`: the
    /// shell owns the platform windows and copies the count here so
    /// observation (and a scenario) can see it.
    pub window_count: usize,
    /// The primary window's size in device px, as the shell last resized it.
    /// App truth because the omnibar's row count depends on it; the default
    /// is an ordinary desktop window so a headless App projects sensibly
    /// before any resize arrives.
    pub viewport: (f32, f32),
    /// Each lens window's pane space (rung 7 depth: windows are pane HOSTS,
    /// not canvas-only): a frisket tree over the one App, indexed by the lens
    /// ordinal the shell's window records carry. `None` = that lens closed
    /// (tombstoned so ordinals stay stable). The primary window's space stays
    /// `frisket` above. Persisted at `windows.json` (rung 7 depth), so the
    /// windows come back as windows.
    pub lenses: Vec<Option<FrisketLayout>>,
    /// The A5 station authority for a space after it first enters the floating
    /// layer. Frisket remains the pre-A8 durable payload and renderer lookup;
    /// this blueprint is the live compositor topology for that active space.
    pub(crate) primary_blueprint: Option<SpaceBlueprint>,
    /// The matching A5 bridge state for lens spaces. Tombstones mirror
    /// `lenses`, keeping window ordinals stable while a lens is closed.
    pub(crate) lens_blueprints: Vec<Option<SpaceBlueprint>>,
    /// Which Roster tab is showing. A MIRROR, not the truth: cambium's tab strip
    /// owns its selection (the widget's state, in the shell's runner), and the
    /// shell copies it here after each dispatch so observation can see it — the
    /// inverse of `content`, where the app holds the data and the shell holds the
    /// live handle. Not persisted yet; restoring a pane's tab wants this on the
    /// frisket leaf rather than on App, once a second pane grows tabs.
    pub roster_tab: usize,
    /// The recycle bin's contents, MIRRORED from the bin port (the eidetic
    /// deleted-node bin at `sessions/<id>/bin`; `Update::BinListed` replaces
    /// this wholesale — the actor answers every record/reopen/spawn with the
    /// refreshed list). Data only, like `content`: the store handle lives in
    /// the shell's actor. Feeds the Trail's Removed section (records whose
    /// node is absent from the graph); recovery restores the ORIGINAL id.
    pub removed: Vec<crate::action::RemovedRecord>,
    /// The omnibar recall lane's current hits, MIRRORED from the trail port
    /// (lexical recall over `sessions/<id>/memory`). Data only, like
    /// `removed`: the index handle lives in the shell's actor.
    pub recall: Vec<crate::action::RecallHit>,
    /// The needle those hits were last ASKED for (not the one they answer):
    /// a late answer whose query no longer matches is dropped, so the lane
    /// can never show hits for text the user has already typed past.
    pub recall_query: String,
    /// A staged denizen install awaiting its visible grant review (B1).
    pub pending_install: Option<crate::denizen::PendingInstall>,
    /// The session's denizen runtime: residents, derived authority, the gate.
    pub denizens: crate::denizen::Denizens,
    /// The profile's root identity: whose authority every denizen grant
    /// descends from (capability-model OQ2). Vault-sealed when a personae
    /// backend exists (the SHARED vault, so this is the user's actual
    /// identity); the loud unsealed fallback otherwise. Install signs a
    /// delegation with it; uninstall revokes that delegation.
    pub identity: std::sync::Arc<crate::identity::RootIdentity>,
    /// Capsule approvals for the active Personae root. The sidecar contains
    /// origin mappings only; client certificate keys are derived on demand.
    pub gemini_identities: crate::gemini_identity::GeminiIdentityBindings,
    /// The attributed edit journal (mere's spine): every graph mutation
    /// captured under its author — `user` for the UI, a denizen's subject hex
    /// during a run. Shared with the capture hook installed at boot.
    pub journal: std::sync::Arc<std::sync::Mutex<mere::kernel::graph::GraphJournal>>,
    /// Standing subscriptions: which denizen wakes on what (graph behaviors
    /// W0/W1). Empty until a behavior is installed, which is why the drain
    /// costs nothing on a session that has none.
    pub watches: servitor::WatchTable,
    /// The app tier's own table (W3). A separate table because a watch cursor
    /// is a position in ONE journal, and these two count different things: a
    /// `GraphJournal` sequence and an app-event ordinal. Sharing a table would
    /// have the two seq spaces advance each other's cursors past unread work.
    pub app_watches: servitor::WatchTable,
    /// How far the behavior drain has read the undrained event queue.
    events_seen: usize,
    /// Events the shell has already taken. The queue's index plus this is a
    /// monotonic ordinal, which is what an app-tier watch cursor counts.
    events_base: u64,
    /// Whether a behavior drain is already running.
    ///
    /// `lower_denizen_actions` lowers a body's Actions through `update`, and
    /// `update` ends in the drain, so running a behavior re-enters it. Today
    /// the graph tier survives that by accident (its table is taken for the
    /// duration, so the nested pass sees nothing), but the clock and app tiers
    /// are not taken and would fire mid-cascade. A flag makes the answer
    /// structural instead of incidental.
    pub(crate) draining: bool,
    /// Schedules: behaviors that wake on the clock (W4).
    pub time_watches: servitor::TimeWatchTable,
    /// Per-behavior actuation bounds and their last accepted output. This is
    /// state, like a watch cursor: losing it on restart reopens a loop that had
    /// already been refused.
    pub deadbands: servitor::DeadbandTable,
    /// The host's clock, in unix milliseconds, fed in rather than sampled.
    ///
    /// `None` on a host that supplies none, and then no schedule ever fires:
    /// the same posture woodshed takes with practice timing, where measuring
    /// nothing beats measuring from the epoch. Injected so a replay wakes the
    /// same behaviors at the same points.
    pub now_ms: Option<u64>,
    /// How far the behavior drain has read the journal. Separate from any
    /// watch's own cursor: this one bounds which entries are even considered.
    pub behavior_cursor: u64,
    /// Rounds one cascade may run, from settings (live; floored at 1 by
    /// `CascadeBudget`).
    pub cascade_budget: u32,
    /// The manifest trash, cached (overmap O3): each closed session's whole
    /// directory sits under `.trash/`, so the trash IS the removed-sessions
    /// record — derived, no parallel bin. Refreshed on adopt / close /
    /// recover (list_trash reads the disk; the Trail renders per frame).
    pub trash: Vec<pandect::GraphSessionManifest>,
    /// Next pane id to mint. Kept above every id in the layout so a summon after
    /// a restore never collides with a persisted pane.
    next_pane_id: u64,
    /// Semantic events since the last drain (the observation pair's stream
    /// half; the shell drains each frame). Data, like everything else here.
    events: Vec<AppEvent>,
}

impl App {
    /// Inspect the current chrome projection policy without giving settings
    /// code access to the shell's transcript or provider registry.
    pub fn shell_chrome_config(&self) -> &ShellChromeConfig {
        self.shell.chrome()
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
    /// `scene.*` facets hang on (the graph is the container node in the one-node
    /// model). `None` if the manifest is somehow absent (scene facets are then
    /// skipped, not fatal).
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
        Self::drive_layout_strategy_for(&mut self.graph_runtimes, w, h);
    }

    /// Run the graph-scoped layout strategy while a particular pane has its
    /// view installed. The strategy sees selection only through that pane's
    /// temporary view state; it never chooses a global active pane.
    fn drive_layout_strategy_for(canvas: &mut mere::canvas::Canvas, w: u32, h: u32) {
        let Some(id) = canvas.layout_strategy().map(str::to_string) else {
            return;
        };
        canvas.refresh_community_cache(&id);
        let focus = canvas.focused_key();
        if canvas.needs_strategy_recompute(&id, w, h, focus) {
            // The host measures (per-node face footprints), the strategy
            // places — extent-aware spacing per the P2 contract.
            let extents = canvas.strategy_extents();
            let strategy = mere::canvas::project_canvas_strategy_with_score(
                &id,
                canvas.graph(),
                focus,
                w,
                h,
                canvas.community(),
                Some(&extents),
                // Recency reading pairs the Spiral's newest-first ordering
                // with the size-by-recency channel (P3).
                canvas.size_by_recency(),
            );
            canvas.apply_strategy_positions(&strategy.positions);
            canvas.set_projection_score(strategy.score);
            canvas.note_strategy_computed(&id, w, h, focus);
        }
    }

    /// Write the LIVE state into the facet store: the canvas arrangement as
    /// the `arrangement.*` family (positions are not graph truth, so the graph
    /// alone loses the layout; sizes / sprites / hulls / materials / faces
    /// ride the same store), the browser map as `web.*`, and the scene's own
    /// settings as `scene.*` on the container id. Other namespaces are
    /// untouched. Shared by the shell's save path and the fork's facet-carry
    /// (both need the store to reflect the moment, not the last save).
    pub fn refresh_facets(&mut self) {
        let geometry = self.graph_runtimes.cartography_geometry();
        let container = self.container_id();
        let facets = self.graph_runtimes.facets_mut();
        pandect::write_web_states(facets, &self.browser);
        pandect::write_arrangement_positions(facets, geometry.iter());
        pandect::write_arrangement_sizes(facets, geometry.size_iter());
        pandect::write_arrangement_sprites(facets, geometry.sprite_iter());
        pandect::write_arrangement_sprite_hulls(facets, geometry.sprite_hull_iter());
        pandect::write_arrangement_materials(facets, geometry.material_iter());
        pandect::write_arrangement_faces(facets, geometry.face_iter());
        if let Some(container) = container {
            let scene = pandect::SceneFacets {
                size_by_degree: geometry.size_by_degree(),
                size_by_importance: geometry.size_by_importance(),
                importance_metric: geometry.importance_metric().to_string(),
                physics_damping: self.physics_damping,
            };
            pandect::write_scene_facets(facets, container, &scene);
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

    fn publish_pane_context(&mut self, pane: PaneId, context: PaneContext) {
        let space = match self.space_of(pane) {
            Some(SpaceRef::Primary) => SpaceId::new("primary"),
            Some(SpaceRef::Lens(ordinal)) => SpaceId::new(format!("lens-{ordinal}")),
            None => return,
        };
        self.pane_context.place(pane, space);
        self.pane_context.publish(pane, context);
        self.pane_context.focus(pane);
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

    pub fn graph_pane_frame(
        &mut self,
        pane: PaneId,
        width: u32,
        height: u32,
    ) -> Option<(netrender::Scene, bool)> {
        self.with_graph_pane(pane, |canvas| {
            Self::drive_layout_strategy_for(canvas, width, height);
            canvas.frame(width, height)
        })
    }

    pub fn graph_pane_focused_member(&self, pane: PaneId) -> Option<uuid::Uuid> {
        self.graph_views.focused_member(pane).or_else(|| {
            self.graph_for_pane(pane)
                .and_then(|graph| self.graph_runtimes.canvas(graph))
                .and_then(mere::canvas::Canvas::focused_member)
        })
    }

    pub fn focused_graph_pane(&self) -> Option<PaneId> {
        match self.focus {
            FocusTarget::Graph(pane) => Some(pane),
            FocusTarget::Chrome | FocusTarget::Content(_) | FocusTarget::Pane(_) => None,
        }
    }

    /// The Forme owner for a Workbench action. A focused Workbench pane wins;
    /// otherwise the focused graph pane supplies its identity forme.
    pub fn workbench_owner_pane(&self) -> Option<PaneId> {
        self.active_pane
            .filter(|pane| self.pane_content(*pane) == Some(&PaneContent::Workbench))
            .or_else(|| self.focused_graph_pane())
    }

    pub fn graph_pane_node_at_screen(
        &mut self,
        pane: PaneId,
        x: f32,
        y: f32,
    ) -> Option<uuid::Uuid> {
        self.with_graph_pane(pane, |canvas| canvas.node_at_screen(x, y))
            .flatten()
    }

    pub fn graph_pane_select_member(&mut self, pane: PaneId, member: uuid::Uuid) -> bool {
        self.with_graph_pane(pane, |canvas| canvas.select_member(member))
            .unwrap_or(false)
    }

    pub fn graph_pane_cursor_moved(&mut self, pane: PaneId, x: f32, y: f32) -> bool {
        self.with_graph_pane(pane, |canvas| canvas.cursor_moved(x, y))
            .unwrap_or(false)
    }

    pub fn graph_pane_wheel(&mut self, pane: PaneId, dx: f32, dy: f32) -> bool {
        self.with_graph_pane(pane, |canvas| canvas.wheel(dx, dy))
            .unwrap_or(false)
    }

    pub fn graph_pane_pointer_down(
        &mut self,
        pane: PaneId,
        button: mere::canvas::PointerButton,
        x: f32,
        y: f32,
    ) -> bool {
        self.with_graph_pane(pane, |canvas| canvas.pointer_down(button, x, y))
            .unwrap_or(false)
    }

    pub fn graph_pane_pointer_up(
        &mut self,
        pane: PaneId,
        button: mere::canvas::PointerButton,
        x: f32,
        y: f32,
    ) -> bool {
        self.with_graph_pane(pane, |canvas| canvas.pointer_up(button, x, y))
            .unwrap_or(false)
    }

    /// Drain the semantic events emitted since the last call (the shell
    /// hands them to the scenario's log, diagnostics, or drops them).
    pub fn take_events(&mut self) -> Vec<AppEvent> {
        // The behavior drain reads this queue without consuming it (the shell
        // owns the drain), so its read cursor is an index into the vec and has
        // to go back to zero when the vec does. The ORDINAL must not: a watch
        // cursor is monotonic, so if event numbering restarted at every shell
        // drain, every later event would look older than what the watch had
        // already seen and nothing would ever wake again. `events_base` is the
        // running total that keeps the two in step.
        self.events_base += self.events.len() as u64;
        self.events_seen = 0;
        std::mem::take(&mut self.events)
    }

    /// The events the behavior drain has not considered yet.
    pub(crate) fn unseen_events(&self) -> &[AppEvent] {
        let seen = self.events_seen.min(self.events.len());
        &self.events[seen..]
    }

    /// Mark everything currently queued as considered.
    pub(crate) fn mark_events_seen(&mut self) {
        self.events_seen = self.events.len();
    }

    /// How many events are queued, for attributing the ones a body produces.
    pub(crate) fn events_len(&self) -> usize {
        self.events.len()
    }

    /// The ordinal of the queue's first entry: events already taken by the
    /// shell. Added to an index to get a number that only ever grows.
    pub(crate) fn events_base(&self) -> u64 {
        self.events_base
    }

    /// Seed a new lens window's pane space: a lone Orrery leaf with a freshly
    /// minted pane id (globally unique across every window's tree, so surface
    /// keys and the active-pane anchor never collide). Returns its ordinal.
    fn seed_lens_space(&mut self) -> usize {
        let pane_id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        let ordinal = self.lenses.len();
        self.lenses.push(Some(FrisketLayout {
            id: crate::panes::FrisketId::new(format!("lens-{ordinal}")),
            label: format!("lens {ordinal}"),
            root: PaneNode::Leaf {
                pane_id,
                content: PaneContent::Orrery,
                graph_id: self.graph_runtimes.active_graph(),
            },
        }));
        self.lens_blueprints.push(None);
        ordinal
    }

    /// The live blueprint for a window space, after an A5 operation has
    /// promoted that space out of legacy Frisket presentation.
    pub(crate) fn blueprint_space(&self, space: SpaceRef) -> Option<&SpaceBlueprint> {
        match space {
            SpaceRef::Primary => self.primary_blueprint.as_ref(),
            SpaceRef::Lens(ordinal) => self.lens_blueprints.get(ordinal).and_then(Option::as_ref),
        }
    }

    fn blueprint_space_mut(&mut self, space: SpaceRef) -> Option<&mut SpaceBlueprint> {
        match space {
            SpaceRef::Primary => self.primary_blueprint.as_mut(),
            SpaceRef::Lens(ordinal) => self
                .lens_blueprints
                .get_mut(ordinal)
                .and_then(Option::as_mut),
        }
    }

    fn take_blueprint_space(&mut self, space: SpaceRef) -> Option<SpaceBlueprint> {
        match space {
            SpaceRef::Primary => self.primary_blueprint.take(),
            SpaceRef::Lens(ordinal) => self.lens_blueprints.get_mut(ordinal).and_then(Option::take),
        }
    }

    fn restore_blueprint_space(&mut self, space: SpaceRef, blueprint: SpaceBlueprint) {
        match space {
            SpaceRef::Primary => self.primary_blueprint = Some(blueprint),
            SpaceRef::Lens(ordinal) => {
                while self.lens_blueprints.len() <= ordinal {
                    self.lens_blueprints.push(None);
                }
                self.lens_blueprints[ordinal] = Some(blueprint);
            }
        }
    }

    /// Transfer a floating station between two live blueprint spaces. The
    /// legacy trees move their payloads separately; this preserves the float
    /// rectangle and z-scoped station that the compositor is actually using.
    pub(crate) fn transfer_floating_blueprint(
        &mut self,
        source: SpaceRef,
        destination: SpaceRef,
        pane: PaneId,
    ) -> bool {
        if source == destination || self.ensure_blueprint_space(destination).is_none() {
            return false;
        }
        let (Some(mut source_blueprint), Some(mut destination_blueprint)) = (
            self.take_blueprint_space(source),
            self.take_blueprint_space(destination),
        ) else {
            return false;
        };
        let moved = source_blueprint
            .tear_out_floating_pane(pane, &mut destination_blueprint)
            .is_ok();
        self.restore_blueprint_space(source, source_blueprint);
        self.restore_blueprint_space(destination, destination_blueprint);
        moved
    }

    /// Start using the blueprint projection for this space. The conversion is
    /// delayed until an A5 station gesture occurs so ordinary Frisket-only
    /// sessions retain their established persistence behavior.
    pub(crate) fn ensure_blueprint_space(
        &mut self,
        space: SpaceRef,
    ) -> Option<&mut SpaceBlueprint> {
        match space {
            SpaceRef::Primary => {
                if self.primary_blueprint.is_none() {
                    self.primary_blueprint =
                        Some(crate::panes::blueprint_from_frisket(&self.frisket));
                }
                self.primary_blueprint.as_mut()
            }
            SpaceRef::Lens(ordinal) => {
                let legacy = self.lenses.get(ordinal)?.as_ref()?;
                while self.lens_blueprints.len() <= ordinal {
                    self.lens_blueprints.push(None);
                }
                if self.lens_blueprints[ordinal].is_none() {
                    self.lens_blueprints[ordinal] =
                        Some(crate::panes::blueprint_from_frisket(legacy));
                }
                self.lens_blueprints[ordinal].as_mut()
            }
        }
    }

    /// A pointer press promotes the clicked float above its siblings. Tiled
    /// panes deliberately do not instantiate a blueprint by being clicked.
    pub(crate) fn raise_floating_pane(&mut self, pane: PaneId) -> bool {
        self.space_of(pane)
            .and_then(|space| self.blueprint_space_mut(space))
            .is_some_and(|space| space.raise_float(pane))
    }

    /// Choose the newest live lens that is not `exclude`, spawning a lens when
    /// none qualifies. The ordinal is returned separately so A5 can prepare
    /// the destination's blueprint before a legacy leaf is moved there.
    fn target_lens(&mut self, exclude: Option<SpaceRef>) -> (usize, Vec<Effect>) {
        let mut effects = Vec::new();
        let target = self
            .lenses
            .iter()
            .enumerate()
            .rev()
            .find(|(i, s)| s.is_some() && exclude != Some(SpaceRef::Lens(*i)))
            .map(|(i, _)| i);
        let ordinal = match target {
            Some(ordinal) => ordinal,
            None => {
                let ordinal = self.seed_lens_space();
                self.events.push(AppEvent::WindowOpened);
                effects.push(Effect::OpenWindow { ordinal });
                ordinal
            }
        };
        (ordinal, effects)
    }

    /// Land a legacy leaf in one chosen lens, beside that lens's final leaf.
    fn land_leaf_at_lens(&mut self, leaf: PaneNode, ordinal: usize) {
        if let Some(Some(lens)) = self.lenses.get_mut(ordinal) {
            let anchor_path = lens
                .iter_leaves()
                .last()
                .map(|(id, _, _)| id)
                .and_then(|id| crate::pane::path_of(lens, id))
                .unwrap_or_default();
            lens.summon_leaf(&anchor_path, InsertSide::Right, leaf);
        }
    }

    /// Land `leaf` in the newest live lens that is not `exclude` (a tear-out
    /// must LEAVE its source window), spawning a lens when none qualifies.
    /// Returns the effects (an `OpenWindow` when a lens spawned).
    fn land_leaf_in_lens(&mut self, leaf: PaneNode, exclude: Option<SpaceRef>) -> Vec<Effect> {
        let (ordinal, effects) = self.target_lens(exclude);
        self.land_leaf_at_lens(leaf, ordinal);
        effects
    }

    /// The space holding `pane`: the primary tree, else the live lens whose
    /// tree carries it. Pane ids are minted from one counter, so the answer is
    /// unique — this is how a pane-anchored op (close, divider, summon-beside,
    /// tear-out) finds which window's tree to mutate.
    pub fn space_of(&self, pane: PaneId) -> Option<SpaceRef> {
        if self.frisket.iter_leaves().any(|(id, _, _)| id == pane) {
            return Some(SpaceRef::Primary);
        }
        self.lenses.iter().enumerate().find_map(|(i, s)| {
            s.as_ref()
                .filter(|space| space.iter_leaves().any(|(id, _, _)| id == pane))
                .map(|_| SpaceRef::Lens(i))
        })
    }

    /// The layout a [`SpaceRef`] names, when it is live.
    pub fn space(&self, space: SpaceRef) -> Option<&FrisketLayout> {
        match space {
            SpaceRef::Primary => Some(&self.frisket),
            SpaceRef::Lens(i) => self.lenses.get(i).and_then(Option::as_ref),
        }
    }

    /// Mutable [`Self::space`].
    fn space_mut(&mut self, space: SpaceRef) -> Option<&mut FrisketLayout> {
        match space {
            SpaceRef::Primary => Some(&mut self.frisket),
            SpaceRef::Lens(i) => self.lenses.get_mut(i).and_then(Option::as_mut),
        }
    }

    /// Note a semantic event from outside `update` — the shell's own divergence
    /// (an interaction that missed, an affordance not yet wired) joins the same
    /// drained stream the update path feeds, so automation reads one channel.
    pub fn note(&mut self, event: AppEvent) {
        self.events.push(event);
    }

    /// Consume one app intent. Never blocks; anything slow leaves as an effect.
    /// Write a note's body, minting the node if the address is new.
    ///
    /// Goes through `visit` and the kernel's body delta, so the write is
    /// journaled and attributed exactly like any other: a summary a behavior
    /// wrote reads back in the node's history under that behavior, not under
    /// the user.
    fn write_note(&mut self, url: String, body: String) -> Vec<Effect> {
        let key = self.graph_runtimes.visit(&url);
        let Some(member) = self
            .graph_runtimes
            .graph()
            .get_node(key)
            .map(|node| node.id)
        else {
            return vec![Effect::Redraw];
        };
        let _ = self.graph_runtimes.set_node_body_for(member, Some(body));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    /// Record a semantic event from outside the app module (the behavior
    /// drain). `events` stays private: the drain is the only outside writer,
    /// and naming it here keeps that true.
    pub(crate) fn record_event(&mut self, event: crate::observe::AppEvent) {
        self.events.push(event);
    }

    /// Apply an action, then run whatever behaviors its commits woke.
    ///
    /// The cascade runs *after* the action's own effects are decided, so a
    /// woken body sees the world the action left rather than the one it found,
    /// and its own effects join the same return.
    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        let mut effects = self.dispatch(action);
        effects.extend(crate::behaviors::drain(self));
        effects
    }

    fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::OpenAddress(url) => self.open_address(url),
            Action::ComposeFocusedSmolwebSubmission => self.compose_focused_smolweb_submission(),
            Action::BeginSmolwebSubmission { source, target } => {
                self.begin_smolweb_submission(source, target)
            }
            Action::SmolwebSubmissionFile {
                name,
                bytes,
                suggested_mime,
            } => self.set_smolweb_submission_file(name, bytes, suggested_mime),
            Action::ContentNavigationCommitted { member, url } => {
                self.commit_content_navigation(member, url)
            }
            Action::ContentTitleChanged { member, title } => self.set_content_title(member, title),
            Action::OpenDocumentFind => self.open_document_find(),
            Action::CloseDocumentFind => self.close_document_find(),
            Action::InsertDocumentFind(text) => self.insert_document_find(text),
            Action::BackspaceDocumentFind => self.backspace_document_find(),
            Action::StepDocumentFind(direction) => self.step_document_find(direction),
            Action::NavBack => self.nav_back(),
            Action::NavForward => self.nav_forward(),
            Action::Reload => self.reload_focused(),
            Action::Stop => self.stop_focused(),
            Action::KeepNode { member } => self.keep_node(member),
            Action::SubscribeFocusedFeed { period } => self.subscribe_focused_feed(period),
            Action::UnsubscribeFocusedFeed => self.unsubscribe_focused_feed(),
            Action::RefreshFeeds => self.refresh_feeds(),
            Action::MarkFocusedFeedEntryRead => self.mark_focused_feed_entry_read(),
            Action::ReseedLayout => self.reseed_layout(),
            Action::SetLayoutStrategy(id) => self.set_layout_strategy(id),
            Action::ToggleIsometric => {
                let on = !self.graph_runtimes.is_isometric();
                self.graph_runtimes.set_isometric(on);
                vec![Effect::Redraw]
            }
            Action::OrbitBy(delta) => {
                self.graph_runtimes.orbit_by(delta);
                vec![Effect::Redraw]
            }
            Action::TiltBy(delta) => {
                let tilt = self.graph_runtimes.tilt();
                self.graph_runtimes.set_tilt(tilt + delta);
                vec![Effect::Redraw]
            }
            Action::ToggleHeightByDegree => {
                let on = !self.graph_runtimes.height_by_degree();
                self.graph_runtimes.set_height_by_degree(on);
                vec![Effect::Redraw]
            }
            Action::FitView => {
                self.graph_runtimes.fit_to_content();
                vec![Effect::Redraw]
            }
            Action::TogglePhysics => {
                self.graph_runtimes.toggle_physics_paused();
                vec![Effect::Redraw]
            }
            Action::ToggleSizeByRecency => self.toggle_size_by_recency(),
            Action::SaveSession => vec![Effect::SaveSession],
            Action::JoinPlace(invite) => self.join_place(invite),
            Action::SendPlaceMessage { channel, body } => {
                self.run_place_command(crate::place::worker::PlaceCommand::SendMessage {
                    channel,
                    body,
                })
            }
            Action::ShareFocusedNode => match self.focused_address() {
                Some(address) => self
                    .run_place_command(crate::place::worker::PlaceCommand::ShareNode { address }),
                None => {
                    self.events
                        .push(AppEvent::PlaceRefused("no focused node to share".into()));
                    vec![Effect::Redraw]
                }
            },
            Action::ResyncPlace => self.resync_place(),
            // Multi-session (rung 6's second half). Both lower to the shell's
            // SwitchSession effect: the PORT saves the departing session and
            // tears down its live handles before the app adopts the target —
            // state here, ports there, ordering correct.
            Action::NewSession => {
                let id = Self::mint_session(&self.data_root, &mut self.sessions);
                vec![Effect::SwitchSession { id }]
            }
            Action::SwitchSession(id) => {
                if id == self.session_id || self.sessions.get(id).is_none() {
                    return vec![Effect::Redraw];
                }
                vec![Effect::SwitchSession { id }]
            }
            // ---- Denizen residency (participant gate B1) ----
            Action::InstallDenizen { path } => self.install_denizen(path),
            Action::ConfirmInstallDenizen => self.confirm_install_denizen(),
            Action::CancelInstallDenizen => {
                if self.pending_install.take().is_some() {
                    self.events
                        .push(AppEvent::DenizenRefused("cancelled".into()));
                }
                self.omnibar = OmnibarState::default();
                self.shell.close_omnibar();
                vec![Effect::Redraw]
            }
            Action::UninstallDenizen { member } => self.uninstall_denizen(member),
            Action::RunDenizen { member } => self.run_denizen(member),
            Action::WriteNote { url, body } => self.write_note(url, body),
            Action::RecoverSession(id) => self.recover_session(id),
            Action::CloseSession => self.close_session(),
            Action::BeginRenameSession => self.begin_rename_session(),
            Action::RenameSession { id, name } => self.rename_session(id, name),
            Action::DeleteFocusedNode => self.delete_focused_node(),
            Action::RecoverDeletedNode(id) => self.recover_deleted_node(id),
            Action::EmptyRecycleBin => self.empty_recycle_bin(),
            Action::NewWindow => self.new_window(),
            Action::FloatActivePane => self.float_active_pane(),
            Action::DockActivePane => self.dock_active_pane(),
            // The tear-out trichotomy's LEAF arm: the active pane's frisket
            // leaf leaves this window's tree and joins the newest lens's
            // (spawning one when none is open). The pane's retained runner is
            // untouched — in the surface-compositor shape, identity across
            // windows is a property of the RUNNER staying put while the leaf
            // changes trees, which is exactly what the forest dom exists to
            // buy the one-shared-DOM shape.
            Action::TearOutActivePane => self.tear_out_active_pane(),
            Action::ReturnActivePaneToPrimary => self.return_active_pane_to_primary(),
            // The trichotomy's BRANCH arm, gesture-first: a workbench tab
            // dragged out of the pane. The tile leaves platen's tiling and
            // becomes a pinned Tile pane in a lens window; its live session
            // (if any) composites there as the pane's content surface.
            Action::TearOutTile { member } => self.tear_out_tile(member),
            // The trichotomy's FORK arm: snapshot the component into a fresh
            // session and switch to it (G4-R R2; the shell saves the donor on
            // the way out, as every switch does).
            Action::ForkNode { member } => self.fork_session_from(member),
            Action::ForkFocusedNode => match self.graph_runtimes.focused_member() {
                Some(member) => self.fork_session_from(member),
                None => Vec::new(),
            },
            Action::SetViewerOverride { member, viewer } => {
                self.set_viewer_override(member, viewer)
            }
            Action::SetNodeSprite {
                member,
                data_uri,
                hull,
            } => self.set_node_sprite(member, data_uri, hull),
            Action::ToggleNodeContent => self.toggle_node_content(),
            Action::OmnibarOpen { command } => self.open_omnibar(command),
            Action::OmnibarClose => self.close_omnibar(),
            Action::OmnibarChar(c) => self.omnibar_char(c),
            Action::OmnibarInsert(s) => self.omnibar_insert(s),
            Action::OmnibarBackspace => self.omnibar_backspace(),
            Action::OmnibarDelete => self.omnibar_delete(),
            Action::OmnibarCaret(m) => {
                // Caret motion never changes the text, so the suggestion
                // list (and the highlight) stays put.
                self.omnibar.move_caret(m);
                vec![Effect::Redraw]
            }
            Action::OmnibarMove(delta) => self.omnibar_move(delta),
            Action::OmnibarCommitRow(index) => self.omnibar_commit_row(index),
            Action::OmnibarCommit => self.commit_omnibar(),
            Action::RepeatShellEntry(id) => self.repeat_shell_entry(id),
            Action::OpenShellEntryTarget(id) => self.open_shell_entry_target(id),
            // Pane tree ops (rung 5 slice C). Each mutates the frisket layout and
            // persists it (SaveSession writes frame.json), so the arrangement
            // survives a restart. Maximize is view state, not persisted.
            Action::SummonPane(kind) => self.summon_pane(kind),
            Action::ChooseKnotDocumentFile { read_only } => {
                vec![Effect::ChooseKnotDocumentFile { read_only }]
            }
            Action::SummonContributedPane { kind, source } => {
                self.summon_contributed_pane(kind, source)
            }
            Action::CloseActivePane => self.close_active_pane(),
            Action::SetSplitRatio { space, path, ratio } => {
                if let Some(layout) = self.space_mut(space) {
                    layout.set_split_ratio(&path, ratio);
                }
                vec![Effect::Redraw]
            }
            Action::SetActivePaneDivider(ratio) => self.set_active_pane_divider(ratio),
            Action::ToggleMaximizePane => self.toggle_maximize_pane(),
            Action::TogglePaneSection { pane, section } => self.toggle_pane_section(pane, section),
            Action::MovePaneSection {
                pane,
                section,
                delta,
            } => self.move_pane_section(pane, section, delta),
            // Workbench ops (rung 5 slice E). Platen owns the model and every
            // mutator; these arms lower intents onto it and persist. The
            // Workbench PANE (the frisket leaf) is where the tiling shows;
            // opening a tile summons it if absent, through the same summon
            // path as a palette summon (one spine, no side door).
            Action::OpenInWorkbench => self.open_in_workbench(),
            Action::WorkbenchActivate(member) => {
                let owner = self.workbench_owner_pane();
                let activated = owner
                    .and_then(|pane| self.workbench_for_pane_mut(pane))
                    .is_some_and(|workbench| workbench.activate(member));
                if activated {
                    if let Some(owner) = owner {
                        self.publish_member_context(owner, Some(member));
                    }
                    vec![Effect::SaveSession, Effect::Redraw]
                } else {
                    vec![Effect::Redraw]
                }
            }
            Action::CloseWorkbenchTile => self.close_workbench_tile(),
            Action::WorkbenchStackOnto { dragged, target } => {
                self.workbench_stack_onto(dragged, target)
            }
            Action::WorkbenchSplitBeside {
                dragged,
                target,
                axis,
                after,
            } => self.workbench_split_beside(dragged, target, axis, after),
            Action::WorkbenchSplitOut {
                dragged,
                axis,
                after,
            } => self.workbench_split_out(dragged, axis, after),
            Action::WorkbenchSetFractions { path, fractions } => {
                if let Some(owner) = self.workbench_owner_pane()
                    && let Some(workbench) = self.workbench_for_pane_mut(owner)
                {
                    workbench.set_split_fractions(&path, &fractions);
                }
                vec![Effect::Redraw]
            }
        }
    }
}

mod denizen_arms;
mod feed_arms;
mod fixtures;
mod node_arms;
mod omnibar_arms;
mod palette;
mod pane_arms;
mod session_lifecycle;
mod updates;

#[cfg(test)]
mod tests;
