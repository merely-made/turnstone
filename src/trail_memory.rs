// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The trail-memory port: browsing capture behind an armillary actor (the
//! search surface wiring plan's W1 slice).
//!
//! Capture IS `eidetic::browsing` — the shell drains the app's semantic
//! events each frame and forwards the navigation ones here; the actor owns a
//! session-scoped `muniment::FjallStore` at `sessions/<id>/memory`,
//! buffers traversals into per-owner `BrowsingTrace` segments through
//! [`eidetic::BrowsingMemory`], and flushes a segment when it fills and on
//! every lifecycle edge (switch, close, release). `from` chains inside the
//! actor: an owner's last destination is the next event's origin, and a
//! fresh actor starts at `None` (an origin event) rather than inventing one.
//!
//! Failures warn and drop the event rather than wedging navigation: capture
//! is an observer of browsing, never a gate on it — which is also why it
//! rides the observation drain instead of lowering an Effect.
//!
//! Legacy PageText records remain readable, but ordinary fetches no longer
//! write extracted bodies. L1 keeps address, title and traversal history;
//! source bytes enter durable storage only through explicit capture.
//!
//! The same actor answers **recall** (W2): the omnibar lowers
//! `Effect::RecallQuery`, and [`TrailCommand::Recall`] projects the corpus
//! through eidetic's fingerprint-keyed page table (W6d) — one record per page
//! however many addresses reached it, now carrying its stored body — then
//! mints BM25 over title, URL and text and a frecency fold over those records
//! (flushing first, so this minute's pages are findable), fuses the lanes that
//! are on, and answers `Update::RecallHits`. Current graph and recycle-bin titles overlay titleless
//! trace pages only while those derived indexes are minted; they do not rewrite
//! browsing history. The indexes are held here, never repaired — a corpus or
//! title projection that moved re-mints.
//! eidetic's concrete types stop at this boundary: the app sees `RecallHit`s,
//! the same rule the bin port follows with `DeletedNode`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use armillary::{ActorHandle, Emitter, Wake, spawn_named};
use eidetic::Backend as _;
use eidetic::{
    BrowsingMemory, BrowsingTrace, PageRef, PageTextStore, PageTexts, TraceEvent, TraceTransition,
    bootstrap_browsing_schema,
};
use eidetic_search::recall_mint::{
    MintObserver, MintPhase, MintedRecall, canonical_sources,
};
use muniment::FjallStore;

use crate::action::{RecallHit, StoredSourceDocument, Update};
pub(crate) mod record_actor;
#[cfg(test)]
#[path = "trail_memory/research_compare_tests.rs"]
mod research_compare_tests;
#[cfg(test)]
#[path = "trail_memory/research_search_tests.rs"]
mod research_search_tests;
mod saved_sources;
mod sealed_capture;
mod sealed_record_receipts;
mod sealed_records;
mod sealed_result_receipts;
mod structured_bounded_store;
#[cfg(test)]
#[path = "trail_memory/structured_owner_tests.rs"]
mod structured_owner_tests;
mod worker_capture_ops;
mod worker_open_sealed;
mod worker_research_snapshot;
mod worker_research_source;

/// Traversals per stored trace segment. Segments are the flush granularity:
/// small enough that a crash loses minutes, large enough that a stored trace
/// is a meaningful corridor slice.
const SEGMENT_SIZE: usize = 32;

pub use eidetic_search::recall_mint::{RecallConfig, RecallSource};

/// Commands the trail-memory actor takes (the shell sends these; ordering on
/// the one channel is the consistency story, the bin actor's exact shape).
/// The sealer is a short-lived capability tied to one host/profile generation.
/// The callback must report the same epoch while that exact binding remains
/// current; `None` or a different epoch withdraws publication rights.
#[derive(Clone)]
pub struct SourceCustody {
    pub sealer: std::sync::Arc<dyn eidetic::PayloadSealer + Send + Sync>,
    pub epoch: std::sync::Arc<dyn Fn() -> Option<u64> + Send + Sync>,
}

#[cfg(test)]
#[path = "trail_memory/custody_test_fixture.rs"]
pub(crate) mod custody_test_fixture;

pub enum TrailCommand {
    ResearchRecords(record_actor::Command),
    /// Ordered custody adoption or withdrawal; no key material is held here.
    SetCustody(Option<SourceCustody>),
    /// Acknowledge all prior custody work, including refreshing the shared
    /// captured-source library, before another owner opens its projections.
    CustodyReady(std::sync::mpsc::SyncSender<()>),
    LoadSavedSources {
        generation: uuid::Uuid,
        request: uuid::Uuid,
        session: crate::panes::SessionId,
        admissions: Vec<crate::saved_sources::Admission>,
        directory: PathBuf,
    },
    ReadSavedSource {
        generation: uuid::Uuid,
        request: uuid::Uuid,
        node: uuid::Uuid,
        url: String,
        target: crate::saved_sources::Target,
        admission: crate::saved_sources::Admission,
        directory: PathBuf,
    },
    ResearchSnapshot {
        context: crate::research::Context,
        selected: Option<Vec<crate::saved_sources::Key>>,
        max_chars: Option<usize>,
        max_metadata_chars: Option<usize>,
        budget: Option<crate::research_storage::SharedReadBudget>,
        reply: std::sync::mpsc::SyncSender<
            Result<Vec<crate::saved_sources::Entry>, crate::research_storage::SourceReadError>,
        >,
    },
    ResearchSource {
        context: crate::research::Context,
        target: crate::saved_sources::Target,
        budget: Option<crate::research_storage::SharedReadBudget>,
        reply: std::sync::mpsc::SyncSender<
            Result<crate::saved_sources::Content, crate::research_storage::SourceReadError>,
        >,
    },
    /// Read exact retained texts through this actor's existing store handle.
    InferenceSnapshot {
        owner: String,
        reply: std::sync::mpsc::SyncSender<
            Result<Vec<eidetic::models::recall::RetainedSourceText>, String>,
        >,
    },
    /// Resolve an immutable page-text version, never the current URL.
    InferenceSource {
        source: eidetic::models::recall::RecallSourceId,
        version: String,
        reply: std::sync::mpsc::SyncSender<Result<String, String>>,
    },
    /// Record one traversal for `owner` (an opaque persona tag — turnstone
    /// passes the root identity's public key hex).
    Record {
        owner: String,
        url: String,
        transition: TraceTransition,
        at_ms: u64,
    },
    /// Legacy automatic-text command retained for compatibility with queued
    /// work. New commands are deliberately ignored under lifecycle ruling L1.
    RecordText {
        url: String,
        text: String,
    },
    /// An explicit request to preserve one fetched HTML response. This remains
    /// separate from `RecordText`: recall text is never source evidence.
    CaptureSourceDocument {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
        directory: PathBuf,
        document: Option<crate::content::FetchedDocument>,
    },
    /// Commit the durable request receipt after owning facet/graph save ack.
    AcknowledgeSourceCapture {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
        directory: PathBuf,
    },
    /// Flush every open segment to the store (a lifecycle edge).
    Flush,
    /// Answer lexical recall over the stored corpus (the omnibar's recall
    /// lane). The query rides back on the answer so the app can drop a reply
    /// to text it has already typed past.
    Recall {
        query: String,
        limit: usize,
        sources: Vec<RecallSource>,
        config: RecallConfig,
    },
    /// Flush, re-point the store at another session's memory dir (a session
    /// switch), and restart origin chaining.
    Reopen {
        directory: PathBuf,
        capture_generation: uuid::Uuid,
    },
    /// Flush, drop the open store, and ack — the close path's handshake:
    /// Windows cannot rename a directory whose files are open, so the shell
    /// releases the memory store BEFORE moving the session dir to the trash.
    Release(std::sync::mpsc::SyncSender<()>),
}

#[cfg(test)]
#[path = "trail_memory/source_capture_tests.rs"]
mod source_capture_tests;

/// What one [`RecallIndex::mint`] covered and what it cost. Every recall after
/// a navigation re-mints from the whole corpus, so this is the price of the
/// derived-not-repaired doctrine — the measurement the engine decision (keep
/// tantivy, or an in-tree BM25) waits on. A receipt, not durable state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MintReceipt {
    /// Stored trace segments read.
    pub traces: usize,
    /// Traversal events across those segments.
    pub events: usize,
    /// Distinct pages by content fingerprint — one index document each.
    pub pages: usize,
    /// How many of those pages carried a stored body into the index (W6c).
    /// Zero on a corpus captured before text was kept, or refused by consent.
    pub pages_with_text: usize,
    /// Distinct destination addresses that folded into those pages: tracking
    /// parameters, fragments, mirrors. Zero means every address was its own
    /// page (the corpus the URL key would have produced).
    pub collapsed_urls: usize,
    /// Indexable text size: url + title bytes summed over every event. The
    /// page bodies are counted separately, by [`text_bytes`](Self::text_bytes),
    /// because they are per page rather than per event.
    pub corpus_bytes: usize,
    /// Stored body bytes the index carried, summed over the pages that had one.
    pub text_bytes: usize,
    /// The tantivy rebuild alone.
    pub lexical: Duration,
    /// The frecency fold alone. Always measured: one pass over the corpus, and
    /// the lane's weight is a fusion-time setting, so it needs no re-mint.
    pub frecency: Duration,
    /// The page-table projection alone — fingerprinting every address and
    /// folding the events onto the records the other lanes then index.
    pub page_table: Duration,
    /// The behavioural lane's candidate index alone: one tokenized pass over
    /// each record's addresses and title, so a keystroke narrows the corpus by
    /// prefix lookup instead of scanning every page.
    pub candidates: Duration,
    /// What that index costs resident, its keys excluded. Divided by
    /// [`pages`](Self::pages) it is the lane's bytes per page.
    pub candidate_bytes: usize,
    /// The whole mint, corpus walk and document fold included.
    pub total: Duration,
}

/// One disposable projection over the authoritative trace corpus: Eidetic's
/// minted recall index plus the cost receipt of minting it.
struct RecallIndex {
    minted: MintedRecall,
    /// Emitted at mint; kept for the tests and harnesses that read it back.
    #[cfg(test)]
    receipt: MintReceipt,
}

/// Answer one recall. The corpus is the authority and both indexes are
/// derived, so stale projections are re-minted rather than repaired: flush
/// what is buffered (otherwise the pages visited this minute would be
/// unrecallable), rebuild from every stored trace, then search. A failure is
/// reported rather than silently answered as an empty trail.
struct RecallRequest<'a> {
    sources: &'a [RecallSource],
    query: &'a str,
    limit: usize,
    config: RecallConfig,
}

#[cfg(test)]
#[path = "trail_memory_evaluation.rs"]
mod evaluation;

#[cfg(test)]
#[path = "trail_memory/tests.rs"]
mod tests;

/// A read capability over the actor, not a second opener of its Fjall files.
pub(crate) struct TrailInferenceOwner(pub std::sync::Arc<ActorHandle<TrailCommand>>);

#[cfg(test)]
#[path = "trail_memory/inference_owner_tests.rs"]
mod inference_owner_tests;

#[path = "trail_memory/source_capture_receipts.rs"]
mod source_capture_receipts;

/// Exact capture/clip owner. It never delegates to the PageText compatibility resolver.
pub(crate) struct CaptureInferenceOwner {
    pub trail: std::sync::Arc<ActorHandle<TrailCommand>>,
    pub bridge: crate::research::Bridge,
}

#[cfg(test)]
#[cfg(feature = "authoring")]
#[path = "trail_memory/research_owner_tests.rs"]
mod research_owner_tests;

#[cfg(test)]
#[cfg(feature = "authoring")]
#[path = "trail_memory/m5_inspection.rs"]
mod m5_inspection;

#[path = "trail_memory/memory_dir.rs"]
mod memory_dir;
#[path = "trail_memory/source_custody_snapshot.rs"]
mod source_custody_snapshot;
pub use memory_dir::memory_dir;
#[path = "trail_memory/sealed_memory_dir.rs"]
mod sealed_memory_dir;
use sealed_memory_dir::sealed_memory_dir;
#[path = "trail_memory/navigation.rs"]
mod navigation;
pub use navigation::navigation;
#[path = "trail_memory/now_ms.rs"]
mod now_ms;
use now_ms::now_ms;
#[path = "trail_memory/open_memory.rs"]
mod open_memory;
use open_memory::open_memory;
#[path = "trail_memory/flush.rs"]
mod flush;
use flush::flush;
#[path = "trail_memory/keep_page_text.rs"]
mod keep_page_text;
use keep_page_text::keep_page_text;
#[path = "trail_memory/capture_source_document.rs"]
#[cfg(test)]
mod capture_source_document;
#[cfg(test)]
use capture_source_document::capture_source_document;
#[path = "trail_memory/capture_core.rs"]
mod capture_core;
#[cfg(test)]
#[path = "trail_memory/capture_golden_tests.rs"]
mod capture_golden_tests;
pub(crate) use capture_core::{capture_source_document_in, verify_source_document_typed};
#[cfg(test)]
pub(crate) use capture_core::verify_source_document;
#[path = "trail_memory/stored_page_texts.rs"]
mod stored_page_texts;
use stored_page_texts::stored_page_texts;
#[path = "trail_memory/mint_receipt_emit.rs"]
mod mint_receipt_emit;
#[path = "trail_memory/recall.rs"]
mod recall;
#[path = "trail_memory/recall_index_mint.rs"]
mod recall_index_mint;
use recall::recall;
#[path = "trail_memory/spawn_trail.rs"]
mod spawn_trail;
pub use spawn_trail::spawn_trail;
#[path = "trail_memory/spawn_trail_with_capture_library.rs"]
mod spawn_trail_with_capture_library;
pub(crate) use spawn_trail_with_capture_library::spawn_trail_with_capture_library;
#[path = "trail_memory/spawn_trail_for_session.rs"]
mod spawn_trail_for_session;
pub(crate) use spawn_trail_for_session::spawn_trail_for_session;
pub(crate) use spawn_trail_for_session::spawn_trail_for_session_with_limits;
#[path = "trail_memory/impl_crate_inference_service_retained_source_owner.rs"]
mod impl_crate_inference_service_retained_source_owner;
#[path = "trail_memory/inference_owner.rs"]
mod inference_owner;
use inference_owner::inference_owner;
#[path = "trail_memory/inference_snapshot.rs"]
mod inference_snapshot;
use inference_snapshot::inference_snapshot;
#[path = "trail_memory/inference_source.rs"]
mod inference_source;
use inference_source::inference_source;
#[path = "trail_memory/emit_capture_recovery.rs"]
mod emit_capture_recovery;
use emit_capture_recovery::emit_capture_recovery;
#[path = "trail_memory/emit_sealed_capture_recovery.rs"]
mod emit_sealed_capture_recovery;
use emit_sealed_capture_recovery::emit_sealed_capture_recovery;
#[path = "trail_memory/impl_crate_inference_service_retained_source_owner_2.rs"]
mod impl_crate_inference_service_retained_source_owner_2;

#[path = "trail_memory/worker.rs"]
mod worker;

#[cfg(test)]
#[path = "trail_memory/storage_owner_review_tests.rs"]
mod storage_owner_review_tests;
