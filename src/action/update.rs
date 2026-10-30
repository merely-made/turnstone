// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A typed service answer, drained by the shell on wake and folded back into
/// state through [`crate::app::apply_update`]. App-owned types only; port
/// adapters convert.
pub enum Update {
    ResearchRecords(crate::research_records::JobResult),
    ResearchCompareFinished {
        request: crate::research_compare::Request,
        result: Result<crate::research_compare::ResultView, String>,
    },
    ResearchSearchFinished {
        request: crate::research_search::Request,
        result: Result<eidetic_search::query::SearchResult, String>,
    },
    ReadingFindChanged {
        target: crate::reading_view::ReadTarget,
        query: String,
        result: Result<inker::DocumentFindState, String>,
    },
    SavedSourcesLoaded {
        generation: uuid::Uuid,
        request: uuid::Uuid,
        session: crate::panes::SessionId,
        admissions: Vec<crate::saved_sources::Admission>,
        result: Result<Vec<crate::saved_sources::Entry>, String>,
    },
    SavedSourceRead {
        generation: uuid::Uuid,
        request: uuid::Uuid,
        node: uuid::Uuid,
        url: String,
        target: crate::saved_sources::Target,
        result: Result<crate::saved_sources::Content, String>,
    },
    /// Exact Gemini response bytes arrived before the terminal fetch answer.
    PageStreamed {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
        url: String,
        response_url: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    },
    /// A page fetch completed (successfully or not) for `node`, which
    /// requested `url` (enrichment applies only while the node still lives
    /// there — a late result against a superseded node drops explicitly).
    PageFetched {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
        url: String,
        result: Result<FetchedPage, String>,
    },
    /// A subscribed feed refresh completed through the ordinary fetch port.
    FeedFetched {
        node: uuid::Uuid,
        url: String,
        result: Result<FetchedPage, String>,
    },
    SmolwebSubmitted {
        request: u64,
        source: Option<uuid::Uuid>,
        target: String,
        result: Result<SmolwebSubmissionReceipt, String>,
    },
    /// A smolweb page fetch reached a protocol input response. `url` is the
    /// member's requested address; `input_url` is the final redirect target
    /// whose query the answer must replace.
    SmolwebInputRequested {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
        url: String,
        input_url: String,
        prompt: String,
        sensitive: bool,
    },
    /// A Gemini capsule requires a client certificate. `identity_url` is the
    /// final redirect target and therefore the capsule origin the identity
    /// must be scoped to.
    GeminiIdentityRequested {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
        url: String,
        identity_url: String,
        prompt: String,
    },
    /// A Gemini TLS certificate differs from durable trust. The request was
    /// refused before application bytes and waits for an explicit decision.
    GeminiCertificateChanged {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
        url: String,
        fetch_url: String,
        target: String,
        pinned: String,
        seen: String,
    },
    /// The fetch actor confirmed that an exact page request was cancelled.
    PageStopped {
        request: fetch::FetchRequestId,
        node: uuid::Uuid,
        url: String,
    },
    /// A favicon's raw bytes arrived for `node`, requested while its page
    /// was `owner_url`.
    FaviconFetched {
        node: uuid::Uuid,
        owner_url: String,
        bytes: Vec<u8>,
    },
    /// The shell deposited a download and attempted its user-visible copy.
    DownloadStored {
        node: uuid::Uuid,
        url: String,
        content_type: Option<String>,
        content_disposition: Option<String>,
        received_at_ms: u64,
        byte_size: u64,
        result: Result<StoredDownload, String>,
    },
    /// The content port spawned a live session for `node`. `facts` carries
    /// the spawn-time mirror (engine id, the structural read's summary) in
    /// app-owned terms — the adapter converts the service's report type at
    /// the boundary, like every other port answer.
    ContentSpawned {
        node: uuid::Uuid,
        facts: Option<crate::content::ContentFacts>,
    },
    DocumentFindChanged {
        node: uuid::Uuid,
        request: u64,
        query: String,
        result: Result<DocumentFindModel, String>,
    },
    PermissionRequested {
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        origin: String,
        descriptors: Vec<inker::PermissionDescriptor>,
    },
    AuthenticationRequested {
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        host: String,
        port: u16,
        realm: Option<String>,
        scheme: String,
        is_proxy: bool,
    },
    PermissionRequestFinished {
        request: crate::user_agent_decision::UserAgentRequestKey,
        answer: inker::PermissionAnswer,
        retention: crate::web_policy::PermissionRetention,
        terminal: bool,
        result: Result<(), String>,
    },
    AuthenticationRequestFinished {
        request: crate::user_agent_decision::UserAgentRequestKey,
        supplied_credentials: bool,
        remember_for_process: bool,
        terminal: bool,
        result: Result<(), String>,
    },
    UserAgentRequestWithdrawn {
        request: crate::user_agent_decision::UserAgentRequestKey,
        kind: &'static str,
        reason: &'static str,
    },
    /// Source held by the host and accepted after a real controller opened.
    ContentSourceCaptured {
        node: uuid::Uuid,
        url: String,
        document: crate::content::FetchedDocument,
    },
    /// The content port could not spawn (or lost) `node`'s session.
    ContentFailed {
        node: uuid::Uuid,
        error: String,
    },
    /// The recycle bin's current contents (the bin port answers every record
    /// / reopen with the refreshed list, and emits one on spawn). Replaces the
    /// app's cache wholesale.
    BinListed {
        records: Vec<RemovedRecord>,
    },
    /// The bin store failed (open / record / list) — loud and attributable,
    /// never an empty list masquerading as "nothing deleted".
    BinFailed {
        error: String,
    },
    /// Lexical recall over browsing memory answered. `query` is the text the
    /// hits answer; the app drops an answer whose query is no longer what the
    /// omnibar holds (the keystroke that superseded it wins).
    RecallHits {
        query: String,
        hits: Vec<RecallHit>,
    },
    /// Recall could not answer (no index yet, a re-mint failure, a broken
    /// store). Loud: a recall lane that silently shows nothing is
    /// indistinguishable from a trail with nothing in it.
    RecallFailed {
        error: String,
    },
    /// The session Eidetic actor completed an explicit source-document capture.
    SourceDocumentCaptured {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
        result: Result<StoredSourceDocument, String>,
    },
    SourceCaptureRecovered {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
        stored: Option<StoredSourceDocument>,
    },
    SourceCaptureSaved {
        capture_generation: uuid::Uuid,
        request: uuid::Uuid,
        result: Result<(), String>,
    },
    SourceCaptureRecoveryFailed {
        capture_generation: uuid::Uuid,
        error: String,
    },
    SourceCaptureRevalidated {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
        stored: StoredSourceDocument,
    },
    /// One retained-place open completed. The app accepts it only while both
    /// the session and generation still match its active opening.
    PlaceOpened {
        session: crate::panes::SessionId,
        generation: u64,
        result: Result<crate::place::OfflinePlaceSnapshot, String>,
    },
    /// A dial to the mere holding a place-held Knot document is prepared, or
    /// could not be. The shell opens it on a visiting thread of its own; the
    /// app only learns that a visit is open or why it was refused.
    // The dial is a crate-internal handle in a crate-internal module; this
    // vocabulary is public only because everything in it is.
    #[allow(private_interfaces)]
    PlaceDocumentVisit {
        session: crate::panes::SessionId,
        generation: u64,
        request: u64,
        holder_root: [u8; 32],
        path: String,
        result: Result<Box<crate::place::lanes::HolderDial>, String>,
    },
    /// The place's live lanes accepted operations and have settled.
    ///
    /// A nudge, not a projection: the watcher samples counters and cannot
    /// apply the authority filter, which belongs with the stores. The app
    /// answers by asking for a resync, so what arrived becomes visible through
    /// exactly the same fold every other path uses.
    PlaceLanesAdvanced {
        session: crate::panes::SessionId,
        generation: u64,
    },
    /// One authored place command completed.
    ///
    /// Success carries the re-folded snapshot, because authoring changes what
    /// the place projects and the app should never keep a stale view of state
    /// it just changed. Failure changes no state: an unauthorized or malformed
    /// command is reported, not absorbed.
    PlaceCommandDone {
        session: crate::panes::SessionId,
        generation: u64,
        request: u64,
        result: Result<crate::place::OfflinePlaceSnapshot, String>,
    },
    /// A local captured-search collection choice was resolved and reminted.
    /// Failure retains the prior snapshot because it authored nothing.
    PlaceCollectionSet {
        session: crate::panes::SessionId,
        generation: u64,
        result: Result<crate::place::OfflinePlaceSnapshot, String>,
    },
    /// One invitation admission completed.
    ///
    /// One place was founded here, and is open listen-only.
    ///
    /// Distinct from [`Update::PlaceJoined`]: nothing was admitted, because
    /// there was nothing to be admitted into until this succeeded.
    PlaceFounded {
        session: crate::panes::SessionId,
        generation: u64,
        result: Result<
            (
                crate::place::PlaceBindingV1,
                crate::place::OfflinePlaceSnapshot,
            ),
            String,
        >,
    },
    /// This profile's published group pre-key for one Moot. An offer, not an
    /// admission: holding it grants the holder nothing.
    PlacePrekeyOffered {
        session: crate::panes::SessionId,
        generation: u64,
        result: Result<Vec<u8>, String>,
    },
    /// One authored invitation for an offered pre-key.
    PlaceInvited {
        session: crate::panes::SessionId,
        generation: u64,
        result: Result<Box<crate::place::invite::PlaceInviteV1>, String>,
    },
    /// Distinct from [`Update::PlaceOpened`] because the failure means
    /// something different: an open that fails leaves a degraded but real
    /// place, while an admission that fails means there is no place at all.
    /// The binding travels in the success arm because until admission returns
    /// it, the app has no admitted binding to name.
    PlaceJoined {
        session: crate::panes::SessionId,
        generation: u64,
        result: Result<
            (
                crate::place::PlaceBindingV1,
                crate::place::OfflinePlaceSnapshot,
            ),
            String,
        >,
    },
}
