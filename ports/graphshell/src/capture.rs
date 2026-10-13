// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Consented browser-history intake into Mere and Eidetic browsing memory.
//!
//! Browser APIs stay at the extension edge. This module owns the portable
//! event shape, configurable privacy policy, graph projection, durable batch
//! boundary, and forget operation.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use chartulary::FacetId;
use eidetic::browsing::{
    BrowsingMemory, PageRef, TraceEvent, TraceTransition, bootstrap_browsing_schema,
};
use eidetic::import::encounters::{
    BrowserEncounter, BrowserEncounterLedger, BrowserEncounterRef, BrowserParent,
    BrowserSourceScope,
};
use eidetic::{NoFetcher, PrivacyClass, list_typed, load_typed, manifest::delete_manifest};
use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
use mere::kernel::graph::{EdgeFamily, NavigationTrigger, RelationKind, RelationSelector};
use muniment::{Backend, StoreError, WriteOp};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::access::{
    ACCESS_HISTORY_FACET, AccessAction, AccessHistory, AccessObservation, AccessRecord,
    AccessTransition, bootstrap_access_record_schema, record_observation, save_access_record,
};
use crate::mere_host::{MereHost, MereHostError};
use crate::product::ProductError;

pub const BROWSER_HISTORY_FACET: &str = "graphshell.browser-history/v1";
pub const BROWSER_HISTORY_HANDLER_PREFIX: &str = "browser.history/";
const ENCOUNTER_KEY_PREFIX: &str = "graphshell/browser-encounters/v1/";

fn default_encounter_limit() -> usize {
    100_000
}

#[cfg(not(target_arch = "wasm32"))]
pub trait CaptureBackend: Backend + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Backend + Sync> CaptureBackend for T {}

#[cfg(target_arch = "wasm32")]
pub trait CaptureBackend: Backend {}
#[cfg(target_arch = "wasm32")]
impl<T: Backend> CaptureBackend for T {}

/// Buffered writes for one capture transaction, plus where each key's most
/// recent write sits in them.
///
/// The ordered vec is what commits; `latest` exists only so a read does not
/// rescan it. `forget_url` interleaves a read per access-record manifest with
/// a `Delete` push into the same growing buffer, and unlike browsing traces
/// those manifests have no retention pass, so a linear overlay scan made that
/// loop quadratic in a set that only ever grows.
#[derive(Default)]
struct StagedWrites {
    ops: Vec<WriteOp>,
    latest: HashMap<String, usize>,
}

impl StagedWrites {
    fn push(&mut self, op: WriteOp) {
        let key = match &op {
            WriteOp::Put { key, .. } | WriteOp::Delete { key } => key.clone(),
        };
        self.latest.insert(key, self.ops.len());
        self.ops.push(op);
    }
}

/// Read-through, write-buffer backend for one capture transaction.
struct CaptureBatch<'a, B> {
    base: &'a B,
    staged: Mutex<StagedWrites>,
}

impl<'a, B: Backend> CaptureBatch<'a, B> {
    fn new(base: &'a B) -> Self {
        Self {
            base,
            staged: Mutex::new(StagedWrites::default()),
        }
    }

    /// Drain the buffer into the base backend. One transaction commits once
    /// and both callers drop the batch afterwards, so taking the buffer under
    /// the lock is cheaper than copying it and leaves no stale overlay behind.
    async fn commit(&self) -> Result<(), StoreError> {
        let staged = std::mem::take(&mut *self.staged.lock().unwrap());
        self.base.apply(&staged.ops).await
    }

    fn overlay(&self, key: &str) -> Option<Option<Vec<u8>>> {
        let staged = self.staged.lock().unwrap();
        match &staged.ops[*staged.latest.get(key)?] {
            WriteOp::Put { value, .. } => Some(Some(value.clone())),
            WriteOp::Delete { .. } => Some(None),
        }
    }

    fn overlay_keys(
        &self,
        mut keys: BTreeSet<String>,
        accepts: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        // Only a key's last write decides whether it is present, so the index
        // gives the same answer as replaying the log. A key `accepts` rejects
        // is one the base listing never returned either, so its `Delete` was
        // already a no-op on this set.
        let staged = self.staged.lock().unwrap();
        for (key, index) in &staged.latest {
            match &staged.ops[*index] {
                WriteOp::Put { .. } if accepts(key) => {
                    keys.insert(key.clone());
                },
                WriteOp::Delete { .. } => {
                    keys.remove(key);
                },
                _ => {},
            }
        }
        keys.into_iter().collect()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<'a, B: CaptureBackend> Backend for CaptureBatch<'a, B> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self.overlay(key) {
            Some(value) => Ok(value),
            None => self.base.get(key).await,
        }
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.staged.lock().unwrap().push(WriteOp::Put {
            key: key.to_string(),
            value: bytes.to_vec(),
        });
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.staged.lock().unwrap().push(WriteOp::Delete {
            key: key.to_string(),
        });
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let keys = self.base.list(prefix).await?.into_iter().collect();
        Ok(self.overlay_keys(keys, |key| key.starts_with(prefix)))
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        let keys = self.base.scan(start, end).await?.into_iter().collect();
        Ok(self.overlay_keys(keys, |key| key >= start && key < end))
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        let mut staged = self.staged.lock().unwrap();
        for op in ops {
            staged.push(op.clone());
        }
        Ok(())
    }
}

/// User-owned browser capture settings. Capture begins disabled; consent is an
/// explicit state transition performed by the host UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCapturePolicy {
    pub enabled: bool,
    pub accepted_schemes: BTreeSet<String>,
    pub excluded_origins: BTreeSet<String>,
    pub strip_query: bool,
    pub strip_fragment: bool,
    pub dedupe_window_ms: u64,
    pub segment_size: usize,
    pub retention_traces: usize,
    #[serde(default = "default_encounter_limit")]
    pub encounter_limit: usize,
}

impl HistoryCapturePolicy {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            accepted_schemes: ["http".to_string(), "https".to_string()]
                .into_iter()
                .collect(),
            excluded_origins: BTreeSet::new(),
            strip_query: false,
            strip_fragment: true,
            dedupe_window_ms: 1_000,
            segment_size: 64,
            retention_traces: 2_048,
            encounter_limit: default_encounter_limit(),
        }
    }

    /// The conservative recommended policy after the user grants browser
    /// history access. Every field remains editable by the host.
    pub fn consented() -> Self {
        Self {
            enabled: true,
            ..Self::disabled()
        }
    }
}

/// One browser visit at the extension boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserVisit {
    pub source: String,
    pub visit_id: Option<String>,
    #[serde(default)]
    pub source_scope: Option<BrowserSourceScope>,
    #[serde(default)]
    pub parent: BrowserParent,
    #[serde(default)]
    pub original_unix_micros: Option<i64>,
    #[serde(default)]
    pub source_is_local: Option<bool>,
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub referrer_url: Option<String>,
    pub transition: String,
    pub at_ms: u64,
    pub private: bool,
}

impl BrowserVisit {
    /// File exports and extension observations join the same admission path.
    pub fn from_imported(item: &eidetic::import::ImportedHistoryVisitItem, source: &str) -> Self {
        use eidetic::import::HistoryTransitionKind as T;
        let transition = match item.transition.as_ref() {
            Some(T::Link) => "link",
            Some(T::Typed) => "typed",
            Some(T::Generated) => "generated",
            Some(T::AutoBookmark) => "auto_bookmark",
            Some(T::AutoSubframe) => "auto_subframe",
            Some(T::Reload) => "reload",
            Some(T::Redirect) => "redirect",
            Some(T::Other(raw)) => raw.as_str(),
            None => "unknown",
        };
        Self {
            source: source.into(),
            source_scope: item.source_scope.clone(),
            visit_id: item.visit_id.clone(),
            parent: item.parent.clone(),
            original_unix_micros: item.visited_at_unix_micros,
            source_is_local: None,
            url: item.page.canonical_url.clone(),
            title: item
                .page
                .normalized_title
                .clone()
                .or_else(|| item.page.raw_title.clone()),
            favicon_url: item.page.favicon_url.clone(),
            referrer_url: item.referring_url.clone(),
            transition: transition.into(),
            at_ms: eidetic::import::history_event(item).at_ms,
            private: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedVisit {
    pub source: String,
    pub visit_id: Option<String>,
    pub source_scope: Option<BrowserSourceScope>,
    pub parent: BrowserParent,
    pub raw_transition: String,
    pub original_unix_micros: Option<i64>,
    pub source_is_local: Option<bool>,
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub referrer_url: Option<String>,
    pub transition: CaptureTransition,
    pub at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTransition {
    LinkClick,
    UrlTyped,
    Back,
    Forward,
    Reload,
    Redirect,
    TabSpawn,
    Restore,
    Imported,
    Unknown,
}

impl CaptureTransition {
    fn from_browser(value: &str) -> Self {
        use chartulary::stemma::TransitionKind as T;
        match eidetic::import::encounters::browser_transition(value) {
            T::LinkClick => Self::LinkClick,
            T::UrlTyped => Self::UrlTyped,
            T::Back => Self::Back,
            T::Forward => Self::Forward,
            T::Reload => Self::Reload,
            T::Redirect => Self::Redirect,
            T::TabSpawn => Self::TabSpawn,
            T::Restore => Self::Restore,
            T::Imported => Self::Imported,
            T::Unknown => Self::Unknown,
        }
    }

    fn trace(self) -> TraceTransition {
        match self {
            Self::LinkClick => TraceTransition::LinkClick,
            Self::UrlTyped => TraceTransition::UrlTyped,
            Self::Back => TraceTransition::Back,
            Self::Forward => TraceTransition::Forward,
            Self::Reload => TraceTransition::Reload,
            Self::Redirect => TraceTransition::Redirect,
            Self::TabSpawn => TraceTransition::TabSpawn,
            Self::Restore => TraceTransition::Restore,
            Self::Imported => TraceTransition::Imported,
            Self::Unknown => TraceTransition::Unknown,
        }
    }

    fn graph(self) -> NavigationTrigger {
        match self {
            Self::LinkClick => NavigationTrigger::LinkClick,
            Self::UrlTyped => NavigationTrigger::AddressBarEntry,
            Self::Back => NavigationTrigger::Back,
            Self::Forward => NavigationTrigger::Forward,
            Self::Reload => NavigationTrigger::Programmatic,
            Self::Redirect => NavigationTrigger::Redirect,
            Self::TabSpawn => NavigationTrigger::PanePromotion,
            Self::Restore => NavigationTrigger::ReopenSession,
            Self::Imported => NavigationTrigger::ImportedHistory,
            Self::Unknown => NavigationTrigger::Unknown,
        }
    }

    fn access(self) -> AccessTransition {
        match self {
            Self::LinkClick => AccessTransition::LinkClick,
            Self::UrlTyped => AccessTransition::UrlTyped,
            Self::Back => AccessTransition::Back,
            Self::Forward => AccessTransition::Forward,
            Self::Reload => AccessTransition::Reload,
            Self::Redirect => AccessTransition::Redirect,
            Self::TabSpawn => AccessTransition::TabSpawn,
            Self::Restore => AccessTransition::Restore,
            Self::Imported => AccessTransition::Imported,
            Self::Unknown => AccessTransition::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDropReason {
    Disabled,
    PrivateWindow,
    InvalidAddress,
    UnsupportedScheme,
    ExcludedOrigin,
    Duplicate,
    InvalidIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    Accepted { node: Uuid },
    Buffered { identity: BrowserEncounterRef },
    Dropped(CaptureDropReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgetMode {
    /// Remove captured history while preserving the addressed object.
    HistoryOnly,
    /// Also remove the object when browser capture originally created it.
    RemoveCapturedObject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserHistoryFacetV1 {
    pub created_by_capture: bool,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    pub visit_count: u64,
    pub latest_title: Option<String>,
    pub latest_favicon_url: Option<String>,
    pub last_transition: CaptureTransition,
    pub sources: BTreeSet<String>,
    #[serde(default)]
    pub encounters: Vec<BrowserEncounterRef>,
    #[serde(default)]
    pub relations: Vec<BrowserDocumentRelation>,
}

/// A document relation projection with exact source encounter handles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserDocumentRelation {
    pub kind: BrowserRelationKind,
    pub parent: BrowserEncounterRef,
    pub child: BrowserEncounterRef,
    pub from_url: String,
    pub to_url: String,
    pub raw_transition: String,
    pub at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRelationKind {
    BrowserReferrer,
    FollowedLink,
    Redirect,
    Reload,
}

impl<B: Backend> MereHost<B> {
    /// Inspect exact encounter evidence for a directed document pair. The
    /// facet is a projection; the source encounters remain in the ledger.
    pub fn browser_relations(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<BrowserDocumentRelation>, CaptureError> {
        let Some(value) = self.facet_value(to, BROWSER_HISTORY_FACET) else {
            return Ok(Vec::new());
        };
        let facet: BrowserHistoryFacetV1 = serde_json::from_value(value.clone())
            .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?;
        Ok(facet
            .relations
            .into_iter()
            .filter(|relation| relation.from_url == from && relation.to_url == to)
            .collect())
    }
}

fn encounter_key<B: Backend>(host: &MereHost<B>) -> String {
    format!(
        "{ENCOUNTER_KEY_PREFIX}{}",
        host.graph_session().manifest().root_graph_id.as_uuid()
    )
}

/// Read the same durable visit authority used by admission, including unresolved
/// source children. No fetcher, page request or live navigation is involved.
pub async fn browser_encounters<B: Backend>(
    host: &MereHost<B>,
    store: &B,
) -> Result<BrowserEncounterLedger, CaptureError> {
    let bytes = store
        .get(&encounter_key(host))
        .await
        .map_err(|error| CaptureError::Store(error.to_string()))?;
    let ledger: BrowserEncounterLedger = bytes
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| CaptureError::InvalidFacet(error.to_string()))
        })
        .transpose()?
        .unwrap_or_default();
    ledger
        .validate()
        .map_err(|error| CaptureError::InvalidFacet(error.into()))?;
    Ok(ledger)
}

fn source_key(source: &str, scope: Option<&BrowserSourceScope>) -> String {
    scope.map_or_else(
        || source.to_string(),
        |scope| serde_json::to_string(scope).expect("browser scope serializes"),
    )
}

impl NormalizedVisit {
    fn trace(&self) -> TraceTransition {
        match self.raw_transition.trim().to_ascii_lowercase().as_str() {
            "auto_subframe" | "manual_subframe" | "download" => TraceTransition::Restore,
            _ => self.transition.trace(),
        }
    }
    fn from_encounter(visit: &BrowserEncounter, parent: Option<&BrowserEncounter>) -> Self {
        Self {
            source: visit.identity.scope.product.clone(),
            visit_id: Some(visit.identity.visit_id.clone()),
            source_scope: Some(visit.identity.scope.clone()),
            parent: visit.parent.clone(),
            raw_transition: visit.raw_transition.clone(),
            original_unix_micros: visit.original_unix_micros,
            source_is_local: visit.source_is_local,
            url: visit.url.clone(),
            title: visit.title.clone(),
            favicon_url: None,
            referrer_url: parent.map(|parent| parent.url.clone()),
            transition: CaptureTransition::from_browser(&visit.raw_transition),
            at_ms: visit.at_ms,
        }
    }
}

#[derive(Debug)]
pub enum CaptureError {
    Store(String),
    Host(MereHostError),
    Product(ProductError),
    RejectedAddress(CaptureDropReason),
    InvalidFacet(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "browser memory: {error}"),
            Self::Host(error) => write!(formatter, "{error}"),
            Self::Product(error) => write!(formatter, "{error}"),
            Self::RejectedAddress(reason) => {
                write!(
                    formatter,
                    "browser history address was rejected: {reason:?}"
                )
            },
            Self::InvalidFacet(error) => write!(formatter, "browser history facet: {error}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<MereHostError> for CaptureError {
    fn from(value: MereHostError) -> Self {
        Self::Host(value)
    }
}

impl From<ProductError> for CaptureError {
    fn from(value: ProductError) -> Self {
        Self::Product(value)
    }
}

/// Capture state loaded from the same Muniment backend used by the local Mere
/// host. The Eidetic payloads and graph document occupy distinct key spaces.
pub struct BrowserHistoryCapture {
    policy: HistoryCapturePolicy,
    memory: BrowsingMemory,
    seen_visit_ids: HashSet<(String, String)>,
    last_accepted: Option<(String, u64)>,
}

impl BrowserHistoryCapture {
    pub async fn load<B: Backend>(
        store: &mut B,
        policy: HistoryCapturePolicy,
    ) -> Result<Self, CaptureError> {
        bootstrap_browsing_schema(store)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?;
        bootstrap_access_record_schema(store)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?;
        let memory = BrowsingMemory::load(store, policy.segment_size)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?;
        let mut seen_visit_ids = HashSet::new();
        let mut fetcher = NoFetcher;
        for manifest in list_typed::<AccessRecord>(store)
            .await
            .map_err(|error| CaptureError::Store(error.to_string()))?
        {
            if let Some(record) = load_typed::<AccessRecord>(store, &mut fetcher, manifest.id)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?
                && let Some(source_event_id) = record.source_event_id
            {
                seen_visit_ids.insert((record.capture_source, source_event_id));
            }
        }
        Ok(Self {
            policy,
            memory,
            seen_visit_ids,
            last_accepted: None,
        })
    }

    pub fn policy(&self) -> &HistoryCapturePolicy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: HistoryCapturePolicy) {
        self.policy = policy;
    }

    pub fn memory(&self) -> &BrowsingMemory {
        &self.memory
    }

    /// Project one service-worker delivery as one durable batch. Every
    /// accepted event is flushed before return, so worker suspension cannot
    /// strand an in-memory partial segment.
    pub async fn ingest_batch<B: CaptureBackend>(
        &mut self,
        host: &mut MereHost<B>,
        store: &mut B,
        visits: impl IntoIterator<Item = BrowserVisit>,
        persona: &str,
        device: &str,
        saved_at_secs: u64,
    ) -> Result<Vec<CaptureOutcome>, CaptureError> {
        let memory_before = self.memory.clone();
        let mut ledger = browser_encounters(host, store).await?;
        let mut batch = CaptureBatch::new(&*store);
        let staged = async {
            let mut outcomes = Vec::new();
            let mut pending_seen = HashSet::new();
            let mut pending_last = self.last_accepted.clone();
            let mut scoped_outcomes = Vec::new();
            for visit in visits {
                let normalized =
                    match self.normalize_against(visit, &pending_seen, pending_last.as_ref()) {
                        Ok(visit) => visit,
                        Err(reason) => {
                            outcomes.push(CaptureOutcome::Dropped(reason));
                            continue;
                        },
                    };
                if let (Some(scope), Some(visit_id)) =
                    (&normalized.source_scope, &normalized.visit_id)
                {
                    let encounter = BrowserEncounter {
                        identity: BrowserEncounterRef {
                            scope: scope.clone(),
                            visit_id: visit_id.clone(),
                        },
                        parent: normalized.parent.clone(),
                        url: normalized.url.clone(),
                        title: normalized.title.clone(),
                        raw_transition: normalized.raw_transition.clone(),
                        at_ms: normalized.at_ms,
                        original_unix_micros: normalized.original_unix_micros,
                        source_is_local: normalized.source_is_local,
                        persona: persona.to_string(),
                        acquisition_device: device.to_string(),
                        intake_at_ms: saved_at_secs.saturating_mul(1_000),
                    };
                    let identity = encounter.identity.clone();
                    let received = ledger
                        .receive(encounter, self.policy.encounter_limit)
                        .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?;
                    if !received {
                        outcomes.push(CaptureOutcome::Dropped(CaptureDropReason::Duplicate));
                        continue;
                    }
                    scoped_outcomes.push((identity.clone(), outcomes.len()));
                    outcomes.push(CaptureOutcome::Buffered { identity });
                    continue;
                }
                let projected = host.project_browser_visit(&normalized, persona, device, None)?;
                save_access_record(&mut batch, &projected.record)
                    .await
                    .map_err(|error| CaptureError::Store(error.to_string()))?;
                self.memory.record_traversal(
                    persona,
                    TraceEvent {
                        from: normalized.referrer_url.as_ref().map(|url| PageRef {
                            url: url.clone(),
                            title: None,
                        }),
                        to: PageRef {
                            url: normalized.url.clone(),
                            title: normalized.title.clone(),
                        },
                        transition: normalized.trace(),
                        at_ms: normalized.at_ms,
                        dwell_ms: None,
                        candidates: Vec::new(),
                    },
                );
                if let Some(visit_id) = &normalized.visit_id {
                    pending_seen.insert((normalized.source.clone(), visit_id.clone()));
                }
                pending_last = Some((normalized.url.clone(), normalized.at_ms));
                outcomes.push(CaptureOutcome::Accepted {
                    node: projected.node,
                });
            }
            for encounter in ledger.resolve() {
                let parent = match &encounter.parent {
                    BrowserParent::Visit { visit_id }
                    | BrowserParent::OutsideRange { visit_id } => {
                        ledger.encounter(&BrowserEncounterRef {
                            scope: encounter.identity.scope.clone(),
                            visit_id: visit_id.clone(),
                        })
                    },
                    _ => None,
                };
                let normalized = NormalizedVisit::from_encounter(&encounter, parent);
                let relation = parent.map(|parent| BrowserDocumentRelation {
                    kind: match normalized.transition {
                        CaptureTransition::LinkClick => BrowserRelationKind::FollowedLink,
                        CaptureTransition::Redirect => BrowserRelationKind::Redirect,
                        CaptureTransition::Reload => BrowserRelationKind::Reload,
                        _ => BrowserRelationKind::BrowserReferrer,
                    },
                    parent: parent.identity.clone(),
                    child: encounter.identity.clone(),
                    from_url: parent.url.clone(),
                    to_url: encounter.url.clone(),
                    raw_transition: encounter.raw_transition.clone(),
                    at_ms: encounter.at_ms,
                });
                let projected = host.project_browser_visit(
                    &normalized,
                    &encounter.persona,
                    &encounter.acquisition_device,
                    relation.as_ref(),
                )?;
                if let Some((_, index)) = scoped_outcomes
                    .iter()
                    .find(|(id, _)| *id == encounter.identity)
                {
                    outcomes[*index] = CaptureOutcome::Accepted {
                        node: projected.node,
                    };
                }
                save_access_record(&mut batch, &projected.record)
                    .await
                    .map_err(|error| CaptureError::Store(error.to_string()))?;
                self.memory.record_traversal(
                    &encounter.persona,
                    TraceEvent {
                        from: normalized
                            .referrer_url
                            .clone()
                            .map(|url| PageRef { url, title: None }),
                        to: PageRef {
                            url: encounter.url,
                            title: encounter.title,
                        },
                        transition: normalized.trace(),
                        at_ms: encounter.at_ms,
                        dwell_ms: None,
                        candidates: Vec::new(),
                    },
                );
            }
            let ledger_bytes = serde_json::to_vec(&ledger)
                .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?;
            batch
                .put(&encounter_key(host), &ledger_bytes)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            self.memory
                .flush(&mut batch, saved_at_secs.saturating_mul(1_000))
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            self.memory
                .apply_quota(&mut batch, self.policy.retention_traces)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            let written = host.stage(&batch, saved_at_secs).await?;
            Ok::<_, CaptureError>((outcomes, pending_seen, pending_last, written))
        }
        .await;
        let (outcomes, pending_seen, pending_last, written) = match staged {
            Ok(staged) => staged,
            Err(error) => {
                self.memory = memory_before;
                return Err(error);
            },
        };
        if let Err(error) = batch.commit().await {
            self.memory = memory_before;
            return Err(CaptureError::Store(error.to_string()));
        }
        host.staged(written);
        self.seen_visit_ids.extend(pending_seen);
        self.last_accepted = pending_last;
        Ok(outcomes)
    }

    pub async fn forget_url<B: CaptureBackend>(
        &mut self,
        host: &mut MereHost<B>,
        store: &mut B,
        url: &str,
        mode: ForgetMode,
        saved_at_secs: u64,
    ) -> Result<usize, CaptureError> {
        // A newly excluded origin must remain forgettable. Exclusions govern
        // intake, not the user's ability to erase records already stored.
        let mut forget_policy = self.policy.clone();
        forget_policy.excluded_origins.clear();
        let canonical =
            canonical_url(url, &forget_policy).map_err(CaptureError::RejectedAddress)?;
        let memory_before = self.memory.clone();
        let mut ledger = browser_encounters(host, store).await?;
        ledger.forget_url(&canonical);
        let mut batch = CaptureBatch::new(&*store);
        let staged = async {
            let forgotten = self
                .memory
                .forget_url(&mut batch, &canonical)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            let mut forgotten_source_events = HashSet::new();
            let mut fetcher = NoFetcher;
            for manifest in list_typed::<AccessRecord>(&mut batch)
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?
            {
                let Some(record) =
                    load_typed::<AccessRecord>(&mut batch, &mut fetcher, manifest.id)
                        .await
                        .map_err(|error| CaptureError::Store(error.to_string()))?
                else {
                    continue;
                };
                if record.address != canonical
                    && record.referring_address.as_deref() != Some(canonical.as_str())
                {
                    continue;
                }
                if let Some(source_event_id) = record.source_event_id {
                    forgotten_source_events.insert((record.capture_source, source_event_id));
                }
                delete_manifest(&mut batch, manifest.id)
                    .await
                    .map_err(|error| CaptureError::Store(error.to_string()))?;
            }
            host.forget_browser_history(&canonical, mode)?;
            host.forget_related_browser_evidence(&canonical)?;
            batch
                .put(
                    &encounter_key(host),
                    &serde_json::to_vec(&ledger)
                        .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?,
                )
                .await
                .map_err(|error| CaptureError::Store(error.to_string()))?;
            let written = host.stage(&batch, saved_at_secs).await?;
            Ok::<_, CaptureError>((forgotten, forgotten_source_events, written))
        }
        .await;
        let (forgotten, forgotten_source_events, written) = match staged {
            Ok(staged) => staged,
            Err(error) => {
                self.memory = memory_before;
                return Err(error);
            },
        };
        if let Err(error) = batch.commit().await {
            self.memory = memory_before;
            return Err(CaptureError::Store(error.to_string()));
        }
        host.staged(written);
        self.seen_visit_ids
            .retain(|key| !forgotten_source_events.contains(key));
        Ok(forgotten)
    }

    #[cfg(test)]
    fn normalize(&self, visit: BrowserVisit) -> Result<NormalizedVisit, CaptureDropReason> {
        self.normalize_against(visit, &HashSet::new(), self.last_accepted.as_ref())
    }

    fn normalize_against(
        &self,
        visit: BrowserVisit,
        pending_seen: &HashSet<(String, String)>,
        pending_last: Option<&(String, u64)>,
    ) -> Result<NormalizedVisit, CaptureDropReason> {
        if !self.policy.enabled {
            return Err(CaptureDropReason::Disabled);
        }
        if visit.private {
            return Err(CaptureDropReason::PrivateWindow);
        }
        let source = visit.source.trim().to_ascii_lowercase();
        if visit
            .source_scope
            .as_ref()
            .is_some_and(|scope| !scope.is_valid() || !scope.product.eq_ignore_ascii_case(&source))
            || (visit.source_scope.is_some()
                && visit
                    .visit_id
                    .as_deref()
                    .is_none_or(|id| id.trim().is_empty()))
        {
            return Err(CaptureDropReason::InvalidIdentity);
        }
        let capture_source = source_key(&source, visit.source_scope.as_ref());
        if visit.visit_id.as_ref().is_some_and(|visit_id| {
            // Scoped encounters deduplicate and check source conflicts in the
            // ledger. Legacy records keep their existing supported read path.
            visit.source_scope.is_none()
                && (self
                    .seen_visit_ids
                    .contains(&(capture_source.clone(), visit_id.clone()))
                    || pending_seen.contains(&(capture_source.clone(), visit_id.clone())))
        }) {
            return Err(CaptureDropReason::Duplicate);
        }
        let url = canonical_url(&visit.url, &self.policy)?;
        if visit.visit_id.is_none()
            && pending_last.is_some_and(|(last_url, at_ms)| {
                last_url == &url && visit.at_ms.abs_diff(*at_ms) <= self.policy.dedupe_window_ms
            })
        {
            return Err(CaptureDropReason::Duplicate);
        }
        let referrer_url = visit
            .referrer_url
            .as_deref()
            .and_then(|url| canonical_url(url, &self.policy).ok());
        Ok(NormalizedVisit {
            source,
            visit_id: visit.visit_id,
            source_scope: visit.source_scope,
            parent: if visit.referrer_url.is_some() && referrer_url.is_none() {
                BrowserParent::Withheld
            } else {
                visit.parent
            },
            raw_transition: visit.transition.clone(),
            original_unix_micros: visit.original_unix_micros,
            source_is_local: visit.source_is_local,
            url,
            title: clean_optional(visit.title),
            favicon_url: clean_optional(visit.favicon_url),
            referrer_url,
            transition: CaptureTransition::from_browser(&visit.transition),
            at_ms: visit.at_ms,
        })
    }
}

fn canonical_url(
    address: &str,
    policy: &HistoryCapturePolicy,
) -> Result<String, CaptureDropReason> {
    let mut url = Url::parse(address).map_err(|_| CaptureDropReason::InvalidAddress)?;
    if !policy.accepted_schemes.contains(url.scheme()) {
        return Err(CaptureDropReason::UnsupportedScheme);
    }
    let origin = url.origin().ascii_serialization();
    if policy.excluded_origins.contains(&origin) {
        return Err(CaptureDropReason::ExcludedOrigin);
    }
    if policy.strip_query {
        url.set_query(None);
    }
    if policy.strip_fragment {
        url.set_fragment(None);
    }
    Ok(url.into())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl<B: Backend> MereHost<B> {
    fn forget_related_browser_evidence(&mut self, address: &str) -> Result<(), CaptureError> {
        let nodes: Vec<_> = self
            .graph()
            .nodes()
            .map(|(key, node)| (key, node.id))
            .collect();
        for (key, id) in nodes {
            if let Some(value) = self
                .graph()
                .facets()
                .get(&id, &FacetId::new(BROWSER_HISTORY_FACET))
                .cloned()
            {
                let mut facet: BrowserHistoryFacetV1 = serde_json::from_value(value)
                    .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?;
                let before = facet.relations.len();
                facet
                    .relations
                    .retain(|relation| relation.from_url != address && relation.to_url != address);
                if facet.relations.len() != before {
                    self.set_facet(
                        key,
                        BROWSER_HISTORY_FACET,
                        serde_json::to_value(facet).unwrap(),
                    )?;
                }
            }
            if let Some(value) = self
                .graph()
                .facets()
                .get(&id, &FacetId::new(ACCESS_HISTORY_FACET))
                .cloned()
            {
                let mut history: AccessHistory = serde_json::from_value(value)
                    .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?;
                let before = history.records.len();
                history.records.retain(|record| {
                    !(record.handler.starts_with(BROWSER_HISTORY_HANDLER_PREFIX)
                        && record.referring_address.as_deref() == Some(address))
                });
                if history.records.len() != before {
                    self.set_facet(
                        key,
                        ACCESS_HISTORY_FACET,
                        serde_json::to_value(history).unwrap(),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Project one visit, its edits authored via the extension that captured
    /// it (reservoir plan §7 item 23).
    fn project_browser_visit(
        &mut self,
        visit: &NormalizedVisit,
        persona: &str,
        device: &str,
        relation: Option<&BrowserDocumentRelation>,
    ) -> Result<ProjectedVisit, CaptureError> {
        self.through(format!("browser.extension.{}", visit.source), |host| {
            host.project_visit(visit, persona, device, relation)
        })
    }

    fn project_visit(
        &mut self,
        visit: &NormalizedVisit,
        persona: &str,
        device: &str,
        relation: Option<&BrowserDocumentRelation>,
    ) -> Result<ProjectedVisit, CaptureError> {
        let created_by_capture = self.graph().get_node_by_url(&visit.url).is_none();
        let node = self.create_address(&visit.url, visit.title.as_deref().unwrap_or_default())?;
        let key = self
            .graph()
            .get_node_key_by_id(node)
            .ok_or_else(|| CaptureError::InvalidFacet("created node is missing".to_string()))?;
        let referring_container_id = if let Some(referrer) = &visit.referrer_url
            && (relation.is_some() || referrer != &visit.url)
        {
            Some(self.create_address(referrer, "")?)
        } else {
            None
        };
        let source_identity = format!(
            "{}\u{0}{}\u{0}{}\u{0}{}",
            source_key(&visit.source, visit.source_scope.as_ref()),
            visit.visit_id.as_deref().unwrap_or_default(),
            visit.url,
            visit.at_ms
        );
        let observation = AccessObservation {
            record_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, source_identity.as_bytes()),
            action: if visit.transition == CaptureTransition::Imported {
                AccessAction::Import
            } else {
                AccessAction::Open
            },
            persona: persona.to_string(),
            device: device.to_string(),
            application: format!("browser.extension.{}", visit.source),
            handler: format!("{BROWSER_HISTORY_HANDLER_PREFIX}{}", visit.source),
            at_ms: visit.at_ms,
            dwell_ms: None,
            referring_container_id,
            referring_address: visit.referrer_url.clone(),
            transition: visit.transition.access(),
            capture_source: source_key(&visit.source, visit.source_scope.as_ref()),
            source_event_id: visit.visit_id.clone(),
            privacy: PrivacyClass::LocalOnly,
        };
        let (record, inserted) = self
            .mutate_product_graph(|graph| record_observation(graph, key, &observation))?
            .map_err(MereHostError::from)?;
        if !inserted {
            return Ok(ProjectedVisit { node, record });
        }

        let mut facet = self
            .facet_value(&visit.url, BROWSER_HISTORY_FACET)
            .map(|value| {
                serde_json::from_value::<BrowserHistoryFacetV1>(value.clone())
                    .map_err(|error| CaptureError::InvalidFacet(error.to_string()))
            })
            .transpose()?
            .unwrap_or_else(|| BrowserHistoryFacetV1 {
                created_by_capture,
                first_seen_ms: visit.at_ms,
                last_seen_ms: visit.at_ms,
                visit_count: 0,
                latest_title: None,
                latest_favicon_url: None,
                last_transition: visit.transition,
                sources: BTreeSet::new(),
                encounters: Vec::new(),
                relations: Vec::new(),
            });
        facet.first_seen_ms = facet.first_seen_ms.min(visit.at_ms);
        facet.last_seen_ms = facet.last_seen_ms.max(visit.at_ms);
        facet.visit_count = facet.visit_count.saturating_add(1);
        if visit.title.is_some() {
            facet.latest_title = visit.title.clone();
        }
        if visit.favicon_url.is_some() {
            facet.latest_favicon_url = visit.favicon_url.clone();
        }
        facet.last_transition = visit.transition;
        facet.sources.insert(visit.source.clone());
        if let (Some(scope), Some(visit_id)) = (&visit.source_scope, &visit.visit_id) {
            facet.encounters.push(BrowserEncounterRef {
                scope: scope.clone(),
                visit_id: visit_id.clone(),
            });
        }
        if let Some(relation) = relation {
            facet.relations.push(relation.clone());
        }
        self.set_facet(
            key,
            BROWSER_HISTORY_FACET,
            serde_json::to_value(facet)
                .map_err(|error| CaptureError::InvalidFacet(error.to_string()))?,
        )?;

        if let Some(from) = referring_container_id {
            let from_key = self.graph().get_node_key_by_id(from).ok_or_else(|| {
                CaptureError::InvalidFacet("referrer node is missing".to_string())
            })?;
            self.mutate_product_graph(|graph| {
                apply_graph_delta(
                    graph,
                    GraphDelta::AppendTraversal {
                        from: from_key,
                        to: key,
                        trigger: visit.transition.graph(),
                        timestamp_ms: Some(visit.at_ms),
                    },
                );
            })?;
        }
        Ok(ProjectedVisit { node, record })
    }

    fn forget_browser_history(
        &mut self,
        address: &str,
        mode: ForgetMode,
    ) -> Result<(), CaptureError> {
        let Some((key, node)) = self.graph().get_node_by_url(address) else {
            return Ok(());
        };
        let node_id = node.id;
        let capture_created = self
            .facet_value(address, BROWSER_HISTORY_FACET)
            .and_then(|value| serde_json::from_value::<BrowserHistoryFacetV1>(value.clone()).ok())
            .is_some_and(|facet| facet.created_by_capture);
        if mode == ForgetMode::RemoveCapturedObject && capture_created {
            self.mutate_product_graph(|graph| {
                apply_graph_delta(graph, GraphDelta::RemoveNode { key });
            })?;
            return Ok(());
        }

        let traversal_pairs = self
            .graph()
            .relations()
            .filter(|relation| {
                relation.kind == RelationKind::Traversal
                    && (relation.from == key || relation.to == key)
            })
            .map(|relation| (relation.from, relation.to))
            .collect::<BTreeSet<_>>();
        self.mutate_product_graph(|graph| {
            if let Some(value) = graph
                .facets()
                .get(&node_id, &FacetId::new(ACCESS_HISTORY_FACET))
                .cloned()
                && let Ok(mut history) = serde_json::from_value::<AccessHistory>(value)
            {
                history
                    .records
                    .retain(|record| !record.handler.starts_with(BROWSER_HISTORY_HANDLER_PREFIX));
                if history.records.is_empty() {
                    apply_graph_delta(
                        graph,
                        GraphDelta::RemoveNodeFacet {
                            key,
                            facet: ACCESS_HISTORY_FACET.to_string(),
                        },
                    );
                } else {
                    apply_graph_delta(
                        graph,
                        GraphDelta::SetNodeFacet {
                            key,
                            facet: ACCESS_HISTORY_FACET.to_string(),
                            value: serde_json::to_value(history)
                                .expect("AccessHistory always serializes"),
                        },
                    );
                }
            }
            apply_graph_delta(
                graph,
                GraphDelta::RemoveNodeFacet {
                    key,
                    facet: BROWSER_HISTORY_FACET.to_string(),
                },
            );
            for (from, to) in traversal_pairs {
                apply_graph_delta(
                    graph,
                    GraphDelta::RetractRelations {
                        from,
                        to,
                        selector: RelationSelector::Family(EdgeFamily::Traversal),
                    },
                );
            }
        })?;
        Ok(())
    }
}

struct ProjectedVisit {
    node: Uuid,
    record: AccessRecord,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::access::AccessContext;
    use crate::access::{AccessRecordFilter, query_access_records};
    use mere::kernel::graph::Author;
    use muniment::MemoryBackend;

    use super::*;
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef};

    const PERSONA: &str = "personae://persona/capture";
    const DEVICE: &str = "personae://device/browser";

    fn selected() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-capture".to_string(),
        }
    }

    fn visit(id: &str, url: &str, referrer_url: Option<&str>, at_ms: u64) -> BrowserVisit {
        BrowserVisit {
            source: "Firefox".to_string(),
            visit_id: Some(id.to_string()),
            source_scope: None,
            parent: BrowserParent::Unknown,
            original_unix_micros: None,
            source_is_local: None,
            url: url.to_string(),
            title: Some(format!("  {id} title  ")),
            favicon_url: Some("https://assets.example/favicon.ico".to_string()),
            referrer_url: referrer_url.map(str::to_string),
            transition: "link".to_string(),
            at_ms,
            private: false,
        }
    }

    fn scoped(mut visit: BrowserVisit, profile: &str, parent: BrowserParent) -> BrowserVisit {
        visit.source_scope = Some(BrowserSourceScope {
            product: "firefox".into(),
            profile: profile.into(),
            generation: "history-1".into(),
        });
        visit.parent = parent;
        visit
    }

    #[test]
    #[ignore = "explicit marker-owned disk fixture; run seed and reopen in separate processes"]
    fn disk_browser_history_receipt() {
        pollster::block_on(async {
            let root = std::path::PathBuf::from(
                std::env::var("MERE_BROWSER_HISTORY_RECEIPT_ROOT").expect("explicit fixture root"),
            );
            assert_eq!(
                std::fs::read_to_string(root.join(".owner")).unwrap().trim(),
                "mere-browser-history-intake-fixture-v1"
            );
            let stage =
                std::env::var("MERE_BROWSER_HISTORY_RECEIPT_STAGE").expect("seed or reopen");
            let backend = muniment::RedbBackend::open(root.join("history.redb")).unwrap();
            let mut store = backend.clone();
            let mut host = if stage == "seed" {
                MereHost::fixture(
                    backend.clone(),
                    selected(),
                    crate::mere_host::fixture_handlers(),
                )
                .unwrap()
            } else {
                assert_eq!(stage, "reopen");
                MereHost::open(
                    backend.clone(),
                    selected(),
                    crate::mere_host::fixture_handlers(),
                    crate::access::AccessContext {
                        persona: PERSONA.into(),
                        device: DEVICE.into(),
                        at_ms: 9000,
                    },
                )
                .await
                .unwrap()
            };
            let a = "https://disk-history.example/a";
            let b = "https://disk-history.example/b";
            let c = "https://disk-history.example/c";
            let visits = vec![
                scoped(
                    visit("2", b, Some(a), 1001),
                    "p",
                    BrowserParent::Visit {
                        visit_id: "1".into(),
                    },
                ),
                scoped(visit("1", a, None, 1000), "p", BrowserParent::Root),
                scoped(
                    visit("3", c, Some(a), 1002),
                    "p",
                    BrowserParent::Visit {
                        visit_id: "1".into(),
                    },
                ),
                scoped(
                    visit("4", b, Some(b), 1003),
                    "p",
                    BrowserParent::Visit {
                        visit_id: "2".into(),
                    },
                ),
                scoped(
                    visit("1", a, None, 1000),
                    "another-profile",
                    BrowserParent::Root,
                ),
            ];
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            if stage == "seed" {
                capture
                    .ingest_batch(
                        &mut host,
                        &mut store,
                        [visits[0].clone()],
                        PERSONA,
                        DEVICE,
                        1,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    browser_encounters(&host, &store)
                        .await
                        .unwrap()
                        .pending
                        .len(),
                    1
                );
                capture
                    .ingest_batch(
                        &mut host,
                        &mut store,
                        visits[1..].to_vec(),
                        PERSONA,
                        DEVICE,
                        2,
                    )
                    .await
                    .unwrap();
            }
            let ledger = browser_encounters(&host, &store).await.unwrap();
            assert_eq!(ledger.snapshot.visits.len(), 5);
            assert!(ledger.pending.is_empty());
            let receipt = serde_json::json!({
                "graph": host.graph_session().manifest().root_graph_id.as_uuid().to_string(),
                "ledger": ledger, "a_to_b": host.browser_relations(a, b).unwrap(),
                "a_to_c": host.browser_relations(a, c).unwrap(), "b_to_b": host.browser_relations(b, b).unwrap(),
            });
            if stage == "seed" {
                std::fs::write(
                    root.join("expected.json"),
                    serde_json::to_vec_pretty(&receipt).unwrap(),
                )
                .unwrap();
            } else {
                assert!(host.was_reopened());
                let expected: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap())
                        .unwrap();
                assert_eq!(receipt, expected);
                let replay = capture
                    .ingest_batch(&mut host, &mut store, visits, PERSONA, DEVICE, 9)
                    .await
                    .unwrap();
                assert!(
                    replay
                        .iter()
                        .all(|result| *result
                            == CaptureOutcome::Dropped(CaptureDropReason::Duplicate))
                );
                assert_eq!(
                    browser_encounters(&host, &store)
                        .await
                        .unwrap()
                        .snapshot
                        .visits
                        .len(),
                    5
                );
            }
            println!(
                "browser history {stage}: five visits, three relations, stable graph/source identity"
            );
        });
    }

    #[test]
    fn stable_legacy_ids_survive_proximity_and_browser_transition_meanings_are_preserved() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let address = "https://legacy.example/revisited";
            let outcomes = capture
                .ingest_batch(
                    &mut host,
                    &mut store,
                    [
                        visit("1", address, None, 1000),
                        visit("2", address, None, 1001),
                    ],
                    PERSONA,
                    DEVICE,
                    1,
                )
                .await
                .unwrap();
            assert!(
                outcomes
                    .iter()
                    .all(|outcome| matches!(outcome, CaptureOutcome::Accepted { .. }))
            );
            let facet: BrowserHistoryFacetV1 = serde_json::from_value(
                host.facet_value(address, BROWSER_HISTORY_FACET)
                    .unwrap()
                    .clone(),
            )
            .unwrap();
            assert_eq!(facet.visit_count, 2);
            let mut generated = scoped(
                visit("3", "https://legacy.example/generated", None, 1002),
                "p",
                BrowserParent::Root,
            );
            generated.transition = "generated".into();
            let mut frame = scoped(
                visit("4", "https://legacy.example/frame", None, 1003),
                "p",
                BrowserParent::Root,
            );
            frame.transition = "auto_subframe".into();
            capture
                .ingest_batch(
                    &mut host,
                    &mut store,
                    [generated, frame],
                    PERSONA,
                    DEVICE,
                    1,
                )
                .await
                .unwrap();
            let ledger = browser_encounters(&host, &store).await.unwrap();
            assert_eq!(
                ledger.snapshot.visits[0].inbound.unwrap().kind,
                chartulary::stemma::TransitionKind::UrlTyped
            );
            assert_eq!(
                ledger.snapshot.visits[1].inbound.unwrap().kind,
                chartulary::stemma::TransitionKind::Unknown
            );
            assert_eq!(
                ledger.snapshot.visits[1].context.raw_transition,
                "auto_subframe"
            );
            assert_eq!(
                capture
                    .memory()
                    .recent_corridor(4)
                    .last()
                    .unwrap()
                    .transition,
                TraceTransition::Restore
            );
        });
    }

    #[test]
    fn extension_production_wire_fixture_enters_the_document_graph() {
        pollster::block_on(async {
            #[derive(Deserialize)]
            struct Delivery {
                policy: HistoryCapturePolicy,
                visits: Vec<BrowserVisit>,
            }
            // Generated by the real background/model scripts against a synthetic
            // browser API. This crosses the JS/Rust schema boundary.
            let delivery: Delivery =
                serde_json::from_str(include_str!("../web/fixtures/browser-history-intake.json"))
                    .unwrap();
            let backend = MemoryBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture = BrowserHistoryCapture::load(&mut store, delivery.policy)
                .await
                .unwrap();
            capture
                .ingest_batch(&mut host, &mut store, delivery.visits, PERSONA, DEVICE, 1)
                .await
                .unwrap();
            let evidence = host
                .browser_relations("https://example.net/from", "https://example.net/to")
                .unwrap();
            assert_eq!(evidence.len(), 1);
            assert_eq!(evidence[0].parent.visit_id, "from");
            assert_eq!(evidence[0].child.visit_id, "to");
            assert_eq!(
                browser_encounters(&host, &store)
                    .await
                    .unwrap()
                    .snapshot
                    .visits
                    .len(),
                2
            );
        });
    }

    #[test]
    fn exported_history_adapter_keeps_source_parent_and_microsecond_evidence() {
        let item: eidetic::import::ImportedHistoryVisitItem = serde_json::from_value(serde_json::json!({
            "page": { "canonical_url": "https://example.org/b", "normalized_title": "B",
                "raw_url": null, "raw_title": "B", "favicon_url": null },
            "visit_id": "12", "source_scope": { "product": "firefox", "profile": "p", "generation": "g" },
            "parent": { "kind": "visit", "visit_id": "11" }, "visited_at_unix_micros": 1_234_567,
            "visited_at_unix_secs": 1, "visit_count_hint": 2, "transition": "Link",
            "referring_url": "https://example.org/a", "session_context": null, "view_time_ms": null,
        })).unwrap();
        let visit = BrowserVisit::from_imported(&item, "firefox");
        assert_eq!(
            visit.parent,
            BrowserParent::Visit {
                visit_id: "11".into()
            }
        );
        assert_eq!(visit.original_unix_micros, Some(1_234_567));
        assert_eq!(visit.at_ms, 1234);
        assert_eq!(visit.source_scope.unwrap().profile, "p");
    }

    #[test]
    fn scoped_encounters_preserve_branches_rapid_revisits_evidence_and_reopen() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let nav_before = host
                .graph()
                .to_snapshot()
                .navigation
                .snapshot()
                .owners
                .clone();
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let a = "https://encounter.example/a";
            let b = "https://encounter.example/b";
            let c = "https://encounter.example/c";
            let parent = |id: &str| BrowserParent::Visit {
                visit_id: id.into(),
            };
            let child = scoped(visit("2", b, Some(a), 1001), "p", parent("1"));
            let first = capture
                .ingest_batch(&mut host, &mut store, [child.clone()], PERSONA, DEVICE, 1)
                .await
                .unwrap();
            assert!(matches!(
                first.as_slice(),
                [CaptureOutcome::Buffered { .. }]
            ));
            assert_eq!(
                browser_encounters(&host, &store)
                    .await
                    .unwrap()
                    .pending
                    .len(),
                1
            );
            let visits = vec![
                scoped(visit("1", a, None, 1000), "p", BrowserParent::Root),
                scoped(visit("3", c, Some(a), 1002), "p", parent("1")),
                scoped(visit("4", b, Some(b), 1003), "p", parent("2")),
                scoped(visit("5", b, Some(b), 1004), "p", parent("4")),
                scoped(
                    visit("1", a, None, 1000),
                    "other-profile",
                    BrowserParent::Root,
                ),
            ];
            capture
                .ingest_batch(&mut host, &mut store, visits.clone(), PERSONA, DEVICE, 2)
                .await
                .unwrap();
            let ledger = browser_encounters(&host, &store).await.unwrap();
            assert_eq!(ledger.snapshot.visits.len(), 6);
            assert!(ledger.pending.is_empty());
            assert_eq!(
                ledger
                    .snapshot
                    .visits
                    .iter()
                    .filter(|visit| visit.parent.is_some())
                    .count(),
                4
            );
            assert!(
                ledger
                    .snapshot
                    .owners
                    .iter()
                    .all(|owner| owner.current.is_none())
            );
            let facet: BrowserHistoryFacetV1 =
                serde_json::from_value(host.facet_value(b, BROWSER_HISTORY_FACET).unwrap().clone())
                    .unwrap();
            assert_eq!(facet.visit_count, 3);
            assert_eq!(facet.encounters.len(), 3);
            assert_eq!(facet.relations.len(), 3);
            assert_eq!(facet.relations[0].parent.visit_id, "1");
            assert_eq!(facet.relations[0].child.visit_id, "2");
            assert_eq!(
                host.browser_relations(a, b).unwrap()[0].kind,
                BrowserRelationKind::FollowedLink
            );
            let a_key = host.graph().get_node_by_url(a).unwrap().0;
            let b_key = host.graph().get_node_by_url(b).unwrap().0;
            let c_key = host.graph().get_node_by_url(c).unwrap().0;
            assert!(
                host.graph()
                    .relations()
                    .any(|relation| relation.kind == RelationKind::Traversal
                        && relation.from == a_key
                        && relation.to == b_key)
            );
            assert!(
                host.graph()
                    .relations()
                    .any(|relation| relation.kind == RelationKind::Traversal
                        && relation.from == a_key
                        && relation.to == c_key)
            );
            let nav_after = host.graph().to_snapshot().navigation;
            for owner in nav_before {
                let after = nav_after
                    .snapshot()
                    .owners
                    .iter()
                    .find(|after| after.identity == owner.identity)
                    .unwrap();
                assert_eq!(after.current, owner.current);
            }
            let reopened = MereHost::open(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
                crate::access::AccessContext {
                    persona: PERSONA.into(),
                    device: DEVICE.into(),
                    at_ms: 3000,
                },
            )
            .await
            .unwrap();
            assert_eq!(
                browser_encounters(&reopened, &store)
                    .await
                    .unwrap()
                    .snapshot
                    .visits
                    .len(),
                6
            );
            let reopened_facet: BrowserHistoryFacetV1 = serde_json::from_value(
                reopened
                    .facet_value(b, BROWSER_HISTORY_FACET)
                    .unwrap()
                    .clone(),
            )
            .unwrap();
            assert_eq!(facet, reopened_facet);
            let mut host = reopened;
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let replay = capture
                .ingest_batch(
                    &mut host,
                    &mut store,
                    visits.into_iter().chain([child]),
                    PERSONA,
                    DEVICE,
                    3,
                )
                .await
                .unwrap();
            assert!(
                replay.iter().all(
                    |outcome| *outcome == CaptureOutcome::Dropped(CaptureDropReason::Duplicate)
                )
            );
            assert_eq!(
                browser_encounters(&host, &store)
                    .await
                    .unwrap()
                    .snapshot
                    .visits
                    .len(),
                6
            );
            capture
                .forget_url(
                    &mut host,
                    &mut store,
                    a,
                    ForgetMode::RemoveCapturedObject,
                    4,
                )
                .await
                .unwrap();
            assert!(host.browser_relations(a, b).unwrap().is_empty());
            assert!(
                browser_encounters(&host, &store)
                    .await
                    .unwrap()
                    .pending
                    .iter()
                    .all(|visit| visit.url != a)
            );
            assert!(host.graph().get_node_by_url(a).is_none());
            let ledger_bytes = store.get(&encounter_key(&host)).await.unwrap().unwrap();
            assert!(!String::from_utf8_lossy(&ledger_bytes).contains(a));
            // Resource identity has independent ownership. Forgetting browser
            // encounters does not erase the shared resource registry.
        });
    }

    #[test]
    fn scoped_pending_outside_range_withheld_parent_and_conflict_are_explicit() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut policy = HistoryCapturePolicy::consented();
            policy
                .excluded_origins
                .insert("https://private.example".into());
            let mut capture = BrowserHistoryCapture::load(&mut store, policy)
                .await
                .unwrap();
            let outside = scoped(
                visit(
                    "outside-child",
                    "https://encounter.example/outside",
                    None,
                    42,
                ),
                "p",
                BrowserParent::OutsideRange {
                    visit_id: "missing".into(),
                },
            );
            let withheld = scoped(
                visit(
                    "withheld",
                    "https://encounter.example/withheld",
                    Some("https://private.example/secret"),
                    43,
                ),
                "p",
                BrowserParent::Visit {
                    visit_id: "private-parent".into(),
                },
            );
            capture
                .ingest_batch(
                    &mut host,
                    &mut store,
                    [outside, withheld.clone()],
                    PERSONA,
                    DEVICE,
                    1,
                )
                .await
                .unwrap();
            let ledger = browser_encounters(&host, &store).await.unwrap();
            assert_eq!(ledger.pending.len(), 1);
            assert_eq!(
                ledger.snapshot.visits[0].context.parent,
                BrowserParent::Withheld
            );
            assert!(
                host.graph()
                    .get_node_by_url("https://private.example/secret")
                    .is_none()
            );
            let stored = store.get(&encounter_key(&host)).await.unwrap().unwrap();
            assert!(!String::from_utf8_lossy(&stored).contains("private.example"));
            let mut conflict = withheld;
            conflict.at_ms += 1;
            assert!(
                capture
                    .ingest_batch(&mut host, &mut store, [conflict], PERSONA, DEVICE, 2)
                    .await
                    .is_err()
            );
            assert_eq!(
                store.get(&encounter_key(&host)).await.unwrap().unwrap(),
                stored
            );
        });
    }

    #[test]
    fn scoped_atomic_retry_does_not_duplicate_document_relations() {
        pollster::block_on(async {
            let backend = RejectingApplyBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let a = "https://retry.example/a";
            let b = "https://retry.example/b";
            let visits = vec![
                scoped(visit("a", a, None, 1000), "p", BrowserParent::Root),
                scoped(
                    visit("b", b, Some(a), 1001),
                    "p",
                    BrowserParent::Visit {
                        visit_id: "a".into(),
                    },
                ),
            ];
            backend.reject.store(true, Ordering::SeqCst);
            assert!(
                capture
                    .ingest_batch(&mut host, &mut store, visits.clone(), PERSONA, DEVICE, 1)
                    .await
                    .is_err()
            );
            assert!(
                browser_encounters(&host, &store)
                    .await
                    .unwrap()
                    .snapshot
                    .visits
                    .is_empty()
            );
            backend.reject.store(false, Ordering::SeqCst);
            capture
                .ingest_batch(&mut host, &mut store, visits, PERSONA, DEVICE, 1)
                .await
                .unwrap();
            assert_eq!(
                browser_encounters(&host, &store)
                    .await
                    .unwrap()
                    .snapshot
                    .visits
                    .len(),
                2
            );
            let facet: BrowserHistoryFacetV1 =
                serde_json::from_value(host.facet_value(b, BROWSER_HISTORY_FACET).unwrap().clone())
                    .unwrap();
            assert_eq!(facet.visit_count, 1);
            assert_eq!(facet.relations.len(), 1);
            assert_eq!(
                list_typed::<AccessRecord>(&mut store).await.unwrap().len(),
                2
            );
        });
    }

    #[derive(Clone)]
    struct RejectingApplyBackend {
        inner: MemoryBackend,
        reject: Arc<AtomicBool>,
    }

    impl RejectingApplyBackend {
        fn new() -> Self {
            Self {
                inner: MemoryBackend::new(),
                reject: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    #[async_trait]
    impl Backend for RejectingApplyBackend {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            self.inner.get(key).await
        }

        async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
            self.inner.put(key, bytes).await
        }

        async fn delete(&self, key: &str) -> Result<(), StoreError> {
            self.inner.delete(key).await
        }

        async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
            self.inner.list(prefix).await
        }

        async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
            self.inner.scan(start, end).await
        }

        async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
            if self.reject.load(Ordering::SeqCst) {
                return Err(StoreError::Backend(
                    "capture transaction rejected".to_string(),
                ));
            }
            self.inner.apply(ops).await
        }
    }

    #[test]
    fn policy_rejects_unconsented_private_internal_excluded_and_duplicate_visits() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut store = backend.clone();
            let disabled =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::disabled())
                    .await
                    .unwrap();
            assert_eq!(
                disabled
                    .normalize(visit("1", "https://example.net/a", None, 1))
                    .unwrap_err(),
                CaptureDropReason::Disabled
            );

            let mut policy = HistoryCapturePolicy::consented();
            policy
                .excluded_origins
                .insert("https://private.example".to_string());
            policy.strip_query = true;
            let capture = BrowserHistoryCapture::load(&mut store, policy)
                .await
                .unwrap();

            let mut private = visit("2", "https://example.net/a", None, 2);
            private.private = true;
            assert_eq!(
                capture.normalize(private).unwrap_err(),
                CaptureDropReason::PrivateWindow
            );
            assert_eq!(
                capture
                    .normalize(visit("3", "about:config", None, 3))
                    .unwrap_err(),
                CaptureDropReason::UnsupportedScheme
            );
            assert_eq!(
                capture
                    .normalize(visit("4", "https://private.example/a", None, 4))
                    .unwrap_err(),
                CaptureDropReason::ExcludedOrigin
            );
            let normalized = capture
                .normalize(visit("5", "https://example.net/a?secret=1#part", None, 5))
                .unwrap();
            assert_eq!(normalized.url, "https://example.net/a");
        });
    }

    #[test]
    fn consented_batch_projects_access_traversal_memory_reopen_and_forget() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut memory_store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture =
                BrowserHistoryCapture::load(&mut memory_store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let from = "https://capture.example/from";
            let to = "https://capture.example/to";
            let mut private = visit("private", "https://capture.example/private", None, 3_000);
            private.private = true;
            let outcomes = capture
                .ingest_batch(
                    &mut host,
                    &mut memory_store,
                    [
                        visit("from", from, None, 1_000),
                        visit("to", to, Some(from), 2_000),
                        visit("to", to, Some(from), 2_000),
                        private,
                    ],
                    PERSONA,
                    DEVICE,
                    2,
                )
                .await
                .unwrap();
            assert_eq!(
                outcomes
                    .iter()
                    .filter(|outcome| matches!(outcome, CaptureOutcome::Accepted { .. }))
                    .count(),
                2
            );
            assert_eq!(
                outcomes,
                vec![
                    outcomes[0].clone(),
                    outcomes[1].clone(),
                    CaptureOutcome::Dropped(CaptureDropReason::Duplicate),
                    CaptureOutcome::Dropped(CaptureDropReason::PrivateWindow),
                ]
            );
            let access = host.access_history_for(to).unwrap();
            assert_eq!(access.records.len(), 1);
            assert_eq!(access.records[0].persona, PERSONA);
            assert_eq!(access.records[0].device, DEVICE);
            assert_eq!(access.records[0].handler, "browser.history/firefox");
            let facet: BrowserHistoryFacetV1 = serde_json::from_value(
                host.facet_value(to, BROWSER_HISTORY_FACET).unwrap().clone(),
            )
            .unwrap();
            assert!(facet.created_by_capture);
            assert_eq!(facet.visit_count, 1);
            assert_eq!(facet.latest_title.as_deref(), Some("to title"));
            assert!(host.graph().relations().any(|relation| {
                relation.kind == RelationKind::Traversal
                    && host.graph().get_node(relation.from).unwrap().url() == from
                    && host.graph().get_node(relation.to).unwrap().url() == to
            }));
            assert_eq!(capture.memory().recent_corridor(8).len(), 2);
            let authors: Vec<Author> = host
                .graph_session()
                .changes()
                .iter()
                .map(|change| change.author.clone())
                .collect();
            assert!(
                authors.contains(
                    &Author::person(FIXTURE_PERSONA_ADDRESS).via("browser.extension.firefox")
                ),
                "a captured visit comes through the extension that captured it: {authors:?}"
            );

            let reopened = MereHost::open(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
                AccessContext {
                    persona: PERSONA.to_string(),
                    device: DEVICE.to_string(),
                    at_ms: 4_000,
                },
            )
            .await
            .unwrap();
            assert!(reopened.graph().get_node_by_url(to).is_some());
            let reloaded_memory = BrowsingMemory::load(&mut memory_store, 64).await.unwrap();
            assert_eq!(reloaded_memory.recent_corridor(8).len(), 2);
            let manifests = list_typed::<AccessRecord>(&mut memory_store).await.unwrap();
            assert_eq!(manifests.len(), 2);
            assert!(
                manifests
                    .iter()
                    .all(|manifest| manifest.privacy == PrivacyClass::LocalOnly)
            );
            let filtered = query_access_records(
                &mut memory_store,
                &AccessRecordFilter {
                    start_ms: Some(1_500),
                    end_ms: Some(2_500),
                    persona: Some(PERSONA.to_string()),
                    device: Some(DEVICE.to_string()),
                },
            )
            .await
            .unwrap();
            assert_eq!(filtered.len(), 1);
            assert_eq!(filtered[0].address, to);
            assert!(
                query_access_records(
                    &mut memory_store,
                    &AccessRecordFilter {
                        persona: Some("persona:someone-else".to_string()),
                        ..AccessRecordFilter::default()
                    },
                )
                .await
                .unwrap()
                .is_empty()
            );
            let mut reloaded_capture =
                BrowserHistoryCapture::load(&mut memory_store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            assert_eq!(
                reloaded_capture
                    .ingest_batch(
                        &mut host,
                        &mut memory_store,
                        [visit("to", to, Some(from), 2_000)],
                        PERSONA,
                        DEVICE,
                        3,
                    )
                    .await
                    .unwrap(),
                vec![CaptureOutcome::Dropped(CaptureDropReason::Duplicate)]
            );

            let mut excluded_after_capture = capture.policy().clone();
            excluded_after_capture
                .excluded_origins
                .insert("https://capture.example".to_string());
            capture.set_policy(excluded_after_capture);
            capture
                .forget_url(
                    &mut host,
                    &mut memory_store,
                    to,
                    ForgetMode::RemoveCapturedObject,
                    3,
                )
                .await
                .unwrap();
            assert!(host.graph().get_node_by_url(to).is_none());
            assert!(
                BrowsingMemory::load(&mut memory_store, 64)
                    .await
                    .unwrap()
                    .recent_corridor(8)
                    .is_empty()
            );
            assert_eq!(
                list_typed::<AccessRecord>(&mut memory_store)
                    .await
                    .unwrap()
                    .len(),
                1,
                "forget deletes access authority records that mention the address"
            );
            assert!(
                host.graph().get_node_by_url(FIXTURE_WEB_ADDRESS).is_some(),
                "forget remains scoped to the captured address"
            );
            capture.set_policy(HistoryCapturePolicy::consented());
            assert_eq!(
                host.access_history_for(FIXTURE_WEB_ADDRESS)
                    .unwrap()
                    .records
                    .len(),
                2
            );
            let recaptured = capture
                .ingest_batch(
                    &mut host,
                    &mut memory_store,
                    [visit("to", to, None, 4_000)],
                    PERSONA,
                    DEVICE,
                    4,
                )
                .await
                .unwrap();
            assert!(
                matches!(recaptured.as_slice(), [CaptureOutcome::Accepted { .. }]),
                "forget also clears stable source-event dedupe state"
            );
        });
    }

    #[test]
    fn capture_batch_is_atomic_and_the_same_delivery_retries_cleanly() {
        pollster::block_on(async {
            let backend = RejectingApplyBackend::new();
            let mut store = backend.clone();
            let mut host = MereHost::fixture(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
            )
            .unwrap();
            let mut capture =
                BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                    .await
                    .unwrap();
            let address = "https://capture.example/atomic";
            backend.reject.store(true, Ordering::SeqCst);
            assert!(
                capture
                    .ingest_batch(
                        &mut host,
                        &mut store,
                        [visit("atomic", address, None, 5_000)],
                        PERSONA,
                        DEVICE,
                        5,
                    )
                    .await
                    .is_err()
            );
            assert!(
                pandect::MereSessions::new(backend.clone())
                    .list()
                    .await
                    .unwrap()
                    .is_empty(),
                "a rejected batch stores no session"
            );
            assert!(
                list_typed::<AccessRecord>(&mut store)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(
                BrowsingMemory::load(&mut store, 64)
                    .await
                    .unwrap()
                    .recent_corridor(8)
                    .is_empty()
            );

            backend.reject.store(false, Ordering::SeqCst);
            assert!(matches!(
                capture
                    .ingest_batch(
                        &mut host,
                        &mut store,
                        [visit("atomic", address, None, 5_000)],
                        PERSONA,
                        DEVICE,
                        5,
                    )
                    .await
                    .unwrap()
                    .as_slice(),
                [CaptureOutcome::Accepted { .. }]
            ));
            let reopened = MereHost::open(
                backend.clone(),
                selected(),
                crate::mere_host::fixture_handlers(),
                AccessContext {
                    persona: PERSONA.to_string(),
                    device: DEVICE.to_string(),
                    at_ms: 6_000,
                },
            )
            .await
            .unwrap();
            assert!(reopened.graph().get_node_by_url(address).is_some());
            assert_eq!(
                list_typed::<AccessRecord>(&mut store).await.unwrap().len(),
                1
            );
            assert_eq!(
                BrowsingMemory::load(&mut store, 64)
                    .await
                    .unwrap()
                    .recent_corridor(8)
                    .len(),
                1
            );
        });
    }
}
