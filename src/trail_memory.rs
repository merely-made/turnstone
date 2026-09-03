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
//! The same actor answers **recall** (W2): the omnibar lowers
//! `Effect::RecallQuery`, and [`TrailCommand::Recall`] mints BM25 plus an
//! optional token n-gram vector projection from the stored corpus (flushing
//! first, so this minute's pages are findable) and answers
//! `Update::RecallHits`. Current graph and recycle-bin titles overlay titleless
//! trace pages only while those derived indexes are minted; they do not rewrite
//! browsing history. The indexes are held here, never repaired — a corpus,
//! title projection, or vector-space setting that moved re-mints.
//! eidetic's concrete types stop at this boundary: the app sees `RecallHit`s,
//! the same rule the bin port follows with `DeletedNode`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use armillary::{ActorHandle, Emitter, Wake, spawn_named};
use eidetic::{
    BrowsingMemory, BrowsingTrace, PageRef, TraceEvent, TraceTransition, bootstrap_browsing_schema,
};
use eidetic_fjall::FjallStore;
use eidetic_search::{FusedHit, TrailIndex, fuse};
use esp::embed::{LexicalEmbeddingProvider, SemanticSearch};

use crate::action::{RecallHit, Update};

/// Traversals per stored trace segment. Segments are the flush granularity:
/// small enough that a crash loses minutes, large enough that a stored trace
/// is a meaningful corridor slice.
const SEGMENT_SIZE: usize = 32;

/// Receipt-backed size for the flat phrase-vector index. The index is minted
/// lazily and discarded with its session, so this is a cost choice rather than
/// durable format authority.
const PHRASE_VECTOR_DIMENSIONS: usize = 4_096;

/// Standard reciprocal-rank damping. The application setting exposes the only
/// ranking-relevant weight ratio: phrase vectors relative to BM25.
const RRF_K: f64 = 60.0;

/// Pull a wider head from each input before reducing to the omnibar row limit.
const FUSION_CANDIDATE_MULTIPLIER: usize = 4;

/// Live application settings consumed by the derived recall index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecallConfig {
    ngram_max_order: u8,
    vector_weight: f32,
}

impl RecallConfig {
    pub fn new(ngram_max_order: u8, vector_weight: f32) -> Self {
        Self {
            ngram_max_order: ngram_max_order.clamp(1, 3),
            vector_weight: if vector_weight.is_finite() {
                vector_weight.clamp(0.0, 4.0)
            } else {
                0.0
            },
        }
    }

    fn vector_enabled(self) -> bool {
        self.vector_weight > 0.0
    }

    fn token_ngram_orders(self) -> Vec<usize> {
        (1..=usize::from(self.ngram_max_order)).collect()
    }
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self::new(2, 0.0)
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
        }
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
        }
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

#[derive(Clone)]
struct RecallDocument {
    hit: RecallHit,
    text: String,
}

fn recall_documents(traces: &[BrowsingTrace]) -> BTreeMap<String, RecallDocument> {
    let mut documents = BTreeMap::<String, RecallDocument>::new();
    for trace in traces {
        for event in &trace.events {
            let url = event.to.url.clone();
            let hit = RecallHit {
                url: url.clone(),
                title: event.to.title.clone(),
                at_ms: event.at_ms,
            };
            let text = match &hit.title {
                Some(title) => format!("{title} {url}"),
                None => url.clone(),
            };
            let candidate = RecallDocument { hit, text };
            if let Some(current) = documents.get_mut(&url) {
                let replace = candidate.hit.at_ms > current.hit.at_ms
                    || candidate.hit.at_ms == current.hit.at_ms
                        && candidate.hit.title > current.hit.title;
                if replace {
                    *current = candidate;
                }
            } else {
                documents.insert(url, candidate);
            }
        }
    }
    documents
}

type PhraseSearch = SemanticSearch<String, LexicalEmbeddingProvider>;

/// One disposable projection over the authoritative trace corpus.
struct RecallIndex {
    lexical: TrailIndex,
    vector: Option<PhraseSearch>,
    vector_order: Option<u8>,
    documents: BTreeMap<String, RecallDocument>,
}

impl RecallIndex {
    fn mint(dir: &Path, traces: &[BrowsingTrace], config: RecallConfig) -> Result<Self, String> {
        let lexical =
            TrailIndex::rebuild(index_dir(dir), traces).map_err(|err| format!("re-mint: {err}"))?;
        let documents = recall_documents(traces);
        let vector = if config.vector_enabled() {
            let provider = LexicalEmbeddingProvider::with_token_ngram_orders(
                PHRASE_VECTOR_DIMENSIONS,
                config.token_ngram_orders(),
            )
            .map_err(|err| format!("phrase provider: {err}"))?;
            let mut search = SemanticSearch::new(provider);
            let items: Vec<(String, &str)> = documents
                .iter()
                .map(|(url, document)| (url.clone(), document.text.as_str()))
                .collect();
            search
                .ingest_batch(&items)
                .map_err(|err| format!("phrase ingest: {err}"))?;
            Some(search)
        } else {
            None
        };
        Ok(Self {
            lexical,
            vector,
            vector_order: config.vector_enabled().then_some(config.ngram_max_order),
            documents,
        })
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
        let Some(vector) = self.vector.as_ref() else {
            return Err("phrase index is not minted".to_string());
        };
        let mut seen = HashSet::new();
        let mut lexical_urls = Vec::new();
        for hit in &lexical_hits {
            if seen.insert(hit.url.clone()) {
                lexical_urls.push(hit.url.clone());
            }
        }

        // Ask the flat index for every record before deterministic tie-breaking.
        // Truncating inside its HashMap-backed ranking could select an arbitrary
        // subset of equal-score URLs.
        let mut vector_hits = vector
            .search(query, vector.len().max(1))
            .map_err(|err| format!("phrase search: {err}"))?;
        vector_hits.retain(|(_, score)| score.is_finite() && *score > 0.0);
        vector_hits.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        vector_hits.truncate(candidate_limit);
        let vector_urls: Vec<String> = vector_hits.into_iter().map(|(url, _)| url).collect();

        Ok(fuse(
            &lexical_urls,
            &vector_urls,
            RRF_K,
            (1.0, f64::from(config.vector_weight)),
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

        // Zero influence is the compatibility path: preserve TrailIndex's
        // ranking and metadata exactly, and do not require a vector projection.
        if !config.vector_enabled() {
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
                self.documents
                    .get(&fused.url)
                    .map(|document| document.hit.clone())
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
    if request.config.vector_enabled()
        && index.as_ref().and_then(|index| index.vector_order)
            != Some(request.config.ngram_max_order)
    {
        *stale = true;
    }
    if *stale || index.is_none() {
        flush(store, memory);
        let traces = traces_with_titles(memory, request.sources);
        *index = Some(RecallIndex::mint(dir, &traces, request.config)?);
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
            // The derived lexical/vector projection and whether the corpus has
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
                    }
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
                    }
                    TrailCommand::Flush => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                    }
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
                    }
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
                    }
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

    /// Enabling the phrase vector through live settings re-mints the derived
    /// projection and corrects a bag-of-words order tie.
    #[test]
    fn phrase_vector_influence_remints_and_steers_recall() {
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, rx) = spawn_trail(wake, dir);
        let decoy = "https://a.example/reversed";
        let target = "https://z.example/ordered";
        for (at_ms, url) in [(1, decoy), (2, target)] {
            handle.command(TrailCommand::Record {
                owner: "p".into(),
                url: url.into(),
                transition: TraceTransition::UrlTyped,
                at_ms,
            });
        }
        let sources = vec![
            RecallSource::new(decoy, "Folder Downloads Open").unwrap(),
            RecallSource::new(target, "Open Downloads Folder").unwrap(),
        ];

        handle.command(TrailCommand::Recall {
            query: "open downloads folder".into(),
            limit: 2,
            sources: sources.clone(),
            config: RecallConfig::default(),
        });
        let Update::RecallHits { hits: lexical, .. } = rx.recv().unwrap() else {
            panic!("lexical recall must answer");
        };
        assert_eq!(lexical[0].url, decoy, "BM25 ignores phrase order");

        handle.command(TrailCommand::Recall {
            query: "open downloads folder".into(),
            limit: 2,
            sources,
            config: RecallConfig::new(2, 2.0),
        });
        let Update::RecallHits { hits: hybrid, .. } = rx.recv().unwrap() else {
            panic!("hybrid recall must answer");
        };
        assert_eq!(hybrid[0].url, target, "bigrams supply phrase order");

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();
    }
}
