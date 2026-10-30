// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's core state and the two update functions — the middle of the
//! spine: `Action -> update -> Effects` and `Update -> apply_update ->
//! Effects`. Holds data, never handles: the ports (actors, stores, the
//! window) live in the shell, which runs the effects this module returns.

use std::collections::HashMap;
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
mod research_clip_arms;
pub(crate) mod research_search_state;
mod research_search_arms;
mod research_compare_arms;
mod research_compare_rows;
mod research_search_rows;
mod research_search_values;
#[cfg(test)]
mod research_search_tests;
mod document_find_arms;
mod fold_arms;
mod journal_origin;
#[cfg(test)]
mod journal_origin_tests;
mod physics_arms;
#[cfg(test)]
mod r1_source_keep_tests;
#[cfg(test)]
mod r2_saved_source_tests;
pub(crate) mod research_note_arms;
#[cfg(test)]
mod resident_admission_tests;
#[cfg(test)]
mod resident_run_tests;
mod runtime_pool;
mod saved_source_arms;
mod scene_arms;
#[cfg(test)]
mod scene_tests;
mod source_capture_arms;
#[cfg(test)]
mod source_capture_persistence_tests;
mod user_agent_decision_arms;

pub use runtime_pool::{
    FormeRuntime, FormeRuntimeKey, FormeRuntimePool, GraphPaneViews, GraphRuntime, GraphRuntimePool,
};

/// The window size a fresh App assumes until the shell resizes it: an
/// ordinary desktop window, so a headless projection (tests, the remote lens)
/// offers the same rows a real one would rather than collapsing to the floor.
pub const DEFAULT_VIEWPORT: (f32, f32) = (1280.0, 800.0);

/// The page scale a node with no stored request sits at. A sidecar `None` and
/// a stored `1.0` mean the same thing; only `None` survives the save path's
/// empty-entry pruning, so reset clears rather than writes.
pub const PAGE_ZOOM_DEFAULT: f32 = 1.0;

/// The application state: hosted graph runtimes, chrome state, and session
/// persistence. A graph pane resolves one runtime; the application itself
/// does not own one singleton graph surface.
/// Where a pending place answer will be written, and which Moot it is about.
///
/// The worker answers with material, never with a file: which path a person
/// chose is app state, so the answer is matched to it here rather than
/// round-tripped through a command that has no business knowing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingPlaceArtifact {
    pub generation: u64,
    pub path: String,
    pub moot: crate::place::PlaceId,
}

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
    /// The person's command menu: kept, dropped and recent commands, read
    /// through Cambium's command set (Scenograph editor plan SE28 to SE32).
    /// Saved in the session's view sidecar, never in graph truth.
    pub reading: crate::reading_view::ReadingState,
    pub(crate) command_choices: cambium::CommandChoices,
    /// Find in the captured active document. This is separate from the
    /// omnibar's graph recall and command lanes.
    pub document_find: crate::document_find::DocumentFindState,
    /// Retained visible decisions for held web permission and authentication
    /// callbacks. The shell retains the callbacks and policy authority; this
    /// state is the controlled UI mirror and contains no durable credentials.
    pub user_agent_decision: crate::user_agent_decision::UserAgentDecisionState,
    /// Per-frame stage costs for the chrome path. Data only, like the rest of
    /// App: the shell writes its own stages into it during `render` and clears
    /// it once the frame is reported. It lives here rather than on Shell
    /// because suggestion recomputation — the first stage the palette open-lag
    /// note names — happens on App's input edge, outside any frame.
    pub frame_timings: crate::frame_timing::FrameTimings,
    /// The Map's resolved theme, kept until the appearance changes.
    pub(crate) map_theme: crate::map_theme::MapThemeCache,
    /// Read-only diagnostic view, refreshed by its Shell owner. This is neither
    /// the observation store nor durable graph/session data.
    pub diagnostic_inspection: Option<Result<apparatus_core::Inspection, String>>,
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
    /// Correlated capture progress; recovery inventory is never corpus admission.
    pub source_captures: HashMap<uuid::Uuid, crate::source_capture::CaptureAttempt>,
    pub(crate) source_capture_generation: uuid::Uuid,
    pub(crate) saved_sources: saved_source_arms::SavedSourcesState,
    pub(crate) research: crate::research::Bridge,
    pub(crate) research_search: research_search_state::State,
    pub(crate) saved_readers: HashMap<uuid::Uuid, saved_source_arms::SavedReader>,
    pub(crate) research_clip_readers: HashMap<uuid::Uuid, crate::research::ClipReader>,
    /// Transient host engine availability; retained across session switches.
    pub engine_inventory: Vec<crate::content::EngineDescriptor>,
    /// Durable feed schedules and duplicate-suppression state for this
    /// session. Entry nodes and unread markers themselves remain graph truth.
    pub feeds: crate::feed::FeedSubscriptions,
    /// The ONE Redshank authority: the listening model, its durable store
    /// under this session's directory, and the single audio runtime every
    /// episode tile shares. The tiles themselves are Redshank's own compact
    /// dock, admitted through the contributed-surface registry.
    pub redshank: crate::redshank_host::RedshankHost,
    /// Which graph member each open episode projects onto. Derived state: a
    /// restart rebuilds it from the members' own progress fields.
    pub redshank_members: std::collections::BTreeMap<redshank_model::ItemId, uuid::Uuid>,
    /// This session's public shared-place binding and product-visible status.
    /// Domain stores, joined lanes, transport handles, and key state belong to
    /// the shell-owned place worker; this field remains data only.
    pub place: crate::place::PlaceState,
    /// Monotonic identity for place-worker opens. It is never reset on a
    /// session switch, so a late answer cannot alias a later visit.
    pub(crate) next_place_generation: u64,
    /// Where a pending place answer will be written, and what it is about.
    /// Set when a founding-vocabulary prompt commits, consumed by the answer.
    pub(crate) pending_place_artifact: Option<PendingPlaceArtifact>,
    /// Monotonic id for authored place commands, so a late answer is
    /// attributable to the command that asked for it.
    pub(crate) next_place_request: u64,
    /// Correlation id for explicit smolweb writes. Never reused within the
    /// process, so a late receipt cannot attach to a later composer.
    pub(crate) next_smolweb_submission: u64,
    pub(crate) active_smolweb_submission: Option<u64>,
    /// Native Micron submissions additionally bind their receipt to the exact
    /// source member/address that the author reviewed.
    pub(crate) micron_submission_sources: HashMap<u64, (uuid::Uuid, String, String)>,
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
    /// Latest typed refusal for one graph's physics controls, not persisted.
    physics_refusal: Option<(GraphId, mere::canvas::OverlayRefusal)>,
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
    /// Legacy live station authority, imported transactionally into Workspace.
    /// Retained only when an unsupported or invalid legacy shape refuses import.
    pub(crate) primary_blueprint: Option<SpaceBlueprint>,
    /// Matching legacy import state for lens spaces. Tombstones mirror
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
    /// A staged participant install awaiting its visible grant review (B1).
    pub pending_install: Option<crate::denizen::PendingInstall>,
    /// The session's participant runtime: residents, derived authority, the gate.
    pub denizens: crate::denizen::Denizens,
    /// Host-owned run receipts for the adopted session. This stays live across
    /// nested lowering; reopening/recovery happens only in `adopt_session`.
    pub resident_runs: crate::resident_runs::ResidentRuns,
    /// A malformed or uncommitted run sidecar is authority-relevant: starting
    /// another run could duplicate an unresolved operation, so refuse it.
    pub resident_run_error: Option<String>,
    /// Default keeps existing shell handoffs usable while recording only a
    /// handoff disposition. Strict mode holds a resident for reconciliation.
    pub resident_run_effect_policy: crate::resident_runs::ExternalEffectPolicy,
    /// Host-configurable limits for one synchronous resident invocation.
    pub resident_run_limits: servitor::RunLimits,
    /// Host storage policy, retained across session adoption and applied to reads.
    pub resident_run_storage_limits: crate::resident_runs::StorageLimits,
    /// The profile's root identity: whose authority every participant grant
    /// descends from (capability-model OQ2): the persona djinn holds,
    /// reached over the custody route. `None` while it is pending (djinn
    /// absent or Locked, dramatis D12): nothing that signs or seals runs, and
    /// there is no fallback key, until [`App::adopt_profile_root`]. Install
    /// signs a delegation with it inside djinn; uninstall revokes that
    /// delegation.
    pub identity: Option<std::sync::Arc<crate::identity::RootIdentity>>,
    /// Capsule approvals for the active Personae root. The sidecar contains
    /// origin mappings only; client certificate keys are derived on demand.
    pub gemini_identities: crate::gemini_identity::GeminiIdentityBindings,
    /// Process-local, ordered captures with graph/session/generation origin.
    pub journal: crate::host_journal::SharedJournal,
    pub(crate) behavior_binding: Option<crate::host_journal::RuntimeOrigin>,
    pub(crate) behavior_refusal: Option<String>,
    /// Standing subscriptions: which participant wakes on what (graph behaviors
    /// W0/W1). Empty until a behavior is installed, which is why the drain
    /// costs nothing on a session that has none.
    pub watches: servitor::WatchTable,
    /// The app tier's own table (W3). A separate table because a watch cursor
    /// is a position in ONE journal, and these two count different things: a
    /// host graph-capture ordinal and an app-event ordinal. Sharing a table would

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
    /// What the shell's live Knot document surfaces currently hold, mirrored
    /// here so observation reads one place. The shell owns the sessions and
    /// refreshes this on every drain; the app never derives it.
    knot_documents: Vec<crate::observe::KnotDocumentFacts>,
}

mod denizen_arms;
mod feed_arms;
mod fixtures;
mod node_arms;

mod command_invocation;
mod omnibar_arms;
mod outer_workspace_arms;
mod palette;
mod pane_arms;
mod redshank_arms;
mod session_lifecycle;
#[cfg(test)]
mod session_persistence_tests;
#[cfg(test)]
mod surface_control_tests;
mod updates;
mod workspace_stations;

#[cfg(test)]
mod tests;

#[path = "page_zoom_percent.rs"]
mod page_zoom_percent;
pub use page_zoom_percent::page_zoom_percent;
#[path = "page_zoom_display.rs"]
mod page_zoom_display;
pub use page_zoom_display::page_zoom_display;
#[path = "canonical_viewer_override.rs"]
mod canonical_viewer_override;
use canonical_viewer_override::canonical_viewer_override;
#[path = "migrate_retired_viewer_overrides.rs"]
mod migrate_retired_viewer_overrides;
use migrate_retired_viewer_overrides::migrate_retired_viewer_overrides;
#[path = "focused_caption.rs"]
mod focused_caption;
pub use focused_caption::focused_caption;
#[path = "navigation_caption.rs"]
mod navigation_caption;
pub(crate) use navigation_caption::navigation_caption;
#[path = "pane_label.rs"]
mod pane_label;
use pane_label::pane_label;
#[path = "app_graph_pane_frame.rs"]
mod app_graph_pane_frame;
#[path = "app_shell_chrome_config.rs"]
mod app_shell_chrome_config;
#[path = "app_write_note.rs"]
mod app_write_note;
