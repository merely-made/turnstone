//! The observation surface: one snapshot + one event stream over app truth
//! (the architecture plan's recorded snapshot/events pair, landed at its
//! trigger — the scenario lane is the first automation consumer, and its
//! asserts read THIS surface instead of poking app fields one by one). The
//! same surface is what the a11y projection, diagnostics, and the
//! session-engines plan's automation story consume later: observation is
//! the vocabulary's other half, so it lives beside `action`, app-owned and
//! port-agnostic.
//!
//! Scope note: events are emitted where Actions and Updates fold — the
//! semantic tier. Continuous gestures bypass Action by the gesture law, so
//! a gesture-end semantic change (click-selection, drag-placement) does not
//! yet emit; that arrives with the gesture-end events the law already
//! promises, not by teaching this module about pointers.

use uuid::Uuid;

use crate::app::App;
use crate::content::NodeContent;
use crate::ui::Suggestion;

/// One coherent read of the application's observable state.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The focused node, when exactly one is selected.
    pub focused: Option<FocusedNode>,
    pub omnibar: OmnibarView,
    /// Find in the captured document, when its field is open.
    pub document_find: Option<DocumentFindView>,
    /// Focused document-control availability, mirrored from its owning engine.
    pub document_capabilities: Option<DocumentCapabilitiesView>,
    /// The active held web permission or authentication decision. Credential
    /// text is intentionally absent; only public request identity and UI state
    /// are observable.
    pub user_agent_decision: Option<UserAgentDecisionView>,
    /// Per-node content lifecycle, as (member, state label) pairs.
    pub content: Vec<(Uuid, String)>,
    pub node_count: usize,
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

/// The focused node's identity and captions, as the UI would present them.
#[derive(Clone, Debug, PartialEq)]
pub struct FocusedNode {
    pub member: Uuid,
    pub url: String,
    /// The at-rest caption (display label, plus host when it adds info).
    pub caption: String,
    /// Durable keep state read from the node's graph tags.
    pub kept: bool,
}

/// The omnibar as observed: state plus suggestion rows as display strings.
#[derive(Clone, Debug, PartialEq)]
pub struct OmnibarView {
    pub open: bool,
    pub text: String,
    pub cursor: usize,
    pub selected: usize,
    pub suggestions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentFindView {
    pub target: Uuid,
    pub query: String,
    pub count: usize,
    pub current: Option<usize>,
    pub pending: bool,
    pub status: String,
    pub current_role: Option<String>,
    pub current_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentCapabilitiesView {
    pub find_in_page: String,
    pub page_zoom: String,
    pub page_capture: String,
    pub navigation: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UserAgentDecisionView {
    pub node: Uuid,
    pub request: u64,
    pub kind: &'static str,
    pub prompt: String,
    pub queued: usize,
    pub submitting: bool,
    pub authentication_field: Option<&'static str>,
    pub remember_for_process: bool,
    pub error: Option<String>,
}

/// A semantic event: something durable or externally observable happened.
/// Drained by the shell each frame (into the scenario's log, or dropped);
/// later consumers (diagnostics, automation) subscribe at the same drain.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    AddressOpened(String),
    /// A live navigable committed a new resource in the same graph member.
    ContentNavigated {
        node: Uuid,
        url: String,
    },
    /// A live document changed the retained title of its graph member.
    ContentTitleChanged {
        node: Uuid,
        title: String,
    },
    /// A live document requested an auxiliary navigable. Placement policy is
    /// deliberately a later host decision; the request is still observable.
    AuxiliaryNavigableRequested {
        node: Uuid,
        url: String,
    },
    /// A live surface changed the host cursor. This is ephemeral presentation
    /// state, exposed so headed input receipts can prove callback consumption.
    SurfaceCursorChanged {
        node: Uuid,
        shape: &'static str,
    },
    /// A page offered a standards-shaped DataTransfer to the host. The node
    /// address remains attached even though winit cannot start the native OS
    /// drag loop needed to complete it yet.
    PageDragRequested {
        node: Uuid,
        types: String,
    },
    /// A page is waiting for an origin-scoped permission decision.
    PermissionRequested {
        node: Uuid,
        id: inker::UserAgentRequestId,
        origin: String,
        descriptors: Vec<inker::PermissionDescriptor>,
    },
    /// An RFC 9110 protection space is waiting for a credential-provider
    /// answer. This event deliberately carries no credentials.
    AuthenticationRequested {
        node: Uuid,
        id: inker::UserAgentRequestId,
        host: String,
        realm: Option<String>,
        scheme: String,
    },
    PermissionAnswered {
        node: Uuid,
        id: inker::UserAgentRequestId,
        answer: inker::PermissionAnswer,
        remembered: bool,
        succeeded: bool,
    },
    AuthenticationAnswered {
        node: Uuid,
        id: inker::UserAgentRequestId,
        supplied_credentials: bool,
        remembered_for_process: bool,
        succeeded: bool,
    },
    UserAgentRequestWithdrawn {
        node: Uuid,
        id: inker::UserAgentRequestId,
        kind: &'static str,
        reason: &'static str,
    },
    /// A smolweb server asked the person for input. The answer is never part
    /// of the event stream; sensitive answers must not enter diagnostics.
    SmolwebInputRequested {
        node: Uuid,
        prompt: String,
        sensitive: bool,
    },
    /// The answer was submitted and a query request issued. The resulting URL
    /// is deliberately absent because a sensitive Gemini answer lives in it.
    SmolwebInputSubmitted {
        node: Uuid,
        sensitive: bool,
    },
    SmolwebSubmissionComposed {
        target: String,
    },
    SmolwebSubmissionStarted {
        target: String,
        bytes: usize,
    },
    SmolwebSubmissionSucceeded {
        target: String,
        outcome: String,
    },
    SmolwebSubmissionFailed {
        target: String,
        error: String,
    },
    DownloadStarted {
        node: Uuid,
        url: String,
        bytes: u64,
    },
    DownloadCompleted {
        node: Uuid,
        destination: String,
        content_hash: String,
    },
    DownloadFailed {
        node: Uuid,
        error: String,
    },
    /// A capsule asked for a client identity. The origin is public routing
    /// context; certificate and key bytes never enter observation.
    GeminiIdentityRequested {
        node: Uuid,
        origin: String,
        prompt: String,
    },
    /// The active persona was approved for one capsule origin.
    GeminiIdentityBound {
        node: Uuid,
        origin: String,
    },
    /// A durable server pin refused a changed Gemini certificate.
    GeminiCertificateChanged {
        node: Uuid,
        target: String,
        pinned: String,
        seen: String,
    },
    /// Back stepped the history cursor to this address.
    NavigatedBack(String),
    /// Forward stepped the history cursor to this address.
    NavigatedForward(String),
    /// The focused node reloaded (refetch + content respawn when live).
    Reloaded(String),
    /// A graph node became a scheduled feed source.
    FeedSubscribed {
        node: Uuid,
        period: &'static str,
    },
    /// A graph node stopped scheduling feed refreshes.
    FeedUnsubscribed(Uuid),
    /// A feed refresh projected changed entries into the graph.
    FeedRefreshed {
        node: Uuid,
        changed: usize,
        unread: usize,
    },
    /// A scheduled or manual feed refresh failed loudly.
    FeedRefreshFailed {
        node: Uuid,
        error: String,
    },
    /// The focused entry's unread marker was cleared.
    FeedEntryRead(Uuid),
    /// One exact graph member entered durable kept state.
    NodeKept(Uuid),
    /// A dropped image textured this node's sprite face.
    NodeSpriteSet(Uuid),
    /// A node's viewer override changed (the settings row).
    ViewerChanged {
        node: Uuid,
        viewer: String,
    },
    /// A lens window was requested (rung 7).
    WindowOpened,
    /// A lens window closed.
    WindowClosed,
    /// The active pane tore out into a lens window (the leaf arm), by tag.
    PaneTornOut(String),
    /// A pane moved from a tiled station into its space's floating layer.
    PaneFloated(String),
    /// A floating pane rejoined its space's tiled topology.
    PaneDocked(String),
    /// A floating pane returned from a lens to the primary window.
    PaneReturned(String),
    /// A workbench tile tore out into a lens window as a pinned Tile pane
    /// (the branch arm), by the node's url.
    TileTornOut(String),
    /// A place command was refused, with the reason. Loud rather than silent:
    /// a message that never sends because authority was withdrawn must say so,
    /// not vanish.
    PlaceRefused(String),
    /// The app adopted a session (a boot, a mint, or a switch), by label.
    SessionSwitched(String),
    /// The current session was closed (trashed).
    SessionClosed,
    /// A fork minted a new session from the focused component (the fork arm).
    SessionForked,
    /// A trashed session was restored (overmap O3), by its label.
    SessionRecovered(String),
    /// A denizen install was staged for review, by label (B1).
    DenizenStaged(String),
    /// A denizen was installed after visible review, by label.
    DenizenInstalled(String),
    /// A resident denizen ran its body, by label.
    DenizenRan(String),
    /// A denizen was uninstalled: its delegations revoked, its residency gone.
    DenizenUninstalled(String),
    /// A behavior cascade hit its round budget with work still pending, naming
    /// the behaviors still answering each other. Loud rather than a silent
    /// truncation: the deferred work is still queued, and the user is owed the
    /// reason their helpers are looping.
    CascadeExhausted(String),
    /// A denizen install or run was refused, with the reason.
    DenizenRefused(String),
    /// A session's display name was set, by its new label.
    SessionRenamed(String),
    /// A node was removed from the graph into the recycle bin, by its url.
    NodeRemoved(String),
    /// The recycle-bin store failed (open / record / list) — the Removed
    /// section may be stale or empty for the WRONG reason; loud + attributable.
    BinFailed(String),
    /// Lexical recall over browsing memory could not answer — the lane going
    /// quiet because the index broke must be visible, not read as "you never
    /// visited anything about that".
    RecallFailed(String),
    /// A pane's composed list section was added or removed (the
    /// gloss-composite's add/remove), by provider id.
    PaneSectionToggled {
        section: String,
        added: bool,
    },
    /// A composed section moved in a pane's stack, by provider id.
    PaneSectionMoved(String),
    /// The recycle bin was emptied on command (athanor's oven), by how many
    /// records were permanently forgotten.
    RecycleBinEmptied(usize),
    /// A removed node was recovered (re-opened), by its url.
    NodeRecovered(String),
    OmnibarOpened,
    OmnibarClosed,
    /// A commit resolved to a suggestion (its display string).
    OmnibarCommitted(String),
    LayoutReseeded,
    ContentState {
        node: Uuid,
        state: String,
    },
    /// A pane of the named kind was summoned into the tree (rung 5 slice C).
    PaneSummoned(&'static str),
    /// The active pane was closed.
    PaneClosed,
    /// The focused node opened as a workbench tile (rung 5 slice E).
    WorkbenchTileOpened(String),
    /// The focused node's workbench tile closed.
    WorkbenchTileClosed,
    /// A tab-drag stacked one tile into another's cell.
    WorkbenchStacked,
    /// An edge-drop split a tile out beside another's cell.
    WorkbenchSplit,
    /// A pane interaction named a target that is not on screen — a
    /// `click-row`/`click-tab`/`click-node` that resolved to nothing. Divergence
    /// a driving script or model must be able to see: the aim missed, and a
    /// receipt that only checks the end state would call the miss a pass. `what`
    /// is the interaction kind, `target` the name that did not resolve.
    InteractionMissed {
        what: &'static str,
        target: String,
    },
    /// An affordance fired that is not wired yet — today only Trail's Recover,
    /// which awaits the deletion log (rung 6). A known-not-yet state, emitted so
    /// a scenario asserts the gap explicitly rather than a silent no-op.
    AffordanceUnavailable {
        what: &'static str,
        target: String,
    },
}

impl AppEvent {
    /// A grep-friendly one-line rendering (what `assert event` matches).
    pub fn describe(&self) -> String {
        match self {
            AppEvent::AddressOpened(url) => format!("address-opened {url}"),
            AppEvent::ContentNavigated { node, url } => {
                format!("content-navigated {node} {url}")
            }
            AppEvent::ContentTitleChanged { node, title } => {
                format!("content-title {node} {title}")
            }
            AppEvent::AuxiliaryNavigableRequested { node, url } => {
                format!("auxiliary-navigable {node} {url}")
            }
            AppEvent::SurfaceCursorChanged { node, shape } => {
                format!("surface-cursor {node} {shape}")
            }
            AppEvent::PageDragRequested { node, types } => {
                format!("page-drag-requested {node} types={types}")
            }
            AppEvent::PermissionRequested {
                node,
                id,
                origin,
                descriptors,
            } => format!(
                "permission-requested {node} request={} {origin} {}",
                id.get(),
                descriptors
                    .iter()
                    .map(|descriptor| format!("{descriptor:?}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            AppEvent::AuthenticationRequested {
                node,
                id,
                host,
                realm,
                scheme,
            } => format!(
                "authentication-requested {node} request={} {host} {scheme} {}",
                id.get(),
                realm.as_deref().unwrap_or("")
            ),
            AppEvent::PermissionAnswered {
                node,
                id,
                answer,
                remembered,
                succeeded,
            } => format!(
                "permission-answered {node} request={} answer={answer:?} remembered={remembered} succeeded={succeeded}",
                id.get()
            ),
            AppEvent::AuthenticationAnswered {
                node,
                id,
                supplied_credentials,
                remembered_for_process,
                succeeded,
            } => format!(
                "authentication-answered {node} request={} supplied={supplied_credentials} process-memory={remembered_for_process} succeeded={succeeded}",
                id.get()
            ),
            AppEvent::UserAgentRequestWithdrawn {
                node,
                id,
                kind,
                reason,
            } => format!(
                "user-agent-request-withdrawn {node} request={} kind={kind} reason={reason}",
                id.get()
            ),
            AppEvent::SmolwebInputRequested {
                node,
                prompt,
                sensitive,
            } => format!("smolweb-input-requested {node} sensitive={sensitive} {prompt}"),
            AppEvent::SmolwebInputSubmitted { node, sensitive } => {
                format!("smolweb-input-submitted {node} sensitive={sensitive}")
            }
            AppEvent::SmolwebSubmissionComposed { target } => {
                format!("smolweb-submission-composed {target}")
            }
            AppEvent::SmolwebSubmissionStarted { target, bytes } => {
                format!("smolweb-submission-started {target} bytes={bytes}")
            }
            AppEvent::SmolwebSubmissionSucceeded { target, outcome } => {
                format!("smolweb-submission-succeeded {target} {outcome}")
            }
            AppEvent::SmolwebSubmissionFailed { target, error } => {
                format!("smolweb-submission-failed {target} {error}")
            }
            AppEvent::DownloadStarted { node, url, bytes } => {
                format!("download-started {url} bytes={bytes} node={node}")
            }
            AppEvent::DownloadCompleted {
                node,
                destination,
                content_hash,
            } => format!(
                "download-completed {node} hash={} {destination}",
                content_hash.get(..12).unwrap_or(content_hash)
            ),
            AppEvent::DownloadFailed { node, error } => {
                format!("download-failed {node} {error}")
            }
            AppEvent::GeminiIdentityRequested {
                node,
                origin,
                prompt,
            } => format!("gemini-identity-requested {node} {origin} {prompt}"),
            AppEvent::GeminiIdentityBound { node, origin } => {
                format!("gemini-identity-bound {node} {origin}")
            }
            AppEvent::GeminiCertificateChanged {
                node,
                target,
                pinned,
                seen,
            } => format!(
                "gemini-certificate-changed {node} {target} {} {}",
                pinned.get(..12).unwrap_or(pinned),
                seen.get(..12).unwrap_or(seen)
            ),
            AppEvent::NavigatedBack(url) => format!("nav-back {url}"),
            AppEvent::NavigatedForward(url) => format!("nav-forward {url}"),
            AppEvent::Reloaded(url) => format!("reloaded {url}"),
            AppEvent::FeedSubscribed { node, period } => {
                format!("feed-subscribed {node} {period}")
            }
            AppEvent::FeedUnsubscribed(node) => format!("feed-unsubscribed {node}"),
            AppEvent::FeedRefreshed {
                node,
                changed,
                unread,
            } => format!("feed-refreshed {node} changed={changed} unread={unread}"),
            AppEvent::FeedRefreshFailed { node, error } => {
                format!("feed-refresh-failed {node} {error}")
            }
            AppEvent::FeedEntryRead(node) => format!("feed-entry-read {node}"),
            AppEvent::NodeKept(node) => format!("node-kept {node}"),
            AppEvent::NodeSpriteSet(node) => format!("sprite-set {node}"),
            AppEvent::ViewerChanged { node, viewer } => format!("viewer-changed {node} {viewer}"),
            AppEvent::WindowOpened => "window-opened".to_string(),
            AppEvent::WindowClosed => "window-closed".to_string(),
            AppEvent::PaneTornOut(tag) => format!("pane-torn-out {tag}"),
            AppEvent::PaneFloated(tag) => format!("pane-floated {tag}"),
            AppEvent::PaneDocked(tag) => format!("pane-docked {tag}"),
            AppEvent::PaneReturned(tag) => format!("pane-returned {tag}"),
            AppEvent::TileTornOut(url) => format!("tile-torn-out {url}"),
            AppEvent::PlaceRefused(reason) => format!("place-refused {reason}"),
            AppEvent::SessionSwitched(label) => format!("session-switched {label}"),
            AppEvent::SessionClosed => "session-closed".to_string(),
            AppEvent::SessionForked => "session-forked".to_string(),
            AppEvent::SessionRecovered(label) => format!("session-recovered {label}"),
            AppEvent::DenizenStaged(label) => format!("denizen-staged {label}"),
            AppEvent::DenizenInstalled(label) => format!("denizen-installed {label}"),
            AppEvent::DenizenRan(label) => format!("denizen-ran {label}"),
            AppEvent::DenizenUninstalled(label) => format!("denizen-uninstalled {label}"),
            AppEvent::CascadeExhausted(names) => format!("cascade-exhausted {names}"),
            AppEvent::DenizenRefused(reason) => format!("denizen-refused {reason}"),
            AppEvent::SessionRenamed(label) => format!("session-renamed {label}"),
            AppEvent::NodeRemoved(url) => format!("node-removed {url}"),
            AppEvent::NodeRecovered(url) => format!("node-recovered {url}"),
            AppEvent::BinFailed(error) => format!("bin-failed {error}"),
            AppEvent::RecallFailed(error) => format!("recall-failed {error}"),
            AppEvent::RecycleBinEmptied(n) => format!("recycle-bin-emptied {n}"),
            AppEvent::PaneSectionToggled { section, added } => {
                let verb = if *added { "added" } else { "removed" };
                format!("pane-section-{verb} {section}")
            }
            AppEvent::PaneSectionMoved(section) => format!("pane-section-moved {section}"),
            AppEvent::OmnibarOpened => "omnibar-opened".to_string(),
            AppEvent::OmnibarClosed => "omnibar-closed".to_string(),
            AppEvent::OmnibarCommitted(what) => format!("omnibar-committed {what}"),
            AppEvent::LayoutReseeded => "layout-reseeded".to_string(),
            AppEvent::ContentState { node, state } => format!("content {node} {state}"),
            AppEvent::PaneSummoned(kind) => format!("pane-summoned {kind}"),
            AppEvent::PaneClosed => "pane-closed".to_string(),
            AppEvent::WorkbenchTileOpened(url) => format!("workbench-opened {url}"),
            AppEvent::WorkbenchTileClosed => "workbench-closed".to_string(),
            AppEvent::WorkbenchStacked => "workbench-stacked".to_string(),
            AppEvent::WorkbenchSplit => "workbench-split".to_string(),
            AppEvent::InteractionMissed { what, target } => {
                format!("interaction-missed {what} {target}")
            }
            AppEvent::AffordanceUnavailable { what, target } => {
                format!("affordance-unavailable {what} {target}")
            }
        }
    }
}

/// Read the application snapshot. Pure; the app is not disturbed.
pub fn snapshot(app: &App) -> Snapshot {
    let focused = app.graph_runtimes.focused_member().and_then(|member| {
        let url = app.graph_runtimes.focused_url()?.to_string();
        let caption = crate::app::focused_caption(&app.graph_runtimes)?;
        Some(FocusedNode {
            member,
            url,
            caption,
            kept: app.node_is_kept(member),
        })
    });
    let content = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter_map(|(_, n)| {
            let state = match app.content.get(n.id)? {
                NodeContent::Requested => "requested".to_string(),
                NodeContent::Live => "live".to_string(),
                NodeContent::AwaitingInput => "awaiting-input".to_string(),
                NodeContent::AwaitingIdentity => "awaiting-identity".to_string(),
                NodeContent::AwaitingTrust => "awaiting-trust".to_string(),
                NodeContent::Failed(err) => format!("failed: {err}"),
            };
            Some((n.id, state))
        })
        .collect();
    // The surface list, derived from app truth (the shell owns the live sessions
    // and the window size; observe reports what a frame would compose). The base
    // is the frisket tree — the Orrery leaf is the canvas, every other leaf a
    // pane — then content over the canvas when the focused node is Live, then
    // chrome on top when it has something to show.
    let mut surfaces: Vec<String> = app
        .frisket
        .iter_leaves()
        .map(|(_, content, _)| {
            match content {
                crate::panes::PaneContent::Orrery => "canvas".to_string(),
                // A pinned Tile pane with a live session composites as a
                // content surface at the pane's rect (the plan's mapping).
                crate::panes::PaneContent::Tile(m)
                    if matches!(app.content.get(*m), Some(NodeContent::Live)) =>
                {
                    "content".to_string()
                }
                _ => "pane".to_string(),
            }
        })
        .collect();
    // A split tree has seams: one divider surface per split node.
    if matches!(app.frisket.root, crate::panes::PaneNode::Split { .. }) && app.maximized.is_none() {
        surfaces.push("divider".to_string());
    }
    // Content surfaces, honestly: a live tile composites where its workbench
    // PANE lives (primary or a lens it tore out to), and the focused node's
    // inset is suppressed while that node tiles in a visible workbench — the
    // one-session-one-surface rule, across windows, mirrored from the shell's
    // plan so this report never claims a surface the frame won't compose.
    let wb_in_primary = app
        .frisket
        .iter_leaves()
        .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Workbench));
    let wb_in_lens = app.lenses.iter().flatten().any(|space| {
        space
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Workbench))
    });
    let live = |m: uuid::Uuid| matches!(app.content.get(m), Some(NodeContent::Live));
    let tiled: Vec<uuid::Uuid> = app
        .active_workbench()
        .map(|workbench| workbench.to_arrangement().1)
        .and_then(|geometry| geometry)
        .map(|geometry| {
            crate::workbench_tiling::place_workbench(
                Some(&geometry),
                crate::surface::Rect::full(1, 1),
            )
            .cells
            .iter()
            .filter_map(|cell| cell.active_member())
            .collect()
        })
        .unwrap_or_default();
    // Pinned Tile panes claim their member wherever their space shows (the
    // same one-session-one-surface rule the workbench tiles follow).
    let tile_panes: Vec<uuid::Uuid> = app
        .frisket
        .iter_leaves()
        .chain(app.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
        .filter_map(|(_, c, _)| match c {
            crate::panes::PaneContent::Tile(m) => Some(*m),
            _ => None,
        })
        .collect();
    let tile_content_here = wb_in_primary && tiled.iter().any(|m| live(*m));
    let focused_inset = app.graph_runtimes.focused_member().is_some_and(|m| {
        live(m)
            && !((wb_in_primary || wb_in_lens) && tiled.contains(&m))
            && !tile_panes.contains(&m)
    });
    if tile_content_here || focused_inset {
        surfaces.push("content".to_string());
    }
    if app.document_find.open
        || app.user_agent_decision.is_open()
        || app.omnibar.open && app.shell_chrome_config().projects_omnibar()
        || app.shell_chrome_config().projects_shellbar()
            && crate::app::focused_caption(&app.graph_runtimes).is_some()
    {
        surfaces.push("chrome".to_string());
    }
    Snapshot {
        focused,
        omnibar: OmnibarView {
            open: app.omnibar.open,
            text: app.omnibar.presented_text(),
            cursor: app.omnibar.presented_cursor(),
            selected: app.omnibar.selected,
            suggestions: app
                .omnibar
                .suggestions
                .iter()
                .map(suggestion_line)
                .collect(),
        },
        document_find: app.document_find.open.then(|| {
            let current = app
                .document_find
                .model
                .current
                .filter(|index| *index < app.document_find.model.count);
            let current_match =
                current.and_then(|index| app.document_find.model.matches.get(index));
            DocumentFindView {
                target: app
                    .document_find
                    .target
                    .expect("an open find has a captured target"),
                query: app.document_find.query.clone(),
                count: app.document_find.model.count,
                current,
                pending: app.document_find.pending,
                status: app.document_find.status(),
                current_role: current_match.and_then(|item| item.role.clone()),
                current_label: current_match.map(|item| item.label.clone()),
            }
        }),
        document_capabilities: app
            .graph_runtimes
            .focused_member()
            .and_then(|node| app.content.facts(node))
            .map(|facts| DocumentCapabilitiesView {
                find_in_page: facts.capabilities.find_in_page.describe(),
                page_zoom: facts.capabilities.page_zoom.describe(),
                page_capture: facts.capabilities.page_capture.describe(),
                navigation: facts.capabilities.navigation.describe(),
            }),
        user_agent_decision: app.user_agent_decision.active().map(|decision| {
            let request = decision.request();
            let authentication_field = matches!(
                decision,
                crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. }
            )
            .then_some(match app.user_agent_decision.authentication.field {
                crate::user_agent_decision::AuthenticationField::Username => "username",
                crate::user_agent_decision::AuthenticationField::Password => "password",
            });
            UserAgentDecisionView {
                node: request.node,
                request: request.id.get(),
                kind: decision.kind(),
                prompt: decision.prompt(),
                queued: app.user_agent_decision.len(),
                submitting: app.user_agent_decision.submitting == Some(request),
                authentication_field,
                remember_for_process: app.user_agent_decision.authentication.remember_for_process,
                error: app.user_agent_decision.error.clone(),
            }
        }),
        content,
        node_count: app.graph_runtimes.graph().nodes().count(),
        feed_subscriptions: app.feeds.len(),
        feed_unread: app.feeds.unread_count(),
        graph_visible: app.graph_runtimes.graph_visible(),
        surfaces,
        focus: app.focus.label().to_string(),
        panes: app
            .frisket
            .iter_leaves()
            .map(|(_, content, _)| content.tag().to_string())
            .collect(),
        active_pane: app.active_pane.and_then(|id| {
            app.frisket
                .iter_leaves()
                .find(|(pid, _, _)| *pid == id)
                .map(|(_, content, _)| content.tag().to_string())
        }),
        floating_panes: app
            .blueprint_space(crate::action::SpaceRef::Primary)
            .into_iter()
            .flat_map(|space| space.visible_floating_panes(true))
            .filter_map(|floating| app.pane_content(floating.pane).map(|content| content.tag()))
            .map(str::to_string)
            .collect(),
        maximized: app.maximized.is_some(),
        trail_rows: app
            .frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Trail))
            .then(|| {
                crate::trail_view::trail_rows(app)
                    .into_iter()
                    .map(|r| r.text)
                    .collect()
            })
            .unwrap_or_default(),
        roster_rows: app
            .frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Roster))
            .then(|| {
                crate::roster_view::roster_rows(app)
                    .into_iter()
                    .map(|r| r.text)
                    .collect()
            })
            .unwrap_or_default(),
        inspector_rows: app
            .frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Inspector))
            .then(|| crate::inspector_view::inspector_lines(app))
            .unwrap_or_default(),
        roster_tab: crate::cambium_pane::tab_label(app.roster_tab),
        split_ratio: match &app.frisket.root {
            crate::panes::PaneNode::Split { ratio, .. } => Some(*ratio),
            crate::panes::PaneNode::Leaf { .. } => None,
        },
        workbench_cells: workbench_cells(app),
        workbench_fractions: app
            .active_workbench()
            .map(mere::platen::Workbench::weights)
            .unwrap_or_default(),
        a11y: crate::a11y::a11y_lines(app),
        can_back: app.focused_can_back(),
        can_forward: app.focused_can_forward(),
        browser_fetch: app.graph_runtimes.focused_member().and_then(|node| {
            app.content.fetch_phase(node).map(|phase| match phase {
                crate::content::PageFetchPhase::Requested => "requested".to_string(),
                crate::content::PageFetchPhase::Streaming { received_bytes, .. } => {
                    format!("streaming bytes={received_bytes}")
                }
                crate::content::PageFetchPhase::Settled { received_bytes } => {
                    format!("settled bytes={received_bytes}")
                }
                crate::content::PageFetchPhase::Loading { progress_millis } => progress_millis
                    .map(|value| format!("loading percent={}", value / 10))
                    .unwrap_or_else(|| "loading".to_string()),
                crate::content::PageFetchPhase::Stopped { received_bytes } => {
                    format!("stopped bytes={received_bytes}")
                }
            })
        }),
        link_preview: app.link_preview.clone(),
        available_actions: app
            .available_actions()
            .into_iter()
            .map(|(label, _)| label)
            .collect(),
        windows: app.window_count,
        lens_panes: app
            .lenses
            .iter()
            .enumerate()
            .filter_map(|(ordinal, space)| space.as_ref().map(|s| (ordinal, s)))
            .flat_map(|(ordinal, space)| {
                space
                    .iter_leaves()
                    .map(move |(_, content, _)| format!("{ordinal}:{}", content.tag()))
                    .collect::<Vec<_>>()
            })
            .collect(),
        lens_floating_panes: app
            .lens_blueprints
            .iter()
            .enumerate()
            .filter_map(|(ordinal, space)| space.as_ref().map(|space| (ordinal, space)))
            .flat_map(|(ordinal, space)| {
                space
                    .visible_floating_panes(true)
                    .into_iter()
                    .filter_map(move |floating| {
                        app.pane_content(floating.pane)
                            .map(|content| format!("{ordinal}:{}", content.tag()))
                    })
                    .collect::<Vec<_>>()
            })
            .collect(),
        session: app.session_label(app.session_id),
        session_count: app.sessions.len(),
        active_ratio: app.active_pane.and_then(|active| {
            let layout = app.space(app.space_of(active)?)?;
            let mut path = crate::pane::path_of(layout, active)?;
            // The parent split holds the divider; a root leaf has none.
            path.pop()?;
            crate::pane::place_panes(layout, crate::surface::Rect::full(100, 100), None)
                .dividers
                .iter()
                .find(|d| d.path == path)
                .map(|d| d.ratio)
        }),
    }
}

/// The workbench's cells as observation strings: each cell's tab labels (the
/// node's display label off graph truth) joined by `|`, `*` on the active tab.
pub fn workbench_cells(app: &App) -> Vec<String> {
    let label_of = |member: uuid::Uuid| -> String {
        app.graph_runtimes
            .graph()
            .nodes()
            .find(|(_, n)| n.id == member)
            .map(|(_, n)| {
                if n.title.trim().is_empty() {
                    n.url().to_string()
                } else {
                    n.title.clone()
                }
            })
            .unwrap_or_else(|| member.to_string())
    };
    app.active_workbench()
        .into_iter()
        .flat_map(|workbench| workbench.slot_views())
        .map(|slot| {
            slot.members
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let label = label_of(*m);
                    if i == slot.active {
                        format!("{label}*")
                    } else {
                        label
                    }
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

/// One suggestion row as its display string (the assert/a11y rendering).
pub fn suggestion_line(s: &Suggestion) -> String {
    match s {
        Suggestion::Node { label, host, .. } if !host.is_empty() => {
            format!("{label} \u{00b7} {host}")
        }
        Suggestion::Node { label, .. } => label.clone(),
        Suggestion::Go { url } => format!("go {url}"),
        Suggestion::Recall { url, .. } => format!("recall {url}"),
        Suggestion::Act { label, .. } => format!("\u{203a} {label}"),
        Suggestion::Hint(h) => h.to_string(),
        Suggestion::Prompt(prompt) => prompt.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::content::{CapabilityStatus, ContentFacts, DocumentCapabilityFacts};

    #[test]
    fn snapshot_reads_focus_omnibar_and_content_coherently() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".to_string()));
        app.update(Action::OmnibarOpen { command: true });
        app.update(Action::OmnibarChar('r'));

        let snap = snapshot(&app);
        let focused = snap.focused.expect("the opened node is focused");
        assert_eq!(focused.url, "mere://alpha");
        assert!(snap.omnibar.open);
        assert_eq!(snap.omnibar.text, ">r");
        assert!(
            snap.omnibar
                .suggestions
                .iter()
                .any(|s| s.contains("Reseed layout")),
            "suggestion rows render as display strings: {:?}",
            snap.omnibar.suggestions
        );
        assert_eq!(snap.node_count, 1);
        assert!(snap.content.is_empty(), "no content lifecycle yet");
    }

    #[test]
    fn snapshot_projects_focused_document_capabilities() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/".to_string()));
        let node = app.graph_runtimes.focused_member().unwrap();
        app.content.note_live(
            node,
            Some(ContentFacts {
                engine: "weld.chromium".into(),
                structure: None,
                lineage: None,
                capabilities: DocumentCapabilityFacts {
                    find_in_page: CapabilityStatus::Supported,
                    page_zoom: CapabilityStatus::Unsupported {
                        reason: "absolute zoom mapping is missing".into(),
                    },
                    page_capture: CapabilityStatus::Partial {
                        detail: "visible viewport only".into(),
                    },
                    navigation: CapabilityStatus::Supported,
                },
            }),
        );

        let capabilities = snapshot(&app)
            .document_capabilities
            .expect("focused live capabilities");
        assert_eq!(capabilities.find_in_page, "supported");
        assert_eq!(
            capabilities.page_zoom,
            "unavailable: absolute zoom mapping is missing"
        );
        assert_eq!(capabilities.page_capture, "partial: visible viewport only");
        assert_eq!(capabilities.navigation, "supported");
    }

    #[test]
    fn semantic_actions_emit_events() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".to_string()));
        app.update(Action::OmnibarOpen { command: false });
        app.update(Action::OmnibarClose);
        app.update(Action::ToggleNodeContent);
        let described: Vec<String> = app.take_events().iter().map(AppEvent::describe).collect();
        assert!(described.iter().any(|e| e == "address-opened mere://alpha"));
        assert!(described.iter().any(|e| e == "omnibar-opened"));
        assert!(described.iter().any(|e| e == "omnibar-closed"));
        assert!(
            described
                .iter()
                .any(|e| e.starts_with("content ") && e.ends_with(" requested")),
            "{described:?}"
        );
        assert!(app.take_events().is_empty(), "take drains");
    }
}
