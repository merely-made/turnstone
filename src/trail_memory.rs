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
//! session-scoped `eidetic_fjall::FjallStore` at `sessions/<id>/memory`,
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
//! Page bodies ride the same channel ([`TrailCommand::RecordText`], W6c): the
//! fetch path extracts them through fleece and the actor writes them into the
//! session store through eidetic's [`PageTextStore`] — blob for the bytes, one
//! slot per address for the index. What may be stored is [`consented_to_keep`]'s
//! question (capture plan C4), named here and answered by a later slice.
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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use armillary::{ActorHandle, Emitter, Wake, spawn_named};
use eidetic::{
    BrowsingMemory, BrowsingTrace, FrecencyConfig, PageFingerprint, PageRef, PageTable,
    PageTextStore, PageTexts, TraceEvent, TraceTransition, bootstrap_browsing_schema,
    frecency_by_page, page_table,
};
use eidetic_fjall::FjallStore;
use eidetic_search::{CandidateIndex, FusedHit, IndexConfig, Ranking, TrailIndex, fuse_many};

use crate::action::{RecallHit, StoredSourceDocument, Update};

/// Traversals per stored trace segment. Segments are the flush granularity:
/// small enough that a crash loses minutes, large enough that a stored trace
/// is a meaningful corridor slice.
const SEGMENT_SIZE: usize = 32;

/// Standard reciprocal-rank damping. The ranking-relevant ratio is exposed
/// instead: frecency, relative to BM25.
const RRF_K: f64 = 60.0;

/// Pull a wider head from each input before reducing to the omnibar row limit.
const FUSION_CANDIDATE_MULTIPLIER: usize = 4;

/// Frecency's fusion weight relative to BM25, on by default. Above 1.0 so a
/// typed prefix the user has been to before outranks a page BM25 merely likes
/// the title of — the behavioural lane's whole point (wiring plan W6b).
const DEFAULT_FRECENCY_WEIGHT: f32 = 2.0;

/// Live application settings consumed by the derived recall index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecallConfig {
    frecency_weight: f32,
}

impl RecallConfig {
    /// Weight of the behavioural ranking relative to BM25; zero is the lane's
    /// off switch. The fold's own policy (per-transition weights, half-life,
    /// dwell bonus) is eidetic's, not the app's. No live setting moves it now
    /// that the vector knobs are gone, so only the harnesses call it.
    #[allow(dead_code)]
    pub fn with_frecency_weight(mut self, weight: f32) -> Self {
        self.frecency_weight = Self::clamp_weight(weight);
        self
    }

    fn clamp_weight(weight: f32) -> f32 {
        if weight.is_finite() {
            weight.clamp(0.0, 4.0)
        } else {
            0.0
        }
    }

    fn frecency_enabled(self) -> bool {
        self.frecency_weight > 0.0
    }
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            frecency_weight: DEFAULT_FRECENCY_WEIGHT,
        }
    }
}

/// One application-owned title available to the derived recall index.
///
/// Graph and recycle-bin state remain authoritative for these labels. The
/// trail actor uses them to fill titleless cloned [`PageRef`]s while minting an
/// index; the stored [`BrowsingTrace`] is never mutated.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecallSource {
    url: String,
    title: String,
}

impl RecallSource {
    /// Ignore fallback labels that add nothing beyond the canonical URL.
    pub fn new(url: impl Into<String>, title: impl Into<String>) -> Option<Self> {
        let url = url.into();
        let title = title.into().trim().to_string();
        (!title.is_empty() && title != url).then_some(Self { url, title })
    }
}

/// Commands the trail-memory actor takes (the shell sends these; ordering on
/// the one channel is the consistency story, the bin actor's exact shape).
pub enum TrailCommand {
    /// Record one traversal for `owner` (an opaque persona tag — turnstone
    /// passes the root identity's public key hex).
    Record {
        owner: String,
        url: String,
        transition: TraceTransition,
        at_ms: u64,
    },
    /// Keep one fetched page's extracted main text, keyed by the address it
    /// was fetched from (wiring plan W6c). Best-effort: a failure warns and
    /// the page stays recallable by title and URL alone.
    RecordText { url: String, text: String },
    /// An explicit request to preserve one fetched HTML response. This remains
    /// separate from `RecordText`: recall text is never source evidence.
    CaptureSourceDocument {
        node: uuid::Uuid,
        url: String,
        document: crate::content::FetchedDocument,
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
    Reopen(PathBuf),
    /// Flush, drop the open store, and ack — the close path's handshake:
    /// Windows cannot rename a directory whose files are open, so the shell
    /// releases the memory store BEFORE moving the session dir to the trash.
    Release(std::sync::mpsc::SyncSender<()>),
}

/// One session's trail-memory directory (under its `sessions/<id>/` dir).
pub fn memory_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("memory")
}

/// The lexical index beside a memory store, on eidetic-recall's `<db>.index`
/// convention. Derived state: it is re-minted from the corpus, never repaired.
fn index_dir(memory: &Path) -> PathBuf {
    let mut dir = memory.as_os_str().to_os_string();
    dir.push(".index");
    PathBuf::from(dir)
}

/// Map a drained [`AppEvent`](crate::observe::AppEvent) onto a traversal,
/// when it is one at all. `AddressOpened` reads as a typed address: the
/// omnibar, a trail row, and a grid click all lower through
/// `Action::OpenAddress`, and the event does not carry which.
pub fn navigation(event: &crate::observe::AppEvent) -> Option<(String, TraceTransition)> {
    use crate::observe::AppEvent;
    match event {
        AppEvent::AddressOpened(url) => Some((url.clone(), TraceTransition::UrlTyped)),
        // The engine callback does not distinguish a link activation from a
        // redirect. Preserve the traversal without manufacturing a cause.
        AppEvent::ContentNavigated { url, .. } => Some((url.clone(), TraceTransition::Unknown)),
        AppEvent::NavigatedBack(url) => Some((url.clone(), TraceTransition::Back)),
        AppEvent::NavigatedForward(url) => Some((url.clone(), TraceTransition::Forward)),
        AppEvent::Reloaded(url) => Some((url.clone(), TraceTransition::Reload)),
        _ => None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Open the store, seed the schema engram, and load the stored corpus. A
/// `None` disables capture until the next `Reopen` — a browser that cannot
/// remember must still browse.
fn open_memory(dir: &Path) -> Option<(FjallStore, BrowsingMemory)> {
    if let Err(err) = std::fs::create_dir_all(dir) {
        tracing::warn!(%err, dir = %dir.display(), "trail memory: create dir failed; capture disabled until reopen");
        return None;
    }
    let mut store = match FjallStore::open(dir) {
        Ok(store) => store,
        Err(err) => {
            tracing::warn!(%err, dir = %dir.display(), "trail memory: open failed; capture disabled until reopen");
            return None;
        },
    };
    if let Err(err) = pollster::block_on(bootstrap_browsing_schema(&mut store)) {
        tracing::warn!(%err, "trail memory: schema bootstrap failed; capture disabled until reopen");
        return None;
    }
    match pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)) {
        Ok(memory) => Some((store, memory)),
        Err(err) => {
            tracing::warn!(%err, "trail memory: corpus load failed; capture disabled until reopen");
            None
        },
    }
}

/// Flush open segments; a failure warns and keeps them buffered (they retry
/// on the next lifecycle edge).
fn flush(store: &mut FjallStore, memory: &mut BrowsingMemory) {
    if let Err(err) = pollster::block_on(memory.flush(store, now_ms())) {
        tracing::warn!(%err, "trail memory: flush failed; open segments retained");
    }
}

/// Canonicalize the current title projection. A URL may appear in more than
/// one graph member; sorting makes the selected title deterministic rather
/// than dependent on graph iteration order.
fn canonical_sources(mut sources: Vec<RecallSource>) -> Vec<RecallSource> {
    sources.sort_unstable();
    sources.dedup_by(|left, right| left.url == right.url);
    sources
}

fn fill_title(page: &mut PageRef, titles: &HashMap<&str, &str>) {
    if page.title.is_none()
        && let Some(title) = titles.get(page.url.as_str())
    {
        page.title = Some((*title).to_string());
    }
}

/// Clone the authoritative trace corpus and fill missing page titles from the
/// application projection. Only the clone reaches the derived index.
fn traces_with_titles(memory: &BrowsingMemory, sources: &[RecallSource]) -> Vec<BrowsingTrace> {
    let titles: HashMap<&str, &str> = sources
        .iter()
        .map(|source| (source.url.as_str(), source.title.as_str()))
        .collect();
    memory
        .traces()
        .cloned()
        .map(|mut trace| {
            for event in &mut trace.events {
                fill_title(&mut event.to, &titles);
                if let Some(from) = &mut event.from {
                    fill_title(from, &titles);
                }
                for candidate in &mut event.candidates {
                    fill_title(candidate, &titles);
                }
            }
            trace
        })
        .collect()
}

/// The consent gate (capture plan C4). The single place a policy will refuse
/// a page: refusing here keeps the body out of the store, and so out of every
/// index projected from it. A no-op today — C4 replaces this body, not its
/// call site.
fn consented_to_keep(_url: &str) -> bool {
    true
}

/// Write one page's extracted text into the session store. Warns and returns
/// on failure: a browser that cannot remember a body must still browse.
fn keep_page_text(store: &FjallStore, url: &str, text: &str) {
    if !consented_to_keep(url) {
        tracing::info!(%url, "trail memory: page text withheld by consent");
        return;
    }
    let texts = PageTextStore::new(store);
    if let Err(err) = pollster::block_on(texts.put(url, text, now_ms())) {
        tracing::warn!(%err, %url, "trail memory: page text not stored");
    }
}

/// Store the exact acquired bytes and the Fleece annotation that describes a
/// canonical-text projection of those bytes. This is invoked only by the
/// explicit action path, never by fetch enrichment or trail recall.
fn capture_source_document(
    store: &mut FjallStore,
    document: &crate::content::FetchedDocument,
) -> Result<StoredSourceDocument, String> {
    use eidetic::models::OpaqueBlob;
    use eidetic::{
        ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp,
        TrustEnvelope, TrustLevel,
    };
    use mere_document_lanes::eidetic_bridge::{CaptureDomMode, CaptureEvidenceV1};

    if document.acquired_at_ms == 0 {
        return Err("source capture requires an observed acquisition time".to_string());
    }
    if !document
        .content_type
        .as_deref()
        .is_some_and(|content_type| {
            content_type
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/html"))
        })
    {
        return Err(
            "source capture requires an observed text/html response content type".to_string(),
        );
    }
    let captured_at = Timestamp(document.acquired_at_ms);
    let effective_url = document
        .effective_url
        .clone()
        .ok_or_else(|| "source capture requires an observed final response URL".to_string())?;
    let parsed = genet_static_dom::StaticDocument::parse(&document.body);
    let extracted = fleece::extract_document(&parsed);
    let character_count = extracted.page.text.chars().count() as u64;
    let anchor = fleece::anchor_for_range(
        &extracted.page.text,
        fleece::TextPositionSelector {
            start: 0,
            end: character_count,
        },
        extracted.contract.quote_context,
    )
    .ok_or_else(|| "Fleece produced no canonical text for a whole-document anchor".to_string())?;
    let raw_manifest = pollster::block_on(eidetic::save_typed(
        store,
        &OpaqueBlob(document.bytes.clone()),
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Imported {
                source: effective_url.clone(),
            },
            upstream: Vec::new(),
            tooling: Some("turnstone-source-document-capture/v1".into()),
            generated_at: captured_at,
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        captured_at,
    ))
    .map_err(|error| format!("raw source deposit: {error}"))?;
    let evidence = CaptureEvidenceV1::new(
        effective_url,
        eidetic::Hash::of(&document.bytes),
        captured_at,
        document.content_type.clone(),
        CaptureDomMode::Source,
        Some(raw_manifest),
        None,
    )
    .map_err(|error| format!("source capture evidence: {error}"))?;
    let record = mere_document_lanes::FleeceAnnotationRecord::from_fleece(
        mere_document_lanes::CaptureIdentity::from_evidence(evidence)
            .map_err(|error| format!("source capture identity: {error}"))?,
        &extracted,
        &anchor,
    )
    .map_err(|error| format!("Fleece annotation: {error}"))?;
    pollster::block_on(mere_document_lanes::bootstrap_fleece_annotation_schema(
        store,
    ))
    .map_err(|error| format!("Fleece schema bootstrap: {error}"))?;
    let annotation_manifest = pollster::block_on(mere_document_lanes::save_fleece_annotation(
        store,
        &record,
        document.acquired_at_ms,
    ))
    .map_err(|error| format!("Fleece annotation deposit: {error}"))?;
    Ok(StoredSourceDocument {
        raw_manifest: raw_manifest.to_string(),
        annotation_manifest: annotation_manifest.to_string(),
    })
}

/// Every stored page body for the open session. An unreadable store yields an
/// empty snapshot rather than failing the recall: pages then recall by title
/// and URL, which is what they did before W6c.
fn stored_page_texts(store: &FjallStore) -> PageTexts {
    match pollster::block_on(PageTextStore::new(store).load_all()) {
        Ok(texts) => texts,
        Err(err) => {
            tracing::warn!(%err, "trail memory: page texts unreadable; recall falls back to titles");
            PageTexts::default()
        },
    }
}

/// The page table over a corpus, taking each page's body from the store's
/// snapshot (W6c). A page with no stored text still gets a record; its
/// fingerprint falls back to the canonical URL, which the record names.
fn page_table_of(traces: &[BrowsingTrace], texts: &PageTexts) -> PageTable {
    page_table(traces, texts.lookup())
}

/// One document per page record, keyed by the address a hit opens (the page's
/// most recent URL). Replaces the URL-string dedup this fold used to do: two
/// addresses for one page are now one document before the index sees them.
fn recall_documents(table: &PageTable) -> BTreeMap<String, RecallHit> {
    table
        .records
        .values()
        .map(|record| {
            (
                record.last_url.clone(),
                RecallHit {
                    url: record.last_url.clone(),
                    title: record.title.clone(),
                    at_ms: record.last_seen_ms,
                },
            )
        })
        .collect()
}

/// The page table as a corpus the lexical index can mint from: the most recent
/// event that reached each page, with its destination rewritten to the
/// record's address and best title. One tantivy document per page rather than
/// one per visit (the 3x duplication W6a measured), while owner, transition
/// and timestamp stay the real ones rather than invented.
fn page_corpus(traces: &[BrowsingTrace], table: &PageTable) -> Vec<BrowsingTrace> {
    let mut latest: BTreeMap<PageFingerprint, (String, TraceEvent)> = BTreeMap::new();
    for trace in traces {
        for event in &trace.events {
            // The table's own memo, so the address is canonicalized once for
            // the whole mint rather than once per fold.
            let Some(fingerprint) = table.by_address.get(&event.to.url).copied() else {
                continue;
            };
            match latest.get_mut(&fingerprint) {
                Some(entry) if event.at_ms >= entry.1.at_ms => {
                    *entry = (trace.owner.clone(), event.clone());
                },
                Some(_) => {},
                None => {
                    latest.insert(fingerprint, (trace.owner.clone(), event.clone()));
                },
            }
        }
    }

    let mut by_owner: BTreeMap<String, Vec<TraceEvent>> = BTreeMap::new();
    for (fingerprint, (owner, mut event)) in latest {
        let Some(record) = table.records.get(&fingerprint) else {
            continue;
        };
        event.to = PageRef {
            url: record.last_url.clone(),
            title: record.title.clone(),
        };
        by_owner.entry(owner).or_default().push(event);
    }
    by_owner
        .into_iter()
        .map(|(owner, mut events)| {
            events.sort_by_key(|event| event.at_ms);
            BrowsingTrace::from_events(owner, events)
        })
        .collect()
}

/// Events, indexable bytes and distinct destination addresses across a
/// corpus. Addresses are what the index used to hold one document each of;
/// the page table's record count subtracted from them is what collapsed.
fn corpus_size(traces: &[BrowsingTrace]) -> (usize, usize, usize) {
    let mut events = 0;
    let mut bytes = 0;
    let mut urls = HashSet::new();
    for trace in traces {
        events += trace.events.len();
        for event in &trace.events {
            bytes += event.to.url.len() + event.to.title.as_deref().map_or(0, str::len);
            urls.insert(event.to.url.as_str());
        }
    }
    (events, bytes, urls.len())
}

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

impl MintReceipt {
    /// One line per mint. Tracing is the receipt's only surface: threading it
    /// through `Update::RecallHits` would widen the omnibar API for a number
    /// the omnibar does not render.
    fn emit(&self) {
        tracing::info!(
            traces = self.traces,
            events = self.events,
            pages = self.pages,
            pages_with_text = self.pages_with_text,
            collapsed_urls = self.collapsed_urls,
            corpus_bytes = self.corpus_bytes,
            text_bytes = self.text_bytes,
            page_table_us = self.page_table.as_micros() as u64,
            candidates_us = self.candidates.as_micros() as u64,
            candidate_bytes = self.candidate_bytes,
            lexical_us = self.lexical.as_micros() as u64,
            frecency_us = self.frecency.as_micros() as u64,
            total_us = self.total.as_micros() as u64,
            "trail memory: recall index minted"
        );
    }
}

/// One disposable projection over the authoritative trace corpus.
struct RecallIndex {
    lexical: TrailIndex,
    /// The behavioural lane. Folded keyed by page fingerprint, so visits split
    /// across a page's addresses sum into one score, then re-keyed to each
    /// record's address — the string the other two lanes rank by. Folded at
    /// mint against one reference `now`, so every query this index answers
    /// decays from the same instant.
    frecency: BTreeMap<String, f64>,
    /// Which records a typed query could mean, by token prefix over each
    /// record's addresses and title. The behavioural lane's narrowing step.
    candidates: CandidateIndex<String>,
    /// The same scores as `frecency`, by the candidate index's record id, so a
    /// wide candidate set is scored by index rather than by key lookup.
    frecency_by_id: Vec<f64>,
    documents: BTreeMap<String, RecallHit>,
    receipt: MintReceipt,
}

impl RecallIndex {
    /// The mint takes no [`RecallConfig`]: both lanes are minted whole and the
    /// only live setting left, the frecency weight, applies at fusion time.
    fn mint(dir: &Path, traces: &[BrowsingTrace], texts: &PageTexts) -> Result<Self, String> {
        let started = Instant::now();
        let (events, corpus_bytes, urls) = corpus_size(traces);
        let table_started = Instant::now();
        let table = page_table_of(traces, texts);
        let documents = recall_documents(&table);
        let page_traces = page_corpus(traces, &table);
        let table_elapsed = table_started.elapsed();
        let lexical_started = Instant::now();
        // The record's own text, not the snapshot's: the page table already
        // resolved which body belongs to the address a document is keyed by.
        let bodies: BTreeMap<&str, &str> = table
            .records
            .values()
            .filter_map(|record| Some((record.last_url.as_str(), record.text.as_deref()?)))
            .collect();
        // Re-minted on every recall and never opened from disk, so the
        // projection stays in memory: the persisted write is pure cost here.
        let lexical = TrailIndex::rebuild_with_config(
            index_dir(dir),
            &page_traces,
            |url| bodies.get(url).map(|text| (*text).to_string()),
            IndexConfig { persist: false, ..IndexConfig::default() },
        )
        .map_err(|err| format!("re-mint: {err}"))?;
        let lexical_elapsed = lexical_started.elapsed();
        let frecency_started = Instant::now();
        let frecency: BTreeMap<String, f64> =
            frecency_by_page(traces, now_ms(), &FrecencyConfig::default(), &table)
                .into_iter()
                .filter_map(|(fingerprint, score)| {
                    table
                        .records
                        .get(&fingerprint)
                        .map(|record| (record.last_url.clone(), score))
                })
                .collect();
        let frecency_elapsed = frecency_started.elapsed();
        // The behavioural lane's narrowing step, over the same text the lane
        // used to substring-scan: every address that resolved to the record,
        // the address a hit opens, and the title.
        let candidates_started = Instant::now();
        let mut frecency_by_id = Vec::with_capacity(table.records.len());
        let candidates = CandidateIndex::build(table.records.values().map(|record| {
            let mut texts: Vec<&str> = record.urls.iter().map(String::as_str).collect();
            texts.push(record.last_url.as_str());
            if let Some(title) = record.title.as_deref() {
                texts.push(title);
            }
            frecency_by_id.push(frecency.get(&record.last_url).copied().unwrap_or(0.0));
            (record.last_url.clone(), texts)
        }));
        let candidates_elapsed = candidates_started.elapsed();
        let index = Self {
            lexical,
            frecency,
            receipt: MintReceipt {
                traces: traces.len(),
                events,
                pages: documents.len(),
                pages_with_text: bodies.len(),
                text_bytes: bodies.values().map(|text| text.len()).sum(),
                collapsed_urls: urls.saturating_sub(documents.len()),
                corpus_bytes,
                lexical: lexical_elapsed,
                frecency: frecency_elapsed,
                page_table: table_elapsed,
                candidates: candidates_elapsed,
                candidate_bytes: candidates.memory_bytes(),
                total: started.elapsed(),
            },
            candidates,
            frecency_by_id,
            documents,
        };
        index.receipt.emit();
        Ok(index)
    }

    /// Pages the typed text could mean, best-frecency first. The case this
    /// lane answers is a typed URL prefix, and the page a prefix should recall
    /// is the one visited most and most recently. Pages with no behavioural
    /// evidence (only redirects and reloads) score zero and stay out — an empty
    /// lane, not a zero-rank one.
    ///
    /// Narrowing is [`CandidateIndex`]'s token prefix, not the whole-query
    /// substring this used to scan every page for: `zette` no longer reaches
    /// `gazette`, and the lane costs a lookup rather than 42k lowercasings.
    fn frecency_urls(&self, query: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(u32, f64)> = self
            .candidates
            .candidate_ids(query)
            .into_iter()
            .map(|id| (id, self.frecency_by_id[id as usize]))
            .filter(|(_, score)| score.is_finite() && *score > 0.0)
            .collect();
        // Best score first, id as the tiebreak; only the head is ordered,
        // since a two-character prefix can name thousands of pages.
        let order = |left: &(u32, f64), right: &(u32, f64)| {
            right.1.total_cmp(&left.1).then_with(|| left.0.cmp(&right.0))
        };
        if scored.len() > limit {
            scored.select_nth_unstable_by(limit - 1, order);
            scored.truncate(limit);
        }
        scored.sort_unstable_by(order);
        scored
            .into_iter()
            .map(|(id, _)| self.candidates.key(id).clone())
            .collect()
    }

    fn fused_hits(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<FusedHit>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let candidate_limit = limit.saturating_mul(FUSION_CANDIDATE_MULTIPLIER).max(limit);
        let lexical_hits = self
            .lexical
            .search(query, candidate_limit)
            .map_err(|err| format!("search: {err}"))?;
        let mut seen = HashSet::new();
        let mut lexical_urls = Vec::new();
        for hit in &lexical_hits {
            if seen.insert(hit.url.clone()) {
                lexical_urls.push(hit.url.clone());
            }
        }

        let frecency_urls = if config.frecency_enabled() {
            self.frecency_urls(query, candidate_limit)
        } else {
            Vec::new()
        };

        // Lane order is the contract fusion's named ranks read: lexical, then
        // behavioural.
        Ok(fuse_many(
            &[
                Ranking::new(&lexical_urls, 1.0),
                Ranking::new(&frecency_urls, f64::from(config.frecency_weight)),
            ],
            RRF_K,
        )
        .into_iter()
        .take(limit)
        .collect())
    }

    fn search(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<RecallHit>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        // The behavioural lane off is the compatibility path: preserve
        // TrailIndex's ranking and metadata exactly.
        if !config.frecency_enabled() {
            return Ok(self
                .lexical
                .search(query, limit)
                .map_err(|err| format!("search: {err}"))?
                .into_iter()
                .map(|hit| RecallHit {
                    url: hit.url,
                    title: hit.title,
                    at_ms: hit.at_ms,
                })
                .collect());
        }

        Ok(self
            .fused_hits(query, limit, config)?
            .into_iter()
            .filter_map(|fused| {
                self.documents.get(&fused.url).cloned()
            })
            .collect())
    }
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

fn recall(
    store: &mut FjallStore,
    memory: &mut BrowsingMemory,
    index: &mut Option<RecallIndex>,
    stale: &mut bool,
    dir: &Path,
    request: RecallRequest<'_>,
) -> Result<Vec<RecallHit>, String> {
    if *stale || index.is_none() {
        flush(store, memory);
        let traces = traces_with_titles(memory, request.sources);
        let texts = stored_page_texts(store);
        *index = Some(RecallIndex::mint(dir, &traces, &texts)?);
        *stale = false;
    }
    let Some(index) = index.as_ref() else {
        return Err("no index".to_string());
    };
    index.search(request.query, request.limit, request.config)
}

/// Spawn the trail-memory actor over the session memory at `dir`, waking the
/// event loop on store activity like the bin does. W1 emits no updates:
/// failures warn and capture continues; the recall pane (W2) is the first
/// reader of what lands here.
pub fn spawn_trail(wake: Wake, dir: PathBuf) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_named(
        "trail-memory",
        wake,
        move |commands, out: Emitter<Update>| {
            let mut state = open_memory(&dir);
            let mut current_dir = dir.clone();
            // The derived lexical/behavioural projection and whether the corpus has
            // moved since it was minted. Built on the first recall, not at
            // spawn: a session that never searches never pays for one.
            let mut index: Option<RecallIndex> = None;
            let mut index_stale = true;
            let mut indexed_sources = Vec::new();
            // Per-owner origin chain: the last destination becomes the next
            // event's `from`.
            let mut last_to: HashMap<String, PageRef> = HashMap::new();
            while let Ok(command) = commands.recv() {
                match command {
                    TrailCommand::Record {
                        owner,
                        url,
                        transition,
                        at_ms,
                    } => {
                        let Some((store, memory)) = state.as_mut() else {
                            continue;
                        };
                        let to = PageRef { url, title: None };
                        let event = TraceEvent {
                            from: last_to.insert(owner.clone(), to.clone()),
                            to,
                            transition,
                            at_ms,
                            dwell_ms: None,
                            candidates: Vec::new(),
                        };
                        index_stale = true;
                        if memory.record_traversal(&owner, event) {
                            flush(store, memory);
                        }
                    },
                    TrailCommand::Recall {
                        query,
                        limit,
                        sources,
                        config,
                    } => {
                        let Some((store, memory)) = state.as_mut() else {
                            out.emit(Update::RecallFailed {
                                error: "the trail store is not open".to_string(),
                            });
                            continue;
                        };
                        let sources = canonical_sources(sources);
                        if sources != indexed_sources {
                            index_stale = true;
                            indexed_sources = sources;
                        }
                        match recall(
                            store,
                            memory,
                            &mut index,
                            &mut index_stale,
                            &current_dir,
                            RecallRequest {
                                sources: &indexed_sources,
                                query: &query,
                                limit,
                                config,
                            },
                        ) {
                            Ok(hits) => out.emit(Update::RecallHits { query, hits }),
                            Err(error) => out.emit(Update::RecallFailed { error }),
                        }
                    },
                    TrailCommand::RecordText { url, text } => {
                        let Some((store, _)) = state.as_ref() else {
                            continue;
                        };
                        keep_page_text(store, &url, &text);
                        // The body is part of the corpus the index is minted
                        // from, so a new one makes the projection stale.
                        index_stale = true;
                    },
                    TrailCommand::CaptureSourceDocument {
                        node,
                        url,
                        document,
                    } => {
                        let result = match state.as_mut() {
                            Some((store, _)) => capture_source_document(store, &document),
                            None => Err("the session Eidetic store is not open".to_string()),
                        };
                        out.emit(Update::SourceDocumentCaptured { node, url, result });
                    },
                    TrailCommand::Flush => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                    },
                    TrailCommand::Reopen(dir) => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                        last_to.clear();
                        // The index belongs to the departing session's corpus;
                        // the adopted one mints its own on first recall.
                        index = None;
                        index_stale = true;
                        indexed_sources.clear();
                        state = open_memory(&dir);
                        current_dir = dir;
                    },
                    TrailCommand::Release(ack) => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                        state = None;
                        index = None;
                        index_stale = true;
                        indexed_sources.clear();
                        last_to.clear();
                        let _ = ack.send(());
                    },
                }
            }
        },
    )
}

#[cfg(test)]
#[path = "trail_memory_evaluation.rs"]
mod evaluation;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir()
            .join("turnstone-trail-memory-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// One synthetic page's url and title. Deterministic, so a test can total
    /// the corpus bytes without asking the code under test.
    fn synthetic_page(index: usize) -> (String, String) {
        (
            format!("https://page{index}.example/trail/{index}"),
            format!("Synthetic Page {index}"),
        )
    }

    /// `events` traversals cycling over `pages` distinct URLs, chained and
    /// chunked into `SEGMENT_SIZE` segments exactly as the actor stores them.
    fn synthetic_traces(events: usize, pages: usize) -> Vec<BrowsingTrace> {
        let mut built = Vec::new();
        let mut segment = Vec::new();
        let mut previous: Option<PageRef> = None;
        for index in 0..events {
            let (url, title) = synthetic_page(index % pages);
            let to = PageRef {
                url,
                title: Some(title),
            };
            segment.push(TraceEvent {
                from: previous.replace(to.clone()),
                to,
                transition: TraceTransition::LinkClick,
                at_ms: index as u64 + 1,
                dwell_ms: None,
                candidates: Vec::new(),
            });
            if segment.len() == SEGMENT_SIZE {
                built.push(BrowsingTrace::from_events(
                    "p",
                    std::mem::take(&mut segment),
                ));
            }
        }
        if !segment.is_empty() {
            built.push(BrowsingTrace::from_events("p", segment));
        }
        built
    }

    fn expected_corpus_bytes(events: usize, pages: usize) -> usize {
        (0..events)
            .map(|index| {
                let (url, title) = synthetic_page(index % pages);
                url.len() + title.len()
            })
            .sum()
    }

    /// The W6a receipt: counts are exact against a corpus the test built, and
    /// both mint lanes are actually timed.
    #[test]
    fn mint_receipt_counts_the_corpus_exactly() {
        const EVENTS: usize = 1_000;
        const PAGES: usize = 300;
        let traces = synthetic_traces(EVENTS, PAGES);

        let index = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
        let receipt = index.receipt;
        assert_eq!(receipt.traces, EVENTS.div_ceil(SEGMENT_SIZE));
        assert_eq!(receipt.events, EVENTS);
        assert_eq!(receipt.pages, PAGES, "distinct destination URLs");
        assert_eq!(receipt.corpus_bytes, expected_corpus_bytes(EVENTS, PAGES));
        assert!(
            receipt.lexical > Duration::ZERO,
            "the lexical lane is timed"
        );
        assert!(
            receipt.frecency > Duration::ZERO,
            "the behavioural lane is timed"
        );
        assert!(receipt.total >= receipt.lexical, "total covers both lanes");
    }

    /// W6d's done-condition: two addresses for one page are one page record,
    /// one index document, one hit, and one summed frecency.
    #[test]
    fn two_urls_for_one_page_recall_once_with_one_frecency() {
        let now = now_ms();
        let addresses = [
            "https://gazette.test/morning?utm_source=newsletter&utm_medium=email",
            "https://gazette.test/morning#lede",
            "https://gazette.test/morning",
        ];
        let events: Vec<TraceEvent> = addresses
            .iter()
            .enumerate()
            .map(|(nth, url)| TraceEvent {
                from: None,
                to: PageRef {
                    url: (*url).to_string(),
                    title: Some("The Morning Paper".to_string()),
                },
                transition: TraceTransition::UrlTyped,
                at_ms: now - nth as u64 * 1_000,
                dwell_ms: None,
                candidates: Vec::new(),
            })
            .collect();
        let traces = vec![BrowsingTrace::from_events("p", events)];

        let index = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
        assert_eq!(index.receipt.events, 3);
        assert_eq!(index.receipt.pages, 1, "one page behind three addresses");
        assert_eq!(index.receipt.collapsed_urls, 2);
        assert!(
            index.receipt.page_table > Duration::ZERO,
            "the fold is timed"
        );
        assert_eq!(
            index.lexical.doc_count().unwrap(),
            1,
            "one index document per page, not per event"
        );

        let hits = index
            .search("gazette morning", 5, RecallConfig::default())
            .unwrap();
        assert_eq!(hits.len(), 1, "one page, one row");
        assert_eq!(
            hits[0].url, addresses[0],
            "the row opens the latest address"
        );

        // One combined score, not three split ones — and it is the sum.
        assert_eq!(index.frecency.len(), 1);
        let combined = index.frecency[addresses[0]];
        let single = RecallIndex::mint(
            &temp_dir(),
            &traces[..1]
                .iter()
                .map(|trace| BrowsingTrace::from_events("p", trace.events[..1].to_vec()))
                .collect::<Vec<_>>(),
            &PageTexts::default(),
        )
        .unwrap()
        .frecency[addresses[0]];
        assert!(
            (combined - 3.0 * single).abs() < single * 0.01,
            "three typed visits to one page sum: {combined} vs 3 x {single}"
        );
    }

    /// W6c's done-condition in the small: a term that appears only in a page
    /// BODY — not in its URL, not in its title — recalls the page, through the
    /// same store the actor writes to.
    #[test]
    fn a_body_only_term_recalls_the_page() {
        const BODY: &str = "A kestrel hovers on a fixed point of air, head still while \
                            the wings work, and stoops only when the vole below commits \
                            to a run across the open verge.";
        let url = "https://field.test/notes/7";
        let dir = temp_dir();
        let store = FjallStore::open(&dir).unwrap();
        // The write path the actor takes, consent hook included.
        keep_page_text(&store, url, BODY);
        let texts = stored_page_texts(&store);
        assert_eq!(texts.get(url), Some(BODY), "the store round-trips the body");

        let traces = vec![BrowsingTrace::from_events(
            "p",
            vec![TraceEvent {
                from: None,
                to: PageRef {
                    url: url.to_string(),
                    title: Some("Notes".to_string()),
                },
                transition: TraceTransition::UrlTyped,
                at_ms: now_ms(),
                dwell_ms: None,
                candidates: Vec::new(),
            }],
        )];

        let index = RecallIndex::mint(&temp_dir(), &traces, &texts).unwrap();
        assert_eq!(index.receipt.pages_with_text, 1, "the body reached the index");

        // "kestrel" is in neither the URL nor the title.
        assert!(!url.contains("kestrel"));
        let hits = index
            .search("kestrel", 5, RecallConfig::default().with_frecency_weight(0.0))
            .unwrap();
        assert_eq!(hits.len(), 1, "a body term recalls the page");
        assert_eq!(hits[0].url, url);

        // The negative control: the same corpus with no stored text cannot.
        let bare = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
        assert_eq!(bare.receipt.pages_with_text, 0);
        assert!(
            bare.search("kestrel", 5, RecallConfig::default().with_frecency_weight(0.0))
                .unwrap()
                .is_empty(),
            "without the body the term is unrecallable — the instrument works"
        );
    }

    /// The W6a cost curve. A measurement run, not a correctness gate: build in
    /// release and pass `--ignored --nocapture`.
    #[test]
    #[ignore = "measurement run: cargo test --release ... -- --ignored --nocapture"]
    fn mint_receipt_scale_ladder() {
        for events in [1_000usize, 10_000, 100_000] {
            // Hold the revisit ratio constant so the curve compares like sizes.
            let pages = events / 3;
            let traces = synthetic_traces(events, pages);
            let index = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
            println!("synthetic pages={pages} {:?}", index.receipt);
        }
    }

    /// Two records chain `from`, Flush persists, Release closes the store,
    /// and a fresh load reads one trace whose events carry the chain — the
    /// round trip W2's re-mint depends on.
    #[test]
    fn records_chain_and_round_trip() {
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, _rx) = spawn_trail(wake, dir.clone());
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: "https://a.example/".into(),
            transition: TraceTransition::UrlTyped,
            at_ms: 1,
        });
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: "https://b.example/".into(),
            transition: TraceTransition::Back,
            at_ms: 2,
        });
        handle.command(TrailCommand::Flush);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();

        let mut store = FjallStore::open(&dir).unwrap();
        let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)).unwrap();
        let traces: Vec<_> = memory.traces().collect();
        assert_eq!(traces.len(), 1, "one flushed segment");
        let events = &traces[0].events;
        assert_eq!(events.len(), 2);
        assert!(events[0].from.is_none(), "first event is an origin");
        assert_eq!(events[0].to.url, "https://a.example/");
        assert_eq!(
            events[1].from.as_ref().map(|p| p.url.as_str()),
            Some("https://a.example/"),
            "the chain: last destination becomes the next origin"
        );
        assert_eq!(events[1].transition, TraceTransition::Back);
    }

    #[test]
    fn source_capture_refuses_incomplete_acquisition_context() {
        let dir = temp_dir();
        let mut store = FjallStore::open(&dir).unwrap();
        let complete = crate::content::FetchedDocument {
            bytes: b"<p>kestrel</p>".to_vec(),
            content_type: Some("text/html".into()),
            body: "<p>kestrel</p>".into(),
            effective_url: Some("https://example.test/kestrel".into()),
            acquired_at_ms: 9,
        };
        let missing_url = crate::content::FetchedDocument {
            effective_url: None,
            ..complete.clone()
        };
        assert!(
            capture_source_document(&mut store, &missing_url)
                .unwrap_err()
                .contains("final response URL")
        );
        let missing_time = crate::content::FetchedDocument {
            acquired_at_ms: 0,
            ..complete
        };
        assert!(
            capture_source_document(&mut store, &missing_time)
                .unwrap_err()
                .contains("acquisition time")
        );
    }

    #[test]
    fn explicit_source_capture_keeps_exact_bytes_and_reopens_annotation() {
        let dir = temp_dir();
        let exact = b"<html><body><main><p>source-only kestrel</p></main></body></html>";
        let document = crate::content::FetchedDocument {
            bytes: exact.to_vec(),
            content_type: Some("text/html; charset=utf-8".into()),
            body: String::from_utf8(exact.to_vec()).unwrap(),
            effective_url: Some("https://final.example/kestrel".into()),
            acquired_at_ms: 1_700_000_000_000,
        };
        let mut store = FjallStore::open(&dir).unwrap();
        let stored = capture_source_document(&mut store, &document).unwrap();
        assert_ne!(stored.raw_manifest, stored.annotation_manifest);
        drop(store);

        let mut reopened = FjallStore::open(&dir).unwrap();
        let manifests = pollster::block_on(eidetic::typed::list_typed::<
            mere_document_lanes::FleeceAnnotationRecord,
        >(&mut reopened))
        .unwrap();
        assert_eq!(manifests.len(), 1);
        let record = pollster::block_on(mere_document_lanes::load_fleece_annotation(
            &mut reopened,
            manifests[0].id,
        ))
        .unwrap()
        .unwrap();
        let evidence = record.extraction.capture.evidence.as_ref().unwrap();
        let raw_manifest_id = evidence.raw_manifest.expect("raw manifest");
        assert_eq!(evidence.final_source, "https://final.example/kestrel");
        assert_eq!(raw_manifest_id.to_string(), stored.raw_manifest);
        assert_eq!(evidence.capture_hash, eidetic::Hash::of(exact));
        assert!(evidence.has_authoritative_acquisition_context());
        let raw_manifest = pollster::block_on(eidetic::manifest::load_manifest(
            &mut reopened,
            raw_manifest_id,
        ))
        .unwrap()
        .unwrap();
        let raw = pollster::block_on(eidetic::manifest::resolve_blob(
            &mut reopened,
            &mut eidetic::NoFetcher,
            &raw_manifest,
        ))
        .unwrap();
        assert_eq!(raw, exact, "the raw response bytes are not Fleece text");
        assert!(
            record
                .extraction
                .canonical_text_record
                .canonical_text
                .contains("kestrel")
        );
    }

    /// A segment that fills flushes without an explicit Flush command.
    #[test]
    fn full_segment_flushes_itself() {
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, _rx) = spawn_trail(wake, dir.clone());
        for i in 0..SEGMENT_SIZE {
            handle.command(TrailCommand::Record {
                owner: "p".into(),
                url: format!("https://page{i}.example/"),
                transition: TraceTransition::LinkClick,
                at_ms: i as u64,
            });
        }
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();

        let mut store = FjallStore::open(&dir).unwrap();
        let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)).unwrap();
        let total: usize = memory.traces().map(|t| t.events.len()).sum();
        assert_eq!(total, SEGMENT_SIZE);
    }

    /// The event mapping: the four navigation events map, the rest do not.
    #[test]
    fn navigation_mapping() {
        use crate::observe::AppEvent;
        assert_eq!(
            navigation(&AppEvent::AddressOpened("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::UrlTyped))
        );
        assert_eq!(
            navigation(&AppEvent::NavigatedBack("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::Back))
        );
        assert_eq!(
            navigation(&AppEvent::NavigatedForward("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::Forward))
        );
        assert_eq!(
            navigation(&AppEvent::Reloaded("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::Reload))
        );
        assert_eq!(navigation(&AppEvent::WindowOpened), None);
    }

    /// Titles remain graph/bin authority: the actor overlays them into its
    /// derived index and re-mints when that projection changes, without a new
    /// traversal.
    #[test]
    fn recall_overlays_current_titles_and_remints_on_change() {
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, rx) = spawn_trail(wake, dir);
        let url = "https://example.test/page/42";
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: url.into(),
            transition: TraceTransition::UrlTyped,
            at_ms: 1,
        });

        for (query, title) in [
            ("field notes", "Field Notes"),
            ("harmony map", "Harmony Map"),
        ] {
            handle.command(TrailCommand::Recall {
                query: query.into(),
                limit: 5,
                sources: vec![RecallSource::new(url, title).unwrap()],
                config: RecallConfig::default(),
            });
            let update = rx.recv().unwrap();
            let Update::RecallHits {
                query: answered,
                hits,
            } = update
            else {
                panic!("title recall must answer with hits");
            };
            assert_eq!(answered, query);
            assert_eq!(hits.len(), 1, "{query:?} should match the current title");
            assert_eq!(hits[0].url, url);
            assert_eq!(hits[0].title.as_deref(), Some(title));
        }

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();
    }

    /// W6b's done-condition: a typed prefix recalls the page the user keeps
    /// going back to, not the page whose title repeats the query. The decoy
    /// wins on BM25 (three occurrences in a short title) and loses on
    /// behaviour (one link click, two hundred days ago).
    #[test]
    fn typed_prefix_recall_follows_frecency_over_title_overlap() {
        const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, rx) = spawn_trail(wake, dir);
        let target = "https://gazette.test/";
        let decoy = "https://elsewhere.test/gazette/gazette-gazette";
        let now = now_ms();

        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: decoy.into(),
            transition: TraceTransition::LinkClick,
            at_ms: now - 200 * DAY_MS,
        });
        for days_ago in 0..6 {
            handle.command(TrailCommand::Record {
                owner: "p".into(),
                url: target.into(),
                transition: TraceTransition::UrlTyped,
                at_ms: now - days_ago * DAY_MS,
            });
        }
        let sources = vec![
            RecallSource::new(target, "The Morning Paper").unwrap(),
            RecallSource::new(decoy, "Gazette Gazette Gazette").unwrap(),
        ];

        let recall = |config| {
            handle.command(TrailCommand::Recall {
                query: "gazette".into(),
                limit: 5,
                sources: sources.clone(),
                config,
            });
            let Update::RecallHits { hits, .. } = rx.recv().unwrap() else {
                panic!("recall must answer with hits");
            };
            hits
        };

        let lexical = recall(RecallConfig::default().with_frecency_weight(0.0));
        assert_eq!(
            lexical[0].url, decoy,
            "BM25 alone follows the repeated title term"
        );

        let behavioural = recall(RecallConfig::default());
        assert_eq!(
            behavioural[0].url, target,
            "frecency lifts the page the prefix was typed into six times"
        );

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();
    }
}
