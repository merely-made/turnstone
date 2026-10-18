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
use eidetic_search::{CandidateIndex, FusedHit, IndexConfig, Ranking, TrailIndex, fuse_many};
use muniment::FjallStore;

use crate::action::{RecallHit, StoredSourceDocument, Update};
mod saved_sources;

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
        reply: std::sync::mpsc::SyncSender<Result<Vec<crate::saved_sources::Entry>, String>>,
    },
    ResearchSource {
        context: crate::research::Context,
        target: crate::saved_sources::Target,
        reply: std::sync::mpsc::SyncSender<Result<crate::saved_sources::Content, String>>,
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
    RecordText { url: String, text: String },
    /// An explicit request to preserve one fetched HTML response. This remains
    /// separate from `RecordText`: recall text is never source evidence.
    CaptureSourceDocument {
        capture_generation: uuid::Uuid,
        request: crate::source_capture::CaptureRequest,
        directory: PathBuf,
        document: Option<crate::content::FetchedDocument>,
    },
    /// Commit the durable request receipt after owning facet/graph save ack.
    AcknowledgeSourceCapture { capture_generation: uuid::Uuid, request: crate::source_capture::CaptureRequest, directory: PathBuf },
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
    Reopen { directory: PathBuf, capture_generation: uuid::Uuid },
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
fn open_memory(
    dir: &Path,
    capture_library: &crate::place::captured_collection::LocalCaptureLibrary,
) -> Option<(FjallStore, BrowsingMemory)> {
    if let Err(err) = std::fs::create_dir_all(dir) {
        capture_library.unavailable(format!("create session capture directory: {err}"));
        tracing::warn!(%err, dir = %dir.display(), "trail memory: create dir failed; capture disabled until reopen");
        return None;
    }
    let mut store = match FjallStore::open(dir) {
        Ok(store) => store,
        Err(err) => {
            capture_library.unavailable(format!("open session capture store: {err}"));
            tracing::warn!(%err, dir = %dir.display(), "trail memory: open failed; capture disabled until reopen");
            return None;
        },
    };
    if let Err(err) = pollster::block_on(bootstrap_browsing_schema(&mut store)) {
        capture_library.unavailable(format!("bootstrap browsing schema: {err}"));
        tracing::warn!(%err, "trail memory: schema bootstrap failed; capture disabled until reopen");
        return None;
    }
    match pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)) {
        Ok(memory) => {
            if let Err(error) = crate::place::captured_collection::refresh_local_capture_library(
                &mut store,
                dir,
                capture_library,
            ) {
                capture_library.unavailable(error);
            }
            Some((store, memory))
        },
        Err(err) => {
            capture_library.unavailable(format!("load browsing corpus: {err}"));
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

/// Enforce L1 at the storage boundary as well as at browse enrichment: any
/// queued or older caller of `RecordText` is harmless. Existing PageText
/// records remain untouched and readable through the legacy Answers adapter.
fn keep_page_text(_store: &FjallStore, url: &str, _text: &str) {
    tracing::debug!(%url, "trail memory: ordinary page text withheld under L1");
}

/// Store the exact acquired bytes and the Fleece annotation that describes a
/// canonical-text projection of those bytes. This is invoked only by the
/// explicit action path, never by fetch enrichment or trail recall.
#[cfg(test)]
fn capture_source_document(
    store: &mut FjallStore,
    document: &crate::content::FetchedDocument,
) -> Result<StoredSourceDocument, String> {
    capture_source_document_in(store, document)
}

pub(crate) fn capture_source_document_in(
    store: &mut dyn eidetic::Store,
    document: &crate::content::FetchedDocument,
) -> Result<StoredSourceDocument, String> {
    use eidetic::models::OpaqueBlob;
    use eidetic::{
        ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp,
        TrustEnvelope, TrustLevel,
    };
    use fleece::eidetic_bridge::{CaptureDomMode, CaptureEvidenceV1};

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
    let raw_manifest = eidetic::ManifestId::of_blob(&document.bytes);
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
    let record = fleece::FleeceAnnotationRecord::from_fleece(
        fleece::CaptureIdentity::from_evidence(evidence)
            .map_err(|error| format!("source capture identity: {error}"))?,
        &extracted,
        &anchor,
    )
    .map_err(|error| format!("Fleece annotation: {error}"))?;
    // Eidetic IDs address payload bytes, not (schema, bytes). Plan and check
    // every typed role before any deposit, so one role cannot replace another
    // valid artifact's mutable manifest at the same content address.
    use eidetic::TypedPayload;
    let annotation_bytes = record
        .serialize_to_bytes()
        .map_err(|error| format!("Fleece annotation encoding: {error}"))?;
    let annotation_manifest = eidetic::ManifestId::of_blob(&annotation_bytes);
    let schema_manifest = fleece::FLEECE_ANNOTATION_SCHEMA_REF.0;
    if raw_manifest == annotation_manifest
        || raw_manifest == schema_manifest
        || annotation_manifest == schema_manifest
    {
        return Err("source capture has incompatible typed roles at one content address".into());
    }
    let raw_exists = preflight_source_artifact(
        store,
        raw_manifest,
        OpaqueBlob::schema_ref(),
        PrivacyClass::LocalOnly,
        Some(&document.bytes),
    )?;
    let annotation_exists = preflight_source_artifact(
        store,
        annotation_manifest,
        fleece::FleeceAnnotationRecord::schema_ref(),
        PrivacyClass::LocalOnly,
        Some(&annotation_bytes),
    )?;
    preflight_source_artifact(
        store,
        schema_manifest,
        *eidetic::META_SCHEMA_REF,
        PrivacyClass::PublicPortable,
        None,
    )?;
    if raw_exists {
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, raw_manifest))
            .map_err(|error| format!("raw source preflight: {error}"))?
            .unwrap();
        if !matches!(
            manifest.provenance.origin,
            ProvenanceOrigin::Imported { .. }
        ) {
            return Err(
                "raw source preflight: incompatible existing acquisition provenance".into(),
            );
        }
    }
    if annotation_exists {
        let stored = StoredSourceDocument {
            raw_manifest: raw_manifest.to_string(),
            annotation_manifest: annotation_manifest.to_string(),
        };
        verify_source_document(store, &stored)?;
    }
    if !raw_exists {
        pollster::block_on(eidetic::save_typed(
            store,
            &OpaqueBlob(document.bytes.clone()),
            Vec::new(),
            PrivacyClass::LocalOnly,
            ProvenanceRecord {
                origin: ProvenanceOrigin::Imported {
                    source: document.effective_url.clone().unwrap(),
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
    }
    pollster::block_on(fleece::bootstrap_fleece_annotation_schema(store))
        .map_err(|error| format!("Fleece schema bootstrap: {error}"))?;
    if !annotation_exists {
        pollster::block_on(fleece::save_fleece_annotation(
            store,
            &record,
            document.acquired_at_ms,
        ))
        .map_err(|error| format!("Fleece annotation deposit: {error}"))?;
    } else {
        // Reusing a complete annotation preserves its exact manifest metadata.
        // Its resource association is independently idempotent and may have
        // been the interrupted final write of the previous deposit attempt.
        pollster::block_on(eidetic::ResourceCaptureStore::new(&*store).record(
            &record.extraction.capture.canonical_source,
            record.extraction.capture.capture_hash,
            eidetic::CaptureContent::Acquired,
            document.acquired_at_ms,
        ))
        .map_err(|error| format!("Fleece capture association: {error}"))?;
    }
    let stored = StoredSourceDocument {
        raw_manifest: raw_manifest.to_string(),
        annotation_manifest: annotation_manifest.to_string(),
    };
    verify_source_document(store, &stored)?;
    Ok(stored)
}

/// Preserve an existing typed role exactly. The trail actor serializes these
/// checks with subsequent writes; no transactional backend guarantee is assumed.
fn preflight_source_artifact(
    store: &mut dyn eidetic::Store,
    id: eidetic::ManifestId,
    schema: eidetic::SchemaRef,
    privacy: eidetic::PrivacyClass,
    expected: Option<&[u8]>,
) -> Result<bool, String> {
    let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, id))
        .map_err(|error| format!("source artifact preflight manifest: {error}"))?;
    if let Some(manifest) = &manifest {
        if manifest.id != id
            || manifest.content_hash != id.0
            || manifest.schema != schema
            || manifest.privacy != privacy
        {
            return Err(format!(
                "source artifact preflight: incompatible existing manifest role at {id}"
            ));
        }
    }
    let key = format!("blob:{}", id.0.to_hex());
    let bytes = pollster::block_on(store.get(&key))
        .map_err(|error| format!("source artifact preflight payload: {error}"))?;
    if let Some(bytes) = &bytes {
        if eidetic::Hash::of(bytes) != id.0 || expected.is_some_and(|expected| expected != bytes) {
            return Err(format!(
                "source artifact preflight: payload integrity mismatch at {id}"
            ));
        }
    }
    if let Some(manifest) = &manifest {
        let bytes = bytes
            .as_ref()
            .ok_or_else(|| format!("source artifact preflight: missing local payload at {id}"))?;
        if manifest.byte_size != bytes.len() as u64
            || !manifest.sources.iter().any(|source| matches!(source, eidetic::BlobSource::Local { key: local } if local == &key))
        {
            return Err(format!("source artifact preflight: inconsistent local payload at {id}"));
        }
    }
    Ok(manifest.is_some())
}

/// Admit only the exact locally addressed raw/annotation pair. Eidetic checks
/// typed payload hashes and Fleece checks annotation facts; the host additionally
/// binds the requested key to the manifest identity and authoritative raw source.
/// No fetcher can repair missing or damaged saved evidence at this boundary.
pub(crate) fn verify_source_document(
    store: &mut dyn eidetic::Store,
    stored: &StoredSourceDocument,
) -> Result<(), String> {
    use eidetic::{ManifestId, PrivacyClass, ProvenanceOrigin};
    use fleece::eidetic_bridge::CaptureDomMode;

    fn manifest_id(value: &str) -> Result<ManifestId, String> {
        serde_json::from_value(serde_json::Value::String(value.to_owned()))
            .map_err(|error| format!("source manifest identity: {error}"))
    }

    fn checked_manifest(
        store: &mut dyn eidetic::Store,
        id: ManifestId,
    ) -> Result<eidetic::BlobManifest, String> {
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, id))
            .map_err(|error| format!("source manifest read: {error}"))?
            .ok_or_else(|| format!("source manifest {id} is missing"))?;
        if manifest.id != id || manifest.content_hash != id.0 {
            return Err(format!("source manifest identity mismatch at {id}"));
        }
        if manifest.privacy != PrivacyClass::LocalOnly {
            return Err(format!("source manifest {id} is not private local evidence"));
        }
        let key = format!("blob:{}", id.0.to_hex());
        if !manifest.sources.iter().any(|source| {
            matches!(source, eidetic::BlobSource::Local { key: local } if local == &key)
        }) {
            return Err(format!("source manifest {id} has no exact local payload"));
        }
        let bytes = pollster::block_on(store.get(&key))
            .map_err(|error| format!("source payload read: {error}"))?
            .ok_or_else(|| format!("source payload {id} is missing"))?;
        if eidetic::Hash::of(&bytes) != id.0 || bytes.len() as u64 != manifest.byte_size {
            return Err(format!("source payload integrity mismatch at {id}"));
        }
        Ok(manifest)
    }

    let raw_id = manifest_id(&stored.raw_manifest)?;
    let annotation_id = manifest_id(&stored.annotation_manifest)?;
    let raw_manifest = checked_manifest(store, raw_id)?;
    let annotation_manifest = checked_manifest(store, annotation_id)?;
    let record = pollster::block_on(fleece::load_fleece_annotation(store, annotation_id))
        .map_err(|error| format!("source annotation validation: {error}"))?
        .ok_or_else(|| "source annotation is missing".to_string())?;
    let evidence = record.extraction.capture.evidence.as_ref()
        .ok_or_else(|| "source annotation lacks acquisition evidence".to_string())?;
    let html = evidence.response_content_type.as_deref().is_some_and(|content_type| {
        content_type.split(';').next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/html"))
    });
    if !evidence.has_authoritative_acquisition_context()
        || evidence.dom_mode != CaptureDomMode::Source
        || !html
        || evidence.raw_manifest != Some(raw_id)
        || evidence.capture_hash != raw_id.0
        || evidence.replay_manifest.is_some()
        // A raw blob is content addressed: identical bytes acquired from two
        // URLs/times share it. Acquisition identity belongs to the annotation,
        // not the mutable metadata of that shared raw manifest.
        || !matches!(raw_manifest.provenance.origin, ProvenanceOrigin::Imported { .. })
        || annotation_manifest.created_at != evidence.captured_at
        || annotation_manifest.provenance.generated_at != evidence.captured_at
        || annotation_manifest.provenance.origin != ProvenanceOrigin::Generated
    {
        return Err("source annotation and raw acquisition provenance disagree".to_string());
    }
    pollster::block_on(eidetic::load_typed::<eidetic::models::OpaqueBlob>(
        store, &mut eidetic::NoFetcher, raw_id,
    ))
    .map_err(|error| format!("raw source validation: {error}"))?
    .ok_or_else(|| "raw source is missing".to_string())?;
    Ok(())
}

#[cfg(test)]
#[path = "trail_memory/source_capture_tests.rs"]
mod source_capture_tests;

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
            IndexConfig {
                persist: false,
                ..IndexConfig::default()
            },
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
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
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
            .filter_map(|fused| self.documents.get(&fused.url).cloned())
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
    spawn_trail_with_capture_library(
        wake,
        dir,
        crate::place::captured_collection::LocalCaptureLibrary::default(),
    )
}

/// Spawn the trail actor with the capture library consumed by the place
/// worker. Only plain records cross this seam; the Fjall handle stays here.
pub(crate) fn spawn_trail_with_capture_library(
    wake: Wake,
    dir: PathBuf,
    capture_library: crate::place::captured_collection::LocalCaptureLibrary,
) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_trail_for_session(wake, Some(dir), capture_library, uuid::Uuid::nil())
}

pub(crate) fn spawn_trail_for_session(
    wake: Wake,
    dir: Option<PathBuf>,
    capture_library: crate::place::captured_collection::LocalCaptureLibrary,
    capture_generation: uuid::Uuid,
) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_named(
        "trail-memory",
        wake,
        move |commands, out: Emitter<Update>| {
            let mut state = dir
                .as_ref()
                .and_then(|dir| open_memory(dir, &capture_library));
            let mut current_dir = dir.unwrap_or_default();
            let mut current_capture_generation = capture_generation;
            emit_capture_recovery(&mut state, &current_dir, current_capture_generation, &out);
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
                    TrailCommand::LoadSavedSources { generation, request, session, admissions, directory } => {
                        let owner = current_dir.parent().and_then(Path::file_name).and_then(|name| name.to_str());
                        let result = if generation != current_capture_generation || directory != current_dir
                            || owner != Some(session.as_uuid().to_string().as_str())
                            || admissions.iter().any(|admission| admission.key.session != session)
                        { Err("Saved source search belongs to another session opening or store".into()) }
                        else { match state.as_mut() {
                            Some((store, _)) => Ok(saved_sources::load(store, &admissions)),
                            None => Err("The session evidence store is unavailable".into()),
                        }};
                        out.emit(Update::SavedSourcesLoaded { generation, request, session, admissions, result });
                    },
                    TrailCommand::ReadSavedSource { generation, request, node, url, target, admission, directory } => {
                        let owner = current_dir.parent().and_then(Path::file_name).and_then(|name| name.to_str());
                        let result = if generation != current_capture_generation || directory != current_dir
                            || owner != Some(target.key.session.as_uuid().to_string().as_str())
                            || admission.key != target.key
                        { Err("Saved source reading belongs to another session opening or attachment".into()) }
                        else { match state.as_mut() {
                            Some((store, _)) => saved_sources::resolve(store, &admission, &target),
                            None => Err("The session evidence store is unavailable".into()),
                        }};
                        out.emit(Update::SavedSourceRead { generation, request, node, url, target, result });
                    },
                    TrailCommand::ResearchSnapshot { context, reply } => {
                        let valid = context.generation == current_capture_generation
                            && inference_owner(&current_dir).ok()
                                == Some(format!("turnstone/{}", context.session.as_uuid()))
                            && context
                                .admissions
                                .iter()
                                .all(|a| a.key.session == context.session);
                        let result = if !valid {
                            Err("The saved-source owner belongs to another session opening.".into())
                        } else {
                            state
                                .as_mut()
                                .map(|(store, _)| saved_sources::load(store, &context.admissions))
                                .ok_or_else(|| "The session evidence store is unavailable.".into())
                        };
                        let _ = reply.send(result);
                    },
                    TrailCommand::ResearchSource {
                        context,
                        target,
                        reply,
                    } => {
                        let valid = context.generation == current_capture_generation
                            && inference_owner(&current_dir).ok()
                                == Some(format!("turnstone/{}", context.session.as_uuid()))
                            && target.key.session == context.session;
                        let result = if !valid {
                            Err("The cited source belongs to another session opening.".into())
                        } else if let Some(admission) = context
                            .admissions
                            .iter()
                            .find(|a| a.key == target.key && a.error.is_none())
                        {
                            match state.as_mut() {
                                Some((store, _)) => {
                                    saved_sources::resolve(store, admission, &target)
                                },
                                None => Err("The session evidence store is unavailable.".into()),
                            }
                        } else {
                            Err("The cited saved source was withdrawn from this session.".into())
                        };
                        let _ = reply.send(result);
                    },
                    TrailCommand::InferenceSnapshot { owner, reply } => {
                        let result = inference_snapshot(
                            state.as_ref().map(|(store, _)| store),
                            &current_dir,
                            &owner,
                        );
                        let _ = reply.send(result);
                    },
                    TrailCommand::InferenceSource {
                        source,
                        version,
                        reply,
                    } => {
                        let result = inference_source(
                            state.as_ref().map(|(store, _)| store),
                            &current_dir,
                            &source,
                            &version,
                        );
                        let _ = reply.send(result);
                    },
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
                        if let Some((store, _)) = state.as_ref() {
                            keep_page_text(store, &url, &text);
                        }
                    },
                    TrailCommand::CaptureSourceDocument { capture_generation, request, directory, document } => {
                        let admitted_session = current_dir.parent().and_then(Path::file_name).and_then(|name| name.to_str());
                        let result = if directory != current_dir
                            || capture_generation != current_capture_generation
                            || admitted_session != Some(request.session.as_uuid().to_string().as_str())
                        {
                            Err("the capture actor belongs to another session opening or directory".into())
                        } else {
                            match state.as_mut() {
                                Some((store, _)) => source_capture_receipts::capture_request(store, &request, document.as_ref()),
                                None => Err("the session Eidetic store is not open".into()),
                            }
                        };
                        out.emit(Update::SourceDocumentCaptured { capture_generation, request, result });
                    },
                    TrailCommand::AcknowledgeSourceCapture { capture_generation, request, directory } => {
                        let admitted_session = current_dir.parent().and_then(Path::file_name).and_then(|name| name.to_str());
                        let result = if directory != current_dir
                            || capture_generation != current_capture_generation
                            || admitted_session != Some(request.session.as_uuid().to_string().as_str())
                        {
                            Err("capture save acknowledgement belongs to another session opening or store".into())
                        } else {
                            match state.as_mut() {
                                Some((store, _)) => source_capture_receipts::acknowledge_saved(store, &request),
                                None => Err("the session Eidetic store is not open".into()),
                            }
                        };
                        if result.is_ok() {
                            if let Some((store, _)) = state.as_mut()
                                && let Err(error) = crate::place::captured_collection::refresh_local_capture_library(store, &current_dir, &capture_library)
                            { capture_library.unavailable(error); }
                        }
                        out.emit(Update::SourceCaptureSaved { capture_generation, request: request.id, result });
                    },
                    TrailCommand::Flush => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                    },
                    TrailCommand::Reopen { directory: dir, capture_generation } => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                        last_to.clear();
                        // The index belongs to the departing session's corpus;
                        // the adopted one mints its own on first recall.
                        index = None;
                        index_stale = true;
                        indexed_sources.clear();
                        // A warm reopen can target this same directory. Release
                        // its Fjall lock before evaluating the new open.
                        drop(state.take());
                        state = open_memory(&dir, &capture_library);
                        current_dir = dir;
                        current_capture_generation = capture_generation;
                        emit_capture_recovery(&mut state, &current_dir, current_capture_generation, &out);
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
                        capture_library.unavailable("the session capture store is released");
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

    /// L1's retention boundary: an ordinary body is absent while address and
    /// title remain recallable from the same traversal.
    #[test]
    fn ordinary_page_body_is_not_retained_but_address_and_title_recall() {
        const BODY: &str = "A kestrel hovers on a fixed point of air, head still while \
                            the wings work, and stoops only when the vole below commits \
                            to a run across the open verge.";
        let url = "https://field.test/notes/7";
        let dir = temp_dir();
        let store = FjallStore::open(&dir).unwrap();
        // Exercise the actor's boundary directly, including stale queued work.
        keep_page_text(&store, url, BODY);
        let texts = stored_page_texts(&store);
        assert!(texts.is_empty(), "ordinary fetch text never enters PageText");

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
        assert_eq!(index.receipt.pages_with_text, 0);

        // "kestrel" is in neither the URL nor the title.
        assert!(!url.contains("kestrel"));
        let body_hits = index
            .search(
                "kestrel",
                5,
                RecallConfig::default().with_frecency_weight(0.0),
            )
            .unwrap();
        assert!(body_hits.is_empty(), "body-only terms are not searchable");

        let title_hits = index.search(
            "Notes",
            5,
            RecallConfig::default().with_frecency_weight(0.0),
        ).unwrap();
        assert_eq!(title_hits.len(), 1, "title recall remains available");
        assert_eq!(title_hits[0].url, url, "address remains attached to recall");
        let address_hits = index.search(
            "field",
            5,
            RecallConfig::default().with_frecency_weight(0.0),
        ).unwrap();
        assert_eq!(address_hits.len(), 1, "address recall remains available");
        assert_eq!(address_hits[0].url, url);
    }

    #[test]
    fn legacy_page_text_survives_ordinary_fetch_without_rewrite() {
        let url = "https://field.test/legacy";
        let dir = temp_dir();
        let store = FjallStore::open(&dir).unwrap();
        let legacy = "legacy amberstone body";
        pollster::block_on(PageTextStore::new(&store).put(url, legacy, 17)).unwrap();
        let before = stored_page_texts(&store);

        keep_page_text(&store, url, "new automatic body must be ignored");

        assert_eq!(stored_page_texts(&store), before);
        assert_eq!(stored_page_texts(&store).get(url), Some(legacy));
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
            fleece::FleeceAnnotationRecord,
        >(&mut reopened))
        .unwrap();
        assert_eq!(manifests.len(), 1);
        let record = pollster::block_on(fleece::load_fleece_annotation(
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

/// A read capability over the actor, not a second opener of its Fjall files.
pub(crate) struct TrailInferenceOwner(pub std::sync::Arc<ActorHandle<TrailCommand>>);
impl crate::inference_service::RetainedSourceOwner for TrailInferenceOwner {
    fn snapshot(
        &self,
        owner: &str,
    ) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.0.command(TrailCommand::InferenceSnapshot {
            owner: owner.into(),
            reply,
        }) {
            return Err("The retained-text owner is closed.".into());
        }
        receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The retained-text owner did not answer.".to_string())?
    }
    fn resolve(
        &self,
        source: &eidetic::models::recall::RecallSourceId,
        version: &str,
    ) -> Result<String, String> {
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.0.command(TrailCommand::InferenceSource {
            source: source.clone(),
            version: version.into(),
            reply,
        }) {
            return Err("The retained-text owner is closed.".into());
        }
        receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The retained-text owner did not answer.".to_string())?
    }
}
fn inference_owner(dir: &Path) -> Result<String, String> {
    let session = dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .and_then(|n| uuid::Uuid::parse_str(n).ok())
        .ok_or("The active browsing session has no retained-text owner.")?;
    Ok(format!("turnstone/{session}"))
}
fn inference_snapshot(
    store: Option<&FjallStore>,
    dir: &Path,
    owner: &str,
) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
    use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
    if inference_owner(dir)? != owner {
        return Err("This answer belongs to another browsing session.".into());
    }
    let store = store.ok_or("The retained-text owner is not open.")?;
    pollster::block_on(async {
        let texts = PageTextStore::new(store);
        let mut out = Vec::new();
        for entry in texts.references().await.map_err(|e| e.to_string())? {
            let text = texts
                .text_at(&entry)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("A retained source version is unavailable.")?;
            out.push(RetainedSourceText {
                source: RecallSourceId {
                    kind: RecallSourceKind::PageText,
                    owner: Some(owner.into()),
                    source: entry.url,
                    node: None,
                },
                version: format!("blake3:{}", entry.blob),
                title: None,
                text,
            });
        }
        Ok(out)
    })
}
fn inference_source(
    store: Option<&FjallStore>,
    dir: &Path,
    source: &eidetic::models::recall::RecallSourceId,
    version: &str,
) -> Result<String, String> {
    use eidetic::models::recall::RecallSourceKind;
    if source.owner.as_deref() != Some(inference_owner(dir)?.as_str())
        || source.kind != RecallSourceKind::PageText
        || source.node.is_some()
    {
        return Err("The cited source belongs to another owner.".into());
    }
    let hash = eidetic::Hash::parse(version).map_err(|_| "The cited source version is invalid.")?;
    let store = store.ok_or("The retained-text owner is not open.")?;
    let reference = eidetic::browsing::text::PageTextRef {
        url: source.source.clone(),
        blob: hash.to_hex(),
        stored_at_ms: 0,
    };
    pollster::block_on(PageTextStore::new(store).text_at(&reference))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "The cited retained version is unavailable.".into())
}

#[cfg(test)]
mod inference_owner_tests {
    use super::*;
    use crate::inference_service::RetainedSourceOwner;
    #[test]
    fn the_actor_lends_versions_without_reopening_its_store_or_substituting_a_new_head() {
        let temp = tempfile::tempdir().unwrap();
        let session = uuid::Uuid::new_v4();
        let dir = temp
            .path()
            .join("sessions")
            .join(session.to_string())
            .join("memory");
        std::fs::create_dir_all(&dir).unwrap();
        pollster::block_on(PageTextStore::new(&FjallStore::open(&dir).unwrap()).put(
            "https://retained.test/note",
            "nebula amberstone",
            1,
        )).unwrap();
        let (handle, _updates) =
            spawn_trail_for_session(std::sync::Arc::new(|| {}), Some(dir), Default::default(), uuid::Uuid::nil());
        let owner = TrailInferenceOwner(std::sync::Arc::new(handle));
        let namespace = format!("turnstone/{session}");
        let old = owner.snapshot(&namespace).unwrap().remove(0);
        assert_eq!(old.text, "nebula amberstone");
        owner.0.command(TrailCommand::RecordText {
            url: "https://retained.test/note".into(),
            text: "nebula indigo".into(),
        });
        let still_legacy = owner.snapshot(&namespace).unwrap().remove(0);
        assert_eq!(old.version, still_legacy.version);
        assert_eq!(still_legacy.text, "nebula amberstone");
        assert_eq!(
            owner.resolve(&old.source, &old.version).unwrap(),
            "nebula amberstone"
        );
        assert!(owner.snapshot("turnstone/another-session").is_err());
        let (ack, receive) = std::sync::mpsc::sync_channel(1);
        owner.0.command(TrailCommand::Release(ack));
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(owner.snapshot(&namespace).is_err());
        match std::sync::Arc::try_unwrap(owner.0) {
            Ok(actor) => actor.join(),
            Err(_) => panic!("unexpected actor borrower"),
        }
    }
}

#[path = "trail_memory/source_capture_receipts.rs"]
mod source_capture_receipts;

fn emit_capture_recovery(
    state: &mut Option<(FjallStore, BrowsingMemory)>, directory: &Path, capture_generation: uuid::Uuid, out: &Emitter<Update>,
) {
    let Some((store, _)) = state.as_mut() else { return; };
    match source_capture_receipts::recover_receipts(store) {
        Ok(receipts) => for receipt in receipts {
            // Directory admission comes from the live actor, not saved JSON.
            let owner = directory.parent().and_then(Path::file_name).and_then(|name| name.to_str());
            if owner != Some(receipt.request.session.as_uuid().to_string().as_str()) {
                out.emit(Update::SourceCaptureRecoveryFailed { capture_generation, error: "capture receipt owner does not match the admitted session store".into() });
                continue;
            }
            if receipt.saved {
                if let Err(error) = source_capture_receipts::verify_receipt(store, &receipt) {
                    let id = receipt.request.id;
                    out.emit(Update::SourceCaptureRecovered { capture_generation, request: receipt.request, stored: receipt.stored });
                    out.emit(Update::SourceCaptureRecoveryFailed { capture_generation, error: format!("capture request {id} verification failed: {error}") });
                } else if let Some(stored) = receipt.stored {
                    out.emit(Update::SourceCaptureRevalidated { capture_generation, request: receipt.request, stored });
                } else {
                    out.emit(Update::SourceCaptureRecoveryFailed { capture_generation, error: "saved capture receipt has no artifacts".into() });
                }
            } else {
                out.emit(Update::SourceCaptureRecovered { capture_generation, request: receipt.request, stored: receipt.stored });
            }
        },
        Err(error) => out.emit(Update::SourceCaptureRecoveryFailed { capture_generation, error }),
    }
}

/// Capture-specific owner. It never delegates to the PageText compatibility resolver.
pub(crate) struct CaptureInferenceOwner {
    pub trail: std::sync::Arc<ActorHandle<TrailCommand>>,
    pub bridge: crate::research::Bridge,
}
impl crate::inference_service::RetainedSourceOwner for CaptureInferenceOwner {
    fn snapshot(
        &self,
        owner: &str,
    ) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
        let context = self.bridge.context()?;
        if owner != crate::research::namespace(context.session) {
            return Err("The answer belongs to another saved-source owner.".into());
        }
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.trail.command(TrailCommand::ResearchSnapshot {
            context: context.clone(),
            reply,
        }) {
            return Err("The saved-source owner is closed.".into());
        }
        let entries = receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The saved-source owner did not answer.")??;
        self.bridge.loaded(&context, &entries)?;
        let mut sources = Vec::new();
        for entry in entries {
            if let Some(content) = entry.content {
                let target = crate::saved_sources::Target {
                    key: entry.admission.key,
                    text_hash: content.text_hash.clone(),
                    selector: None,
                };
                let source = crate::research::source_id(&target)?;
                if !sources
                    .iter()
                    .any(|entry: &eidetic::models::recall::RetainedSourceText| {
                        entry.source == source
                    })
                {
                    sources.push(eidetic::models::recall::RetainedSourceText {
                        source,
                        version: content.text_hash,
                        title: Some(content.title),
                        text: content.text,
                    });
                }
            }
        }
        Ok(sources)
    }
    fn resolve(
        &self,
        source: &eidetic::models::recall::RecallSourceId,
        version: &str,
    ) -> Result<String, String> {
        let target = crate::research::target(source, version)?;
        let context = self.bridge.context()?;
        if source.owner.as_deref() != Some(crate::research::namespace(context.session).as_str()) {
            return Err("The citation belongs to another saved-source owner.".into());
        }
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.trail.command(TrailCommand::ResearchSource {
            context: context.clone(),
            target,
            reply,
        }) {
            return Err("The saved-source owner is closed.".into());
        }
        let content = receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The saved-source owner did not answer.")??;
        if self.bridge.context()? != context {
            return Err("The cited source session or admissions changed.".into());
        }
        Ok(content.text)
    }
    fn open_source(
        &self,
        opened: &crate::inference_service::OpenSource,
        generation: Option<uuid::Uuid>,
    ) -> Result<(), String> {
        let context = self.bridge.context()?;
        if Some(context.generation) != generation
            || opened.retained.source.owner.as_deref()
                != Some(crate::research::namespace(context.session).as_str())
        {
            return Err("The citation belongs to an earlier session opening.".into());
        }
        let mut target =
            crate::research::target(&opened.retained.source, &opened.retained.version)?;
        if target.text_hash != eidetic::Hash::of(opened.retained.text.as_bytes()).to_string()
            || opened.selector.start >= opened.selector.end
            || opened.selector.end > opened.retained.text.chars().count()
        {
            return Err("The cited source text or Unicode range is invalid.".into());
        }
        target.selector = Some(crate::saved_sources::Selector {
            start: opened.selector.start,
            end: opened.selector.end,
        });
        if !context
            .admissions
            .iter()
            .any(|admission| admission.key == target.key && admission.error.is_none())
        {
            return Err("The cited saved source was withdrawn from this session.".into());
        }
        self.bridge
            .send(crate::research::Intent::Citation { context, target });
        Ok(())
    }
    fn choices(&self) -> Vec<crate::research::Choice> {
        self.bridge.choices()
    }
    fn generation(&self) -> Option<uuid::Uuid> {
        self.bridge.context().ok().map(|c| c.generation)
    }
}

#[cfg(test)]
mod research_owner_tests {
    use super::*;
    use crate::inference_service::RetainedSourceOwner;

    #[test]
    #[ignore = "requires an exited R4 headed profile and an output receipt path"]
    fn independently_inspect_exited_research_profile() {
        let root = PathBuf::from(std::env::var_os("TURNSTONE_R4_PROFILE").expect("R4 profile"));
        let observations: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var_os("TURNSTONE_R4_OBSERVATION").expect("R4 observation"))
                .unwrap(),
        )
        .unwrap();
        let session = observations["session"].as_str().unwrap();
        let admissions: Vec<crate::saved_sources::Admission> =
            serde_json::from_value(observations["admissions"].clone()).unwrap();
        let mut reports = Vec::new();
        for entry in std::fs::read_dir(root.join("sessions")).unwrap() {
            let directory = entry.unwrap().path();
            if !directory.join("memory/version").is_file() {
                continue;
            }
            let mut store = FjallStore::open(directory.join("memory")).unwrap();
            let references = pollster::block_on(PageTextStore::new(&store).references()).unwrap();
            assert!(
                references.is_empty(),
                "fresh ordinary history retained a PageText body"
            );
            let mut report = serde_json::json!({"session":directory.file_name().unwrap().to_string_lossy(),"page_text_references":references.len()});
            if directory.file_name().unwrap() == session {
                let entries = saved_sources::load(&mut store, &admissions);
                assert_eq!(entries.len(), 3);
                assert!(
                    entries
                        .iter()
                        .all(|entry| entry.error.is_none() && entry.content.is_some())
                );
                report["validated_captures"] = serde_json::to_value(entries).unwrap();
            }
            reports.push(report);
        }
        assert!(
            reports
                .iter()
                .any(|report| report.get("validated_captures").is_some())
        );
        std::fs::write(std::env::var_os("TURNSTONE_R4_STORE_RECEIPT").expect("R4 store receipt"),
            serde_json::to_vec_pretty(&serde_json::json!({"result":"pass","independent_after_app_exit":true,"sessions":reports})).unwrap()).unwrap();
    }

    use crate::saved_sources::{Admission, Key, Target};
    #[test]
    fn research_owner_loads_only_live_captures_and_reopens_exact_versions_without_page_text_fallback()
     {
        let root = tempfile::tempdir().unwrap();
        let session = crate::panes::SessionId::new();
        let directory = root
            .path()
            .join(session.as_uuid().to_string())
            .join("memory");
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = FjallStore::open(&directory).unwrap();
        let make = |label: &str| {
            let body =
                format!("<html><main>{label}: naïve — 東京 🐦 saved answer evidence</main></html>");
            crate::content::FetchedDocument {
                bytes: body.as_bytes().to_vec(),
                body,
                content_type: Some("text/html".into()),
                effective_url: Some("https://gone.test/A".into()),
                acquired_at_ms: 123456,
            }
        };
        let a1 = capture_source_document_in(&mut store, &make("A1")).unwrap();
        let a2 = capture_source_document_in(&mut store, &make("A2")).unwrap();
        let orphan =
            capture_source_document_in(&mut store, &make("orphan foreign sentinel")).unwrap();
        drop(store);
        let node = uuid::Uuid::new_v4();
        let admitted = |manifest: String| Admission {
            key: Key {
                session,
                graph: crate::panes::GraphId::nil(),
                node,
                annotation_manifest: manifest,
            },
            occurrence: 0,
            title: "Source A".into(),
            url: "https://gone.test/A".into(),
            error: None,
        };
        let generation = uuid::Uuid::new_v4();
        let context = crate::research::Context {
            session,
            generation,
            admissions: vec![
                admitted(a1.annotation_manifest.clone()),
                admitted(a2.annotation_manifest.clone()),
            ],
        };
        let (actor, _) = spawn_trail_for_session(
            std::sync::Arc::new(|| {}),
            Some(directory.clone()),
            Default::default(),
            generation,
        );
        let actor = std::sync::Arc::new(actor);
        let bridge = crate::research::Bridge::default();
        bridge.project(context.clone());
        let owner = CaptureInferenceOwner {
            trail: actor.clone(),
            bridge: bridge.clone(),
        };
        let namespace = crate::research::namespace(session);
        let sources = owner.snapshot(&namespace).unwrap();
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|source| !source.text.contains("orphan")));
        let source_a1 = sources
            .iter()
            .find(|source| source.text.starts_with("A1"))
            .unwrap();
        assert!(
            owner
                .resolve(&source_a1.source, &source_a1.version)
                .unwrap()
                .starts_with("A1")
        );
        let a1_target = crate::research::target(&source_a1.source, &source_a1.version).unwrap();
        assert!(crate::research::scope(&namespace, &[a1_target.clone()], &sources).is_ok());
        assert!(crate::research::scope(&namespace, &[], &sources).is_err());
        let opened = crate::inference_service::OpenSource {
            run: format!("{namespace}/citation"),
            handle: 1,
            retained: source_a1.clone(),
            selector: eidetic::models::recall::PassageSelector {
                start: 0,
                end: source_a1.text.chars().count(),
            },
        };
        owner.open_source(&opened, Some(generation)).unwrap();
        let intents = bridge.drain();
        assert_eq!(
            intents.len(),
            1,
            "citation completion queues an App action without any surface frame"
        );
        match &intents[0] {
            crate::research::Intent::Citation {
                context: actual,
                target,
            } => {
                assert_eq!(actual, &context);
                assert_eq!(target.key, a1_target.key);
                assert!(target.selector.is_some());
            }
            _ => panic!("expected exact citation intent"),
        }
        assert!(
            owner
                .open_source(&opened, Some(uuid::Uuid::new_v4()))
                .is_err()
        );
        assert!(bridge.drain().is_empty());

        assert!(owner.snapshot("turnstone/wrong/research/v1").is_err());
        let mut false_source = source_a1.source.clone();
        false_source.owner = Some("foreign".into());
        assert!(owner.resolve(&false_source, &source_a1.version).is_err());
        let unadmitted = Target {
            key: Key {
                annotation_manifest: orphan.annotation_manifest,
                ..a1_target.key.clone()
            },
            ..a1_target.clone()
        };
        assert!(
            owner
                .resolve(
                    &crate::research::source_id(&unadmitted).unwrap(),
                    &unadmitted.text_hash
                )
                .is_err()
        );
        // Selection is ask intent only: old A1 remains resolvable alongside A2.
        assert!(owner.resolve(&source_a1.source, &source_a1.version).is_ok());
        bridge.project(crate::research::Context {
            admissions: vec![admitted(a2.annotation_manifest)],
            ..context.clone()
        });
        assert!(
            owner
                .resolve(&source_a1.source, &source_a1.version)
                .unwrap_err()
                .contains("withdrawn")
        );
        bridge.project(crate::research::Context {
            generation: uuid::Uuid::new_v4(),
            ..context
        });
        assert!(
            owner.snapshot(&namespace).is_err(),
            "an old opening cannot read the new actor context"
        );
        let (ack, receive) = std::sync::mpsc::sync_channel(1);
        actor.command(TrailCommand::Release(ack));
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(owner);
        std::sync::Arc::try_unwrap(actor).ok().unwrap().join();
        let mut reopened = FjallStore::open(&directory).unwrap();
        assert!(
            saved_sources::load(&mut reopened, &[admitted(a1.annotation_manifest)])[0]
                .content
                .as_ref()
                .unwrap()
                .text
                .starts_with("A1")
        );
    }
}
