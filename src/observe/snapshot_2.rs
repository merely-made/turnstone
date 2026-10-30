// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One coherent read of the application's observable state.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Current-session capture acknowledgement and recovery facts.
    pub source_captures: Vec<SourceCaptureView>,
    pub graph_scene: crate::graph_scenes::Facts,
    /// Same last-refresh facts as the place status surface, without its filter.
    pub place_status: Vec<String>,
    /// Machine-readable place facts, when this session is in one. Ids and
    /// counts a second peer can be compared against; not a liveness claim.
    pub place: Option<PlaceFacts>,
    /// The focused node, when exactly one is selected.
    pub focused: Option<FocusedNode>,
    pub omnibar: OmnibarView,
    /// Find in the captured document, when its field is open.
    pub document_find: Option<DocumentFindView>,
    /// Focused document-control availability, mirrored from its owning engine.
    pub document_capabilities: Option<DocumentCapabilitiesView>,
    /// Host engine availability, distinct from a running page's capabilities.
    pub engine_inventory: Vec<crate::content::EngineDescriptor>,
    /// The same choices and refusals the focused document's viewer presents.
    pub viewer_choices: Vec<crate::inspector_controls::ViewerOption>,
    /// The active held web permission or authentication decision. Credential
    /// text is intentionally absent; only public request identity and UI state
    /// are observable.
    pub user_agent_decision: Option<UserAgentDecisionView>,
    /// Per-node content lifecycle, as (member, state label) pairs.
    pub content: Vec<(Uuid, String)>,
    pub node_count: usize,
    /// The layout's kinetic energy and the node pairs closer than a body's
    /// diameter — a physics law's signature, the same two figures the web
    /// host publishes. (Physics catalog — P4 native.)
    pub physics_energy: f32,
    /// Whether the layout physics is paused — a paused canvas refuses every
    /// settle, so an add never separates. (Physics catalog — P4 native.)
    pub physics_paused: bool,
    pub layout_overlaps: usize,
    /// Durable feed sources in this session and their unread entry count.
    pub feed_subscriptions: usize,
    pub feed_unread: usize,
    /// Whether at least one node lies inside the viewport.
    pub graph_visible: bool,
    /// The composited surfaces present this frame, as kind labels in z-order
    /// (rung 5 slice A). Derived from app truth: canvas is always present,
    /// content when the focused node is live, chrome when it has content.
    /// The window size lives in the shell, so this is the surface LIST, not
    /// pixel rects.
    pub surfaces: Vec<String>,
    /// Which surface holds semantic input, as a label ("canvas" / "chrome" /
    /// "content").
    pub focus: String,
    /// The panes in the frisket tree, as `PaneContent` tags (rung 5 slice C).
    /// A single-pane layout reads `["orrery"]`; summoning a Roster adds
    /// `"roster"`. The active pane, if any, is `active_pane`.
    pub panes: Vec<String>,
    /// The active pane's tag, or `None` when the canvas (Orrery) is active.
    pub active_pane: Option<String>,
    /// Visible A5 floating stations in the primary window, as pane tags in
    /// bottom-to-top order. Empty until a space enters the float layer.
    pub floating_panes: Vec<String>,
    /// Whether a pane is maximized.
    pub maximized: bool,
    /// When a Trail pane is in the tree, its row texts (rung 5 slice D), so a
    /// scenario can assert a row's content. Empty when no Trail pane is open.
    pub trail_rows: Vec<String>,
    /// When a Roster pane is in the tree, its row texts (the node manifest).
    pub roster_rows: Vec<String>,
    /// When an Inspector pane is in the tree, its rows as "Key: value" lines
    /// (node facts + content facts off the spawn-time mirror).
    pub inspector_rows: Vec<String>,
    /// When an Arrange pane is in the tree, its rows: the arrangement, the
    /// physics law, overlays, sources and profile as plain text.
    pub arrange_rows: Vec<String>,
    /// The Roster's active tab label, mirrored out of the strip's own state.
    pub roster_tab: &'static str,
    /// The root split's ratio, when the pane tree is split at all. The divider
    /// receipts assert against this after a drag.
    pub split_ratio: Option<f32>,
    /// The workbench's cells (rung 5 slice E), one string per cell: the tab
    /// labels joined by `|` with `*` marking the active tab. Empty when the
    /// workbench holds no tiles.
    pub workbench_cells: Vec<String>,
    /// The workbench's ROOT split fractions (empty for a lone cell). The
    /// workbench-divider receipts assert against these after a drag.
    pub workbench_fractions: Vec<f32>,
    /// The accessibility projection as "role: label" lines (rung 5 slice F):
    /// the stitched application tree, flattened, so a scenario can assert a
    /// node the same way a screen reader would announce it.
    pub a11y: Vec<String>,
    /// Whether the focused node's content lineage can step back / forward.
    pub can_back: bool,
    pub can_forward: bool,
    /// Focused-node browser fetch phase, formatted for scenario receipts.
    pub browser_fetch: Option<String>,
    /// The retained content link currently presented as a prospective node.
    pub link_preview: Option<String>,
    /// Every action offered right now, by label, in the palette's own order
    /// (contextual rows first). The snapshot's last promised member: an
    /// automation lane asks what it may do, and gets the same list a person
    /// sees, resolvable by the same label through `Automatable::act`.
    pub available_actions: Vec<String>,
    /// How many windows are open (rung 7; mirrored from the shell).
    pub windows: usize,
    /// Each live lens window's panes, as "ordinal:tag" strings (rung 7 depth:
    /// windows are pane hosts). Empty when no lens is open.
    pub lens_panes: Vec<String>,
    /// Visible floating stations in lens windows, as "ordinal:tag" strings.
    pub lens_floating_panes: Vec<String>,
    /// The ACTIVE pane's parent-split ratio, in whichever space (primary or
    /// lens) holds the pane — the honest readback of the divider op, wherever
    /// it lands. `None` with no active pane or an unsplit tree.
    pub active_ratio: Option<f32>,
    /// The live session's display label (rung 6's second half).
    pub session: String,
    /// How many sessions the manifest set holds.
    pub session_count: usize,
}
