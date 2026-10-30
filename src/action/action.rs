// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
/// A typed app intent. The shell (keys, later the omnibar / command palette /
/// automation adapters) produces these; [`crate::app::update`] consumes them.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Reading(crate::reading_view::ReadAction),
    /// Edit authored themes in the shared native workshop.
    OpenThemeWorkshop,
    /// Open Alembic's shared evidence-brief workshop.
    OpenAlembicWorkshop,
    /// Open an address: mint/select its node in the graph and fetch it.
    OpenAddress(String),
    /// Open the submission composer for the focused Titan or Spartan address.
    ComposeFocusedSmolwebSubmission,
    /// Open the focused Micron page's explicit form editor, if it has one.
    ComposeFocusedMicronForm,
    /// Begin a submission from a typed document interaction or explicit target.
    BeginSmolwebSubmission {
        source: Option<uuid::Uuid>,
        target: String,
    },
    /// Replace the composer's body with bytes read from a dropped file.
    SmolwebSubmissionFile {
        name: String,
        bytes: Vec<u8>,
        suggested_mime: String,
    },
    /// A live navigable committed a new top-level resource in an existing
    /// graph member. This grows that member's own navigation lineage; it does
    /// not mint another member merely because its current URL changed.
    ContentNavigationCommitted {
        member: uuid::Uuid,
        url: String,
    },
    /// The active document supplied a title for its existing graph member.
    ContentTitleChanged {
        member: uuid::Uuid,
        title: String,
    },
    /// Capture the focused live document and open its retained find field.
    OpenDocumentFind,
    CloseDocumentFind,
    /// The find field's edited input, text and caret: the one edit the
    /// field's shared key map reports.
    EditDocumentFind(cambium::TextInput),
    StepDocumentFind(DocumentFindDirection),
    /// Step one member's requested page zoom a rung up the ladder. A document
    /// scale: chrome appearance and the canvas camera are untouched. The
    /// member rides along (the `KeepNode` shape) because the palette rows
    /// resolve the focused node while ctrl+wheel resolves the node under the
    /// pointer, and a workbench can tile several live documents at once.
    PageZoomIn {
        member: uuid::Uuid,
    },
    /// Step one member's requested page zoom a rung down.
    PageZoomOut {
        member: uuid::Uuid,
    },
    /// Return one member to the default page scale.
    PageZoomReset {
        member: uuid::Uuid,
    },
    /// Capture this exact live page member. The target is resolved when the
    /// action is offered so a later focus change cannot redirect completion.
    CapturePage {
        member: uuid::Uuid,
    },
    /// Explicitly preserve the current fetched HTML source and its Fleece
    /// annotation. This does not inspect a rendered surface or recall text.
    CaptureSourceDocument {
        target: crate::source_capture::SourceCaptureTarget,
    },
    /// Retry the same explicit request; never recapture a replacement response.
    RetrySourceDocumentCapture {
        request: uuid::Uuid,
    },
    /// Search the current session's explicitly attached saved evidence.
    SearchSavedSources,
    ResearchSearch(ResearchSearchAction),
    /// Read immutable source text at an optional exact Unicode passage.
    OpenSavedSource(crate::saved_sources::Target),
    ChoosePermission {
        request: crate::user_agent_decision::UserAgentRequestKey,
        choice: crate::user_agent_decision::PermissionChoice,
    },
    FocusAuthenticationField(crate::user_agent_decision::AuthenticationField),
    InsertAuthentication(String),
    BackspaceAuthentication,
    ToggleAuthenticationMemory,
    SubmitAuthentication {
        request: crate::user_agent_decision::UserAgentRequestKey,
    },
    CancelAuthentication {
        request: crate::user_agent_decision::UserAgentRequestKey,
    },
    /// Step the focused node back through its own content lineage. No-op at
    /// that node's oldest entry.
    Scope(crate::scope_navigation::ScopeCommand),
    FocusCard(crate::focus_card::Command),
    SelectScopeMere(crate::panes::SessionId),
    NavBack,
    /// Step the focused node forward through its own content lineage.
    NavForward,
    /// Reload the focused node: refetch its enrichment, and respawn its live
    /// content session when it has one.
    Reload,
    /// Stop the focused node's active page request or hosted navigation.
    Stop,
    /// Promote one exact graph member into durable kept state. The member is
    /// captured when the control is offered so a later focus change cannot
    /// redirect the gesture.
    KeepNode {
        member: uuid::Uuid,
    },
    /// Keep the focused node as a feed source and refresh it on this cadence.
    /// The first refresh is immediate; later refreshes use the host clock.
    SubscribeFocusedFeed {
        period: servitor::Period,
    },
    /// Stop scheduled refreshes for the focused source. The node remains kept.
    UnsubscribeFocusedFeed,
    /// Refresh every subscribed source now, subject to the in-flight gate.
    RefreshFeeds,
    /// Clear the unread marker from the focused feed entry.
    MarkFocusedFeedEntryRead,
    /// Set a node's sprite face (a dropped image file, decoded by the shell
    /// into a PNG data-URI — the decode is platform/file work, so it happens
    /// port-side and only the typed result lowers). `hull` is the traced
    /// collider polygon (face-normalized; under 3 points = keep the
    /// silhouette collider) — the meerkat-harvest tracer, now canvas's.
    SetNodeSprite {
        member: uuid::Uuid,
        data_uri: String,
        hull: Vec<(f32, f32)>,
    },
    /// Open another window onto the same state (rung 7): a lens — the same
    /// graph through its own camera and its OWN pane space (each window holds
    /// a frisket tree over the one App), per the one-state-N-windows doctrine.
    NewWindow,
    /// Tear the active pane out into a lens window (the tear-out trichotomy's
    /// leaf arm): the pane's frisket leaf leaves this window's tree and joins
    /// the newest lens's (spawning one when none is open). The pane's retained
    /// runner — its DOM, widget state, scroll — is untouched by the move, so
    /// identity survives BY CONSTRUCTION in the surface-compositor shape.
    TearOutActivePane,
    /// Move the active tiled pane into its space's floating layer. Its
    /// `PaneId` and retained runner remain unchanged.
    FloatActivePane,
    /// Dock the active floating pane beside a tiled peer in its current space.
    DockActivePane,
    /// Move the active floating pane from a lens back to the primary window,
    /// retaining it as a float there.
    ReturnActivePaneToPrimary,
    /// Set a node's viewer override (the settings matrix row): `None` returns
    /// to automatic routing; `Some(engine_id)` pins that lane. Persists in the
    /// browser-state sidecar and respawns live content through the pinned
    /// route, so the change is APPLIED, not merely stored.
    SetViewerOverride {
        member: uuid::Uuid,
        viewer: Option<String>,
    },
    /// Re-seed the canvas layout and replay the settle.
    ReseedLayout,
    /// Shared graph grammar commands, applied to the owning Orrery pane.
    ResetToSeed,
    SetSeed,
    RestoreRecipeSeed,
    ToggleForegroundPin,
    DismissRetained,
    GrowTiles,
    ShrinkTiles,
    /// Toggle the owning Map's categorical ground; leaves graph data and arrangement intact.
    ToggleTerritory,
    /// Native view curation; never changes graph truth or source positions.
    FoldSelection,
    ExpandFold,
    ExpandAllFolds,
    UndoFold,
    RedoFold,
    /// Frame the camera on the current content bounds. An analytic layout can
    /// place nodes anywhere in world space (and the extent-aware Spiral spreads
    /// wide), so the view needs an explicit fit. (Projection proofs — P3.)
    FitView,
    /// Switch the canvas layout: `Some(id)` selects an analytic cartography
    /// strategy (the shell projects it per frame through the canvas's
    /// recompute gate); `None` reverts to force-directed physics. The first
    /// host wiring of the analytic catalog (projection-engine proof 1).
    SetLayoutStrategy(Option<&'static str>),
    /// Switch the physics law the graph moves under, by catalog id
    /// (`mere::canvas::PhysicsLaw::id`). (Physics catalog — P2.)
    SetPhysicsLaw(&'static str),
    /// Turn one physics overlay on or off, by catalog id.
    SetPhysicsOverlay(&'static str, bool),
    /// Where the Kinds law reads a node's kind from, by catalog id.
    SetPhysicsKindSource(&'static str),
    /// Where Orbit's masses and the hub overlays' weights come from.
    SetPhysicsMassSource(&'static str),
    /// Where the Depth overlay reads a node's depth from.
    SetPhysicsDepthSource(&'static str),
    /// Apply a named physics profile (a law plus overlays), by id.
    ApplyPhysicsProfile(&'static str),
    /// The role every item takes unless a group or the item says otherwise,
    /// by `mere::canvas::Role::id`.
    SetPhysicsRole(&'static str),
    /// Which groups channel role overrides read, by kind-source id.
    SetPhysicsRoleGroups(&'static str),
    /// Replace the law with one bounded two-law force mix.
    ApplyPhysicsMix(mere::canvas::physics_controls::TwoLawMix),
    /// Replace the law with one bounded grouped composition.
    ApplyPhysicsGrouping(mere::canvas::physics_controls::TwoLawGrouping),
    /// Replace the law with a two-stage schedule and start it.
    ApplyPhysicsSchedule(mere::canvas::physics_controls::TwoStageSchedule),
    /// Return from a composition to the single law the canvas is running.
    UseSingleLaw,
    /// Run the simulation at a speed preset, by `SPEED_PRESETS` id.
    SetPhysicsSpeed(&'static str),
    /// Toggle the isometric (2.5D foreshortened) view.
    ToggleIsometric,
    /// Orbit the view (yaw) by radians.
    OrbitBy(f32),
    /// Tilt the view (vertical foreshorten) by a delta.
    TiltBy(f32),
    /// Toggle height-by-degree (hubs float above the ground plane).
    ToggleHeightByDegree,
    /// Play/pause the layout physics. Global and orthogonal to the
    /// arrangement: any arrangement composes with either state (a paused
    /// Spiral holds its placement, a running one relaxes from it), and
    /// force-directed is simply no arrangement with physics running.
    TogglePhysics,
    /// Toggle size-by-recency: newest content reads largest, older shrinks.
    /// Pairs with the Spiral (newest at center, age spiralling outward) —
    /// projection-engine proof 3, the recency channel.
    ToggleSizeByRecency,
    /// Persist the session now (close path; enrichment saves ride effects).
    SaveSession,
    /// Prompt for a place name, then found one. The founding half of the
    /// vocabulary follows the rename shape: `Begin*` opens the omnibar, the
    /// commit action below carries the typed line.
    BeginFoundPlace,
    /// Found a new place in this session and open it listen-only.
    FoundPlace {
        name: String,
    },
    /// Prompt for the path a place card is written to.
    BeginExportPlaceCard,
    /// Write this place's card: binding, founder root, current rendezvous.
    /// Carries no authority and no key material.
    ExportPlaceCard {
        path: String,
    },
    /// Refresh only contact hints from a card for the already joined place.
    BeginRefreshPlaceContact,
    RefreshPlaceContact {
        path: String,
    },
    ReconnectPlaceWithCard {
        session: crate::panes::SessionId,
        generation: u64,
        card: Box<crate::place::PlaceCardV1>,
    },
    /// Prompt for the card path a pre-key is offered against.
    BeginOfferPlacePrekey,
    /// Publish this profile's group identity for the card's Moot.
    OfferPlacePrekey {
        path: String,
    },
    /// The card read back by the shell, with the offer's output path.
    OfferPlacePrekeyForCard {
        card: Box<crate::place::PlaceCardV1>,
        out: String,
    },
    /// Prompt for the pre-key offer an invitation answers.
    BeginInviteToPlace,
    /// The same prompt, for an invitation that admits a reader.
    BeginInviteToPlaceAsReader,
    /// Admit one offered pre-key's root and author its invitation.
    InviteToPlace {
        path: String,
        access: crate::place::PlaceInviteAccess,
        lifetime_ms: Option<u64>,
    },
    /// The offer read back by the shell, with the invitation's output path.
    InviteToPlaceWithPrekey {
        prekey: Vec<u8>,
        out: String,
        access: crate::place::PlaceInviteAccess,
        lifetime_ms: Option<u64>,
    },
    /// Remove one member from the open place and revoke its grants.
    RevokePlaceMember {
        member: [u8; 32],
    },
    /// Prompt for the invitation file to join through.
    BeginJoinPlaceFile,
    /// Read and validate one invitation file, then join through it.
    JoinPlaceFile {
        path: String,
    },
    /// Prompt for a message body on the active place's default channel.
    BeginSendPlaceMessage,
    /// Admit an invitation into the current session, then open the place.
    ///
    /// The envelope carries no authority. Nothing durable exists for this place
    /// until every admission check has answered, so this is a request to try,
    /// not a statement that the session now belongs to a place.
    JoinPlace(Box<crate::place::invite::PlaceInviteV1>),
    /// Leave the active place locally, detaching its binding while preserving
    /// retained stores and shared membership records.
    LeavePlace,
    /// Dial saved contact hints for the currently admitted place.
    ReconnectPlace,
    /// Reconnect a locally left place, clearing its left mark. Valid only
    /// from `PlaceState::Left`.
    RejoinPlace,
    /// Inspect the current place and request fresh local sync observations.
    ShowPlaceStatus,
    /// Put this bind's full rendezvous ticket(s) on the system clipboard. The
    /// status row elides the middle of a ~200-character ticket; this is how the
    /// whole of it leaves the app.
    CopyLocalRendezvous,
    /// Send one message to a channel of the active place.
    SendPlaceMessage {
        channel: String,
        body: String,
    },
    /// Share the focused node's address into the place's shared graph.
    ShareFocusedNode,
    /// Re-fold the active place's projections from what its lanes have drained
    /// in. Cheap and idempotent; it authors nothing.
    ResyncPlace,
    /// Scope captured-page search to one exact Gemot collection version.
    /// `None` clears the local choice and searches all effective contributions.
    SetPlaceCollection(Option<crate::place::PlaceCollectionVersion>),
    /// Open the local captured-page search field for the active place.
    SearchCapturedPages,
    /// Flip the focused node's live content: spawn a document session for it
    /// through the content port, or close the one it has (rung 4; the
    /// session-engines plan's phase-4 consumer intent).
    ToggleNodeContent,
    /// Summon the omnibar (`command` pre-seeds the `>` actions lane).
    OmnibarOpen {
        command: bool,
    },
    /// Dismiss the omnibar without committing.
    OmnibarClose,
    /// Insert one typed character at the caret.
    OmnibarChar(char),
    /// Insert a string at the caret (an IME commit; later, paste).
    OmnibarInsert(String),
    /// Delete the character before the caret.
    OmnibarBackspace,
    /// Delete the character after the caret.
    OmnibarDelete,
    /// Move the caret within the omnibar text.
    OmnibarCaret(CaretMove),
    /// Move the suggestion highlight by a delta (wraps at the ends).
    OmnibarMove(i32),
    /// Commit the highlighted suggestion (or literal-go on address-shaped
    /// text with nothing highlighted).
    OmnibarCommit,
    /// Commit the suggestion row at this index — a ROW CLICK in the retained
    /// chrome (select, then the ordinary commit path).
    OmnibarCommitRow(usize),
    /// Expand the bare `>` lane to the whole catalog ("All commands…").
    OmnibarShowAllCommands,
    /// Keep or drop the command on the suggestion row at this index (its
    /// Keep/Drop control).
    OmnibarToggleKeepRow(usize),
    /// Keep or drop the command on the highlighted row (Ctrl+D).
    OmnibarToggleKeepSelected,
    /// Repeat one intentional shell interaction from the bounded local
    /// transcript, preserving its captured pane context.
    RepeatShellEntry(crate::shell_services::ShellEntryId),
    /// Ask the focused-context router to reopen the original target captured
    /// by a transcript entry. A2 supplies the multi-graph router.
    OpenShellEntryTarget(crate::shell_services::ShellEntryId),
    /// Summon a pane beside the active one, splitting the frisket tree (rung 5
    /// slice C). Meerkat's fixed Right-split off the graph pane, generalized to
    /// the active pane.
    SummonPane(PaneKindId),
    /// Ask over this browsing session through the resident inference route.
    OpenAnswers,
    NewResearchNote,
    OpenResearchNote,
    ChooseResearchNote {
        choice: crate::app::research_note_arms::NoteChoice,
    },
    CommitResearchNote {
        choice: crate::app::research_note_arms::NoteChoice,
    },
    ResearchNoteCreated {
        generation: uuid::Uuid,
        address: String,
    },
    SetAnswerSelection {
        generation: uuid::Uuid,
        expected: crate::inference_surface::AnswerSourceV2,
        chosen: Vec<crate::research::Target>,
    },
    /// Ask the platform shell to choose a local Knot document. The resulting
    /// path becomes a contributed source only after the user chooses it.
    ChooseKnotDocumentFile {
        read_only: bool,
    },
    /// Explicit file selection for retained content in the current mere.
    ChooseClipPackageFile,
    RetainProducerSelection {
        choice: crate::app::research_note_arms::NoteChoice,
        selection: crate::producer_retention::ProducerSelection,
    },
    ImportClipPackage(std::path::PathBuf),
    ChooseClipExportFile,
    ExportClipPackage {
        clip: eidetic::ManifestId,
        path: std::path::PathBuf,
        replace: bool,
    },
    /// Summon a provider-owned pane beside the active pane. The source was
    /// minted by the caller; provider admission still occurs at the render
    /// boundary, while the app preserves this exact durable value.
    SummonContributedPane {
        kind: PaneKindId,
        source: PaneSource,
    },
    /// Close the active pane, collapsing its split back into its sibling.
    CloseActivePane,
    /// Shared outer-frame intent; exact window and pane identity.
    OuterWorkspace {
        space: SpaceRef,
        event: workbench::WorkspaceEvent,
    },
    /// Set the divider ratio of the active pane's split (drag the seam). Clamped
    /// by the geometry walker so neither side collapses.
    SetActivePaneDivider(f32),
    /// Set a split's ratio by its path — the divider drag's lowering. Redraw
    /// only; the shell saves the session once, on release. `space` names the
    /// tree the path walks (a lens's seam drags reweight the LENS's split).
    SetSplitRatio {
        space: SpaceRef,
        path: Vec<crate::panes::SplitChoice>,
        ratio: f32,
    },
    /// Toggle maximize on the active pane (a host view state; frisket has no
    /// maximize op). A maximized pane takes the whole pane area.
    ToggleMaximizePane,
    /// Add or remove a composed list section on a pane, by provider id (the
    /// gloss-composite's add/remove, offered as pane-scoped palette rows).
    /// The choice rides the pane's frisket leaf, so it persists with
    /// `frame.json` and moves with the pane on tear-out.
    TogglePaneSection {
        pane: crate::panes::PaneId,
        section: String,
    },
    /// Move a composed section earlier (`-1`) or later (`+1`) in a pane's
    /// stack. Composition ORDER is the config's order, so reordering is the
    /// same leaf edit; clamped at the ends (no wrap — a stack has a top).
    MovePaneSection {
        pane: crate::panes::PaneId,
        section: String,
        delta: i32,
    },
    /// Open the focused node as a workbench tile (rung 5 slice E): summons the
    /// Workbench pane if absent, opens the tile in platen's model, and spawns
    /// the node's content if it has none.
    OpenInWorkbench,
    /// Primary reading and graph views keep the same document identity.
    SetWorkspaceMap(bool),
    ReadGraphMember(uuid::Uuid),
    NavigateRelationship(crate::relationship_navigation::RelationshipTarget),
    RevealRelationship(crate::relationship_navigation::RelationshipTarget),
    SetPaneBinding(crate::pane_binding::BindingIntent),
    /// Reveal an existing graph member without changing its reading arrangement.
    RevealGraphMember(uuid::Uuid),
    /// Hide retains the pane in an inactive stack; show restores its split.
    ToggleToolPane(PaneKindId),
    SetChromeLayout(crate::chrome_layout::ChromeLayout),
    /// Tear a tile OUT of the workbench — the tab drag released outside the
    /// pane. The tile leaves platen's tiling and becomes a pinned
    /// `PaneContent::Tile` pane in a lens window: the tear-out trichotomy's
    /// BRANCH arm, gesture-first (the leaf arm is `TearOutActivePane`).
    TearOutTile {
        member: uuid::Uuid,
    },
    /// Fork the connected component containing this node into a freshly
    /// minted session (the tear-out trichotomy's FORK arm, tear-out brief
    /// §4.3): new SessionId + GraphId, a `picked_from` record,
    /// `CopiedFrom` provenance per node, per-node character carried by facets.
    /// Gesture-first (Ctrl+Shift at the tab drag-out); the palette arm is
    /// `ForkFocusedNode`.
    ForkNode {
        member: uuid::Uuid,
    },
    /// Fork from the focused node — the palette / keyboard arm of `ForkNode`.
    ForkFocusedNode,
    /// Mint a fresh session (rung 6's second half): a new manifest under
    /// `sessions/<id>/`, then switch to it. The old session saves on the way
    /// out; the new one starts on an empty graph.
    NewSession,
    /// Switch to an existing session by id. The switcher lane (omnibar `>`)
    /// offers one of these per other session, labelled.
    SwitchSession(crate::panes::SessionId),
    /// Close the current session: trash its directory + manifest, then switch
    /// to the most-recent remaining session (minting one if it was the last).
    CloseSession,
    /// Open the omnibar in rename mode for the current session, seeded with its
    /// current label — the free-text prompt behind [`Action::RenameSession`].
    BeginRenameSession,
    /// Set a session's display name (the rename mode's commit). An empty name
    /// clears it back to the derived/uuid label.
    RenameSession {
        id: crate::panes::SessionId,
        name: String,
    },
    /// Remove the focused node from the graph ("forget this page"): its record
    /// stages into the recycle bin (the eidetic deleted-node bin, through the
    /// bin port) and the node leaves the graph. Recoverable from the Trail's
    /// Removed section until athanor permanently forgets it. Closes its live
    /// content and any workbench tile.
    DeleteFocusedNode,
    /// Recover a staged node from the recycle bin BY ITS ORIGINAL member id
    /// (a Trail Removed-row click): the node re-mints under the same uuid
    /// (identity restored), with its recorded title and tags.
    RecoverDeletedNode(uuid::Uuid),
    /// Permanently forget every staged node ("empty the recycle bin") —
    /// athanor's oven, on command. Irreversible; the records leave the store.
    EmptyRecycleBin,
    /// Stage a scenario pack (.lua) as a participant install: read + derive the
    /// content subject, then surface the VISIBLE grant review in the palette
    /// (participant gate B1). Nothing is minted or granted here.
    InstallDenizen {
        path: String,
    },
    /// Commit the staged install after the visible review: mint the participant
    /// node + binding facets, project the grant into its nested world through
    /// the servitor gate, and register the palette Run row.
    ConfirmInstallDenizen,
    /// Discard the staged install; nothing was minted.
    CancelInstallDenizen,
    /// Uninstall a resident participant: REVOKE the delegations the user granted
    /// it (cascading to anything it delegated onward) and un-reside it — the
    /// binding facet goes, the runtime entry goes. Its node and world stay,
    /// un-resided, so nothing is destroyed by revoking authority.
    UninstallDenizen {
        member: uuid::Uuid,
    },
    /// Run a resident participant's scenario body: piccolo evaluates it under a
    /// step budget, and its emitted Actions lower through this same spine
    /// with mere's GraphJournal scoped to the participant's author (attribution).
    RunDenizen {
        member: uuid::Uuid,
    },
    /// Write `body` as the content of the node at `url`, minting the node if
    /// it is not there. The authoring lane a summarizing behavior needs; the
    /// `Author` ring gates it, and that ring is never preselected.
    WriteNote {
        url: String,
        body: String,
    },
    /// Restore a trashed SESSION from the manifest trash and switch to it
    /// (overmap O3; a Trail Removed-sessions-row click). The whole session
    /// directory moved to `.trash/` intact at close, so restore is
    /// same-identity by construction.
    RecoverSession(crate::panes::SessionId),
    /// Make `member`'s tab the active (visible) one in its workbench cell.
    WorkbenchActivate(uuid::Uuid),
    /// Close the focused node's workbench tile (its cell collapses when
    /// emptied). A no-op when the focused node has no tile.
    CloseWorkbenchTile,
    /// Stack `dragged` into the cell holding `target` — the tab-drag gesture's
    /// lowering (platen's `move_to_slot_of`).
    WorkbenchStackOnto {
        dragged: uuid::Uuid,
        target: uuid::Uuid,
    },
    /// Split `dragged` out as its own cell beside `target`, on the `after`
    /// side of `axis` — the edge-drop half of the tab-drag gesture (platen's
    /// `split_beside_axis`).
    WorkbenchSplitBeside {
        dragged: uuid::Uuid,
        target: uuid::Uuid,
        axis: WbAxis,
        after: bool,
    },
    /// Split `dragged` out of its OWN cell onto that cell's edge — a tab
    /// dragged to an edge of the stack it lives in (platen's `split_out`).
    WorkbenchSplitOut {
        dragged: uuid::Uuid,
        axis: WbAxis,
        after: bool,
    },
    /// Set the fractions of the workbench split at `path` — a workbench
    /// divider drag. Redraw only; the shell saves once, on release.
    WorkbenchSetFractions {
        path: Vec<usize>,
        fractions: Vec<f32>,
    },
}
