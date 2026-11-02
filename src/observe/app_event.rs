// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A semantic event: something durable or externally observable happened.
/// Drained by the shell each frame (into the scenario's log, or dropped);
/// later consumers (diagnostics, automation) subscribe at the same drain.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    /// A physics law applied while refusing overlays, or an overlay request
    /// was refused without changing the force set.
    PhysicsRefused(mere::canvas::OverlayRefusal),
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
    SourceDocumentCaptureProgress {
        request: Uuid,
        node: Uuid,
        session: crate::panes::SessionId,
        stage: String,
        detail: Option<String>,
        stored: Option<crate::action::StoredSourceDocument>,
    },
    /// Verified local evidence and its owning facet/graph writes acknowledged success.
    SourceDocumentCaptured {
        node: Uuid,
        url: String,
        raw_manifest: String,
        annotation_manifest: String,
    },
    /// The explicit action was refused or its local custody write failed.
    SourceDocumentCaptureFailed {
        node: Uuid,
        url: String,
        error: String,
    },
    /// Eidetic custody succeeded, but the target changed before the manifest
    /// reference could attach to its node.
    SourceDocumentCaptureUnattached {
        node: Uuid,
        url: String,
        raw_manifest: String,
        annotation_manifest: String,
        reason: String,
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
    /// A Redshank dock command was refused, in the listener's words.
    RedshankRefused(String),
    /// One exact graph member entered durable kept state.
    NodeKept(Uuid),
    /// A dropped image textured this node's sprite face.
    NodeSpriteSet(Uuid),
    /// A node's viewer override changed (the settings row).
    ViewerChanged {
        node: Uuid,
        viewer: String,
    },
    /// A node's REQUESTED page zoom changed. What the engine then applies is
    /// its own business and is not reported here.
    PageZoomChanged {
        node: Uuid,
        percent: u32,
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
    /// The personae vault was locked at start: the profile identity is
    /// pending, and what speaks as the profile waits (vault lock ruling 35).
    ProfileIdentityPending,
    /// The vault unlocked and the pending identity became the profile root,
    /// with the backend's account of what protects it.
    ProfileIdentityUnlocked(String),
    /// A place command was refused, with the reason. Loud rather than silent:
    /// a message that never sends because authority was withdrawn must say so,
    /// not vanish.
    PlaceRefused(String),
    /// This bind's full rendezvous ticket(s) went to the system clipboard,
    /// by count. The ticket itself is never observed: the status row elides
    /// it precisely so it does not ride along in a receipt.
    PlaceRendezvousCopied(usize),
    /// The app adopted a session (a boot, a mint, or a switch), by label.
    SessionSwitched(String),
    SessionLoadRefused {
        error: String,
    },
    /// The current session was closed (trashed).
    SessionClosed,
    /// A fork minted a new session from the focused component (the fork arm).
    SessionForked,
    /// A trashed session was restored (overmap O3), by its label.
    SessionRecovered(String),
    /// A participant install was staged for review, by label (B1).
    DenizenStaged(String),
    /// A participant was installed after visible review, by label.
    DenizenInstalled(String),
    /// A resident participant ran its body, by label.
    DenizenRan(String),
    /// A participant was uninstalled: its delegations revoked, its residency gone.
    DenizenUninstalled(String),
    /// A behavior cascade hit its round budget with work still pending, naming
    /// the behaviors still answering each other. Loud rather than a silent
    /// truncation: the deferred work is still queued, and the user is owed the
    /// reason their helpers are looping.
    CascadeExhausted(String),
    /// A participant install or run was refused, with the reason.
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
    /// The person kept (or dropped) a command in the palette.
    CommandKept {
        label: String,
        kept: bool,
    },
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
    ClipPackageExported {
        clip: String,
        path: String,
    },
}
