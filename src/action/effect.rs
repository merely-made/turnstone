// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A side effect `update` asks the shell to run through a port. `update`
/// itself never blocks and never touches a platform API.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    ResearchSearch(crate::research_search::Request),
    CancelResearchSearch,
    ResearchCompare(crate::research_compare::Request),
    SaveReadingPreferences,
    RestoreReadingAnchor {
        target: crate::reading_view::ReadTarget,
        anchor: tabard::ReadingAnchor,
    },
    ReadFind {
        target: crate::reading_view::ReadTarget,
        query: String,
        step: Option<inker::DocumentFindDirection>,
        close: bool,
    },
    /// Persist curation for the resolved graph runtime and its session.
    SaveGraphFolds {
        graph: crate::panes::GraphId,
    },
    /// Open/focus the host-owned workshop on the existing render core.
    OpenThemeWorkshop,
    OpenAlembicWorkshop,
    /// Fetch a page document through the fetch actor, for enrichment of the
    /// node that requested it (correlation-over-URLs: several nodes may
    /// share an address, and a node may navigate away mid-flight).
    FetchPage {
        /// Process-local request identity, echoed by the fetch actor.
        request: fetch::FetchRequestId,
        /// An older request for this node that this one replaces.
        supersedes: Option<fetch::FetchRequestId>,
        node: uuid::Uuid,
        /// The address sent to the network.
        url: String,
        /// The graph address that owns the answer. Usually identical to
        /// `url`; sensitive Gemini input keeps its query out of graph truth.
        owner_url: String,
        /// A capsule-scoped client certificate selected by the host. The
        /// fetch actor still rechecks its origin on redirects.
        identity: Option<fetch::GeminiClientIdentity>,
    },
    /// Abort one exact actor-backed page request.
    CancelPage {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
    },
    /// Fetch a subscribed source without treating its response as page
    /// enrichment. It uses the same actor and capsule-scoped identity path.
    FetchFeed {
        node: uuid::Uuid,
        url: String,
        identity: Option<fetch::GeminiClientIdentity>,
    },
    /// Perform one confirmed smolweb mutation exactly once.
    SubmitSmolweb {
        request: u64,
        source: Option<uuid::Uuid>,
        target: String,
        protocol: crate::ui::SmolwebSubmissionProtocol,
        body: Vec<u8>,
        mime: String,
        token: Option<SensitiveString>,
        identity: Option<fetch::GeminiClientIdentity>,
    },
    /// Replace one durable Gemini server pin after an explicit human decision.
    /// The shell executes this synchronously before the following fetch effect.
    ReplaceGeminiTrust {
        node: uuid::Uuid,
        fetch_url: String,
        owner_url: String,
        target: String,
        pinned: String,
        seen: String,
    },
    /// Fetch a favicon (already-absolute `url`) for `node`, whose page lives
    /// at `owner_url` (the staleness check compares against it on return).
    FetchFavicon {
        node: uuid::Uuid,
        owner_url: String,
        url: String,
    },
    /// Persist one content-addressed image blob under its digest hex.
    /// The node already carries the reference and the canvas already has the
    /// decoded pixels, so this is durability only: dropping it costs a
    /// re-fetch, never correctness.
    StoreImage {
        hex: String,
        bytes: Vec<u8>,
    },
    /// Deposit a completed response in the session's representation store and
    /// write a user-visible file. The graph node remains identified by `url`;
    /// the destination returned by the shell is metadata only.
    StoreDownload {
        node: uuid::Uuid,
        url: String,
        content_type: Option<String>,
        content_disposition: Option<String>,
        received_at_ms: u64,
        bytes: Vec<u8>,
    },
    /// Persist the session through the persistence port.
    SaveSession,
    SaveChromeLayout(crate::chrome_layout::ChromeLayout),
    /// Put `text` on the system clipboard. The clipboard itself stays behind
    /// the shell; neither the app nor the place worker holds one.
    CopyText(String),
    /// Ask the native shell for one Djot or Knot file. A cancelled picker has
    /// no effect; an accepted path is minted into a provider-owned source.
    ChooseKnotDocumentFile {
        read_only: bool,
    },
    CreateResearchNote {
        generation: uuid::Uuid,
    },
    ChooseResearchNote {
        choice: crate::app::research_note_arms::NoteChoice,
    },
    ChooseClipPackageFile,
    RetainProducerSelection {
        choice: crate::app::research_note_arms::NoteChoice,
        selection: crate::producer_retention::ProducerSelection,
    },
    ImportClipPackage(std::path::PathBuf),
    ChooseClipExportFile {
        clip: eidetic::ManifestId,
    },
    ExportClipPackage {
        clip: eidetic::ManifestId,
        path: std::path::PathBuf,
        replace: bool,
    },
    ControlRetainedClip(crate::retained_clip::ClipRequest),
    /// Open this session's retained place domains through the shell-owned
    /// worker. `generation` makes every later answer session-specific.
    OpenPlace {
        session: crate::panes::SessionId,
        generation: u64,
        binding: crate::place::PlaceBindingV1,
    },
    /// Found a place in this session's directory and open it listen-only.
    FoundPlace {
        session: crate::panes::SessionId,
        generation: u64,
        name: String,
    },
    /// Publish this profile's group identity for one Moot.
    OfferPlacePrekey {
        session: crate::panes::SessionId,
        generation: u64,
        moot: [u8; 32],
    },
    /// Admit one offered pre-key's root and author its invitation.
    InvitePlace {
        session: crate::panes::SessionId,
        generation: u64,
        prekey: Vec<u8>,
        access: crate::place::PlaceInviteAccess,
        lifetime_ms: Option<u64>,
    },
    /// Write one place artifact where the person asked for it. Files are a
    /// shell concern; the worker never touches the filesystem outside the
    /// session's own store.
    WritePlaceArtifact {
        path: String,
        bytes: Vec<u8>,
    },
    /// Read one place artifact and lower what it holds back into an action.
    ReadPlaceArtifact {
        path: String,
        kind: PlaceArtifactKind,
    },
    /// Admit one invitation, then open the place it names.
    ///
    /// Boxed because the envelope carries inline artifacts and every `Effect`
    /// would otherwise pay for the largest variant.
    JoinPlace {
        session: crate::panes::SessionId,
        generation: u64,
        invite: Box<crate::place::invite::PlaceInviteV1>,
    },
    /// Send one confirmed native Micron form map.  The shell owns the
    /// Reticulum link; app state only retains a redacted envelope.
    SubmitMicron {
        request: u64,
        source: Option<uuid::Uuid>,
        submission: MicronSubmission,
    },
    CancelMicronSubmission {
        request: u64,
    },
    /// Release the place worker, then mark this session's binding locally
    /// left: the binding, rendezvous hints and retained stores all stay in
    /// place, so `Rejoin` has what it needs.
    LeavePlace {
        session: crate::panes::SessionId,
        generation: u64,
        retained: crate::place::PlaceLeftSummary,
    },
    ReconnectPlace {
        session: crate::panes::SessionId,
        generation: u64,
        binding: crate::place::PlaceBindingV1,
        contact: Option<Box<crate::place::PlaceCardV1>>,
    },
    /// Clear a local left mark. Fired alongside `ReconnectPlace` by
    /// `rejoin_place`, never on its own.
    ClearPlaceLeftMark {
        session: crate::panes::SessionId,
    },
    /// Author one fact into the active place and publish it to live peers.
    RunPlaceCommand {
        session: crate::panes::SessionId,
        generation: u64,
        request: u64,
        command: crate::place::worker::PlaceCommand,
    },
    /// Re-fold the active place's projections without touching its lanes.
    ResyncPlace {
        session: crate::panes::SessionId,
        generation: u64,
    },
    /// Change the open place's local captured-search scope and remint it.
    SetPlaceCollection {
        session: crate::panes::SessionId,
        generation: u64,
        selection: Option<crate::place::PlaceCollectionVersion>,
    },
    /// Release any retained place handles. The shell waits for acknowledgement
    /// on switch, trash, and shutdown before touching the session directory.
    ClosePlace {
        session: crate::panes::SessionId,
        generation: u64,
    },
    /// Spawn a live document session for `node` at `url` through the
    /// content port (registry-dispatched once genet-documents lands;
    /// until then the port answers with an honest ContentFailed).
    SpawnContent {
        node: uuid::Uuid,
        url: String,
    },
    /// Replace the body of an already-live incremental smolweb session.
    UpdateContent {
        node: uuid::Uuid,
        url: String,
    },
    /// Close `node`'s live session; the port drops the handle.
    CloseContent {
        node: uuid::Uuid,
    },
    /// Drive the optional web control plane of a live surface producer.
    ControlContent {
        node: uuid::Uuid,
        control: ContentControl,
    },
    /// Apply `scale` as one live surface's page zoom (`1.0` = 100%). The
    /// requested value is already persisted when this runs; a refusal from the
    /// engine changes what the document shows, never what the node stores.
    ScaleContent {
        node: uuid::Uuid,
        scale: f32,
    },
    /// Start one host-correlated viewport capture of this live web surface.
    CaptureContent {
        node: uuid::Uuid,
    },
    /// Preserve one exact, app-held fetched HTML response through the session
    /// Eidetic store, bound to the request's owner and acquired response.
    CaptureSourceDocument {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
    },
    PersistSourceCapture {
        request: uuid::Uuid,
    },
    LoadSavedSources {
        generation: uuid::Uuid,
        request: uuid::Uuid,
        session: crate::panes::SessionId,
        admissions: Vec<crate::saved_sources::Admission>,
    },
    ReadSavedSource {
        generation: uuid::Uuid,
        request: uuid::Uuid,
        node: uuid::Uuid,
        url: String,
        target: crate::saved_sources::Target,
        admission: crate::saved_sources::Admission,
    },
    /// Ask one exact live content owner to replace or step its retained find.
    FindContent {
        node: uuid::Uuid,
        request: u64,
        query: String,
        direction: DocumentFindDirection,
        find_next: bool,
    },
    ClearContentFind {
        node: uuid::Uuid,
    },
    /// Answer one exact held web-permission callback. The app captures the
    /// surface-scoped request identity in the action and the shell owns the
    /// backend callback plus durable profile registry.
    AnswerPermissionRequest {
        request: crate::user_agent_decision::UserAgentRequestKey,
        answer: inker::PermissionAnswer,
        retention: crate::web_policy::PermissionRetention,
    },
    /// Answer or cancel one exact HTTP authentication challenge. Credential
    /// values are redacted in Debug and remain process-local.
    AnswerAuthenticationRequest {
        request: crate::user_agent_decision::UserAgentRequestKey,
        credentials: Option<(SensitiveString, SensitiveString)>,
        remember_for_process: bool,
    },
    /// Open a lens window (platform work: window + surface creation) showing
    /// the pane space the app seeded at `App::lenses[ordinal]`.
    OpenWindow {
        ordinal: usize,
    },
    /// Switch the live session (port work: the shell saves the departing
    /// session, tears down its live ports — content sessions, lens windows —
    /// then has the app adopt `id` and runs the adoption's own effects).
    SwitchSession {
        id: crate::panes::SessionId,
    },
    /// Stage a removed node's record into the recycle bin (the bin port's
    /// actor persists it in the session's eidetic store and answers with the
    /// refreshed list).
    RecordDeleted {
        record: RemovedRecord,
    },
    /// Permanently forget every staged node — the bin actor clears its store
    /// and answers with the empty list ("empty the recycle bin").
    EmptyRecycleBin,
    /// Ask the trail port for lexical recall over this session's browsing
    /// memory (the omnibar's recall lane). Answered by `Update::RecallHits`
    /// carrying the query back, so a late answer to superseded text drops.
    RecallQuery {
        query: String,
    },
    /// Hand the trail port a fetched page's extracted main text, so recall
    /// reaches the body and not just the title and URL (wiring plan W6c).
    /// Best-effort like every other enrichment: dropping it costs recall
    /// reach, never correctness.
    RecordPageText {
        url: String,
        text: String,
    },
    /// Close a session (overmap O3): the shell releases the bin store (its
    /// open files block the rename on Windows), moves the closing session's
    /// whole directory to the manifest trash via `App::apply_trash`, and
    /// adopts `next` WITHOUT the departing save (a trashed session must not
    /// be resurrected as a zombie directory by a post-trash save).
    TrashSession {
        closing: crate::panes::SessionId,
        next: crate::panes::SessionId,
    },
    /// The projection is stale; present another frame.
    Redraw,
}
