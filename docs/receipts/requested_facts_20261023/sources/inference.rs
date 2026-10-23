// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The resident inference lane: one esp inference actor behind an
//! application route (resident inference and recall plan, I1).
//!
//! [`InferenceLane`] owns the actor and a table of runs. A bridge thread
//! folds the actor's updates into the table, so the lane needs no async
//! executor and composes the same way in a desktop or an embedded resident.
//! [`InferenceAskEndpoint`] is the route: `Ask` and `Cancel` intents in, a
//! card whose media is the run table out, and a payload-free notice when the
//! table changes. The route's notice poll interval is therefore the rate at
//! which streamed fragments reach a client.
//!
//! `inference-model` adds local model-library resolution and a session-bound
//! declared template. A persona journal adds token checkpoints and lock pause/
//! resume (I3); recall and citations remain I4.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use armillary::actor::{ActorHandle, Wake};
use chirograph::{
    BoundsRelationship, CachePolicy, CardValueV1, CarrierNotice, ContentHash, EndpointDescriptor,
    IntentInvocation, IntentResult, PortableCardV1, PresentationBinding, PresentationCapability,
    PresentationCodec, PresentationKey, PresentationManifest, PresentationOffer,
    PresentationSemantics, ProjectionOffer, ProjectionRequest, ProjectionSession,
    ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse, SemanticRole,
};
use esp::infer::actor::spawn_inference_actor_with_checkpoints;
pub use esp::infer::provider::GenerationCheckpoint;
use esp::infer::{GenerationRequest, InferCommand, InferUpdate};
mod observations;
pub use observations::InferenceObservation;
#[cfg(feature = "inference-model")]
mod answer_contract;
#[cfg(feature = "inference-model")]
pub mod durable;
#[cfg(feature = "inference-model")]
pub mod recall;
#[cfg(feature = "inference-embedding")]
pub mod recall_embedding;
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Representation, Scene, Score, Size2,
    SourceRef, Transform2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use serde::{Deserialize, Serialize};

#[cfg(feature = "inference-gpu")]
mod gpu;
#[cfg(feature = "inference-gpu")]
pub use gpu::{
    discrete_gpu_inference_device, open_gpu_library_lane, open_gpu_library_lane_with_journal,
    open_gpu_library_lane_with_journal_and_cache,
};

#[cfg(feature = "inference-model")]
pub use esp::infer::decoder::DecoderDevice as InferenceDevice;
/// The provider seam a host builds a lane from, re-exported so a host
/// composes the lane without naming esp.
pub use esp::infer::{InferError, InferenceProvider, StubInferenceProvider};
#[cfg(feature = "inference-model")]
pub use esp::infer::{
    layer_library::import_layered_model,
    library::{InferenceModelRow, load_library_session, resolve_library_session},
    prompt::PromptTemplate,
};

/// Parse the self-describing content reference from owner settings.
#[cfg(feature = "inference-model")]
pub fn parse_model_row(row: &str) -> Result<eidetic::ManifestId, String> {
    eidetic::Hash::parse(row)
        .map(eidetic::ManifestId::from_hash)
        .map_err(|error| format!("inference model row {row:?}: {error}"))
}

#[cfg(feature = "inference-model")]
pub(crate) fn library_layer_source<B: eidetic::Store + Send + 'static>(
    library: Arc<tokio::sync::Mutex<B>>,
    resolved: &esp::infer::library::ResolvedInferenceRow,
) -> Option<Arc<dyn esp::infer::decoder::streaming::LayerSource>> {
    resolved.artifact_source(move |artifact, limit| {
        let mut store = library.blocking_lock();
        pollster::block_on(artifact.read(&mut *store, limit))
    })
}

#[cfg(feature = "inference-model")]
pub(crate) fn borrow_layer_source(
    source: &Option<Arc<dyn esp::infer::decoder::streaming::LayerSource>>,
) -> Option<Box<dyn esp::infer::decoder::streaming::LayerSource>> {
    source.as_ref().map(|source| {
        let source = source.clone();
        Box::new(source) as Box<dyn esp::infer::decoder::streaming::LayerSource>
    })
}

/// The library lock covers resolution and individual streamed layer reads.
/// It is released before tensor work, so the trainer can publish artifacts
/// between reads while a CPU ask is running.
#[cfg(feature = "inference-model")]
pub fn open_library_lane<B: eidetic::Store + Send + 'static>(
    library: Arc<tokio::sync::Mutex<B>>,
    row: eidetic::ManifestId,
    max_tokens: usize,
) -> InferenceLane {
    open_library_lane_with_journal(library, row, max_tokens, None).expect("ephemeral library lane")
}

#[cfg(feature = "inference-model")]
pub fn open_library_lane_with_journal<B: eidetic::Store + Send + 'static>(
    library: Arc<tokio::sync::Mutex<B>>,
    row: eidetic::ManifestId,
    max_tokens: usize,
    journal: Option<Arc<dyn RunJournal>>,
) -> Result<InferenceLane, String> {
    InferenceLane::try_open_with_journal(
        max_tokens,
        move || {
            let resolved = {
                let mut store = library.blocking_lock();
                pollster::block_on(esp::infer::library::resolve_library_session(
                    &mut *store,
                    row,
                ))?
            };
            let (device, loader) = if resolved.weight_format().is_some() {
                (InferenceDevice::flex(), "burn-flex")
            } else {
                (InferenceDevice::ndarray(), "burn-ndarray")
            };
            let source = library_layer_source(library, &resolved);
            let provider = resolved.load_with_layer_source(
                &device,
                loader,
                max_tokens,
                borrow_layer_source(&source),
            )?;
            Ok(Box::new(provider))
        },
        journal,
    )
}

/// Decoder-free application wire types; the runtime keeps its provider identity.
pub use crate::inference_protocol::{
    ASK_INTENT, ASK_SAMPLED_INTENT, AskRequestV1, CANCEL_INTENT, CancelRequestV1, Continuation,
    INFERENCE_ASK_NOTICE_POLL, INFERENCE_ASK_ROUTE, INFERENCE_RUNS_SCHEMA, PAUSE_INTENT,
    RESUME_INTENT, RunStateV1, SampledAskRequestV1,
};
pub type RunV1 = crate::inference_protocol::RunV1<esp::infer::provider::SessionIdentity>;
pub type InferenceRunsV1 =
    crate::inference_protocol::InferenceRunsV1<esp::infer::provider::SessionIdentity>;

/// Why the lane would not take an ask or a cancel.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InferenceLaneError {
    /// The run name is already in the table.
    #[error("run {0:?} already exists")]
    DuplicateRun(String),
    /// No run by that name.
    #[error("no run {0:?}")]
    UnknownRun(String),
    /// The run name or question is empty.
    #[error("{0} is empty")]
    Empty(&'static str),
    /// The lane has closed.
    #[error("the inference lane is closed")]
    Closed,
    #[error("the inference route is locked")]
    Locked,
    #[error("inference journal: {0}")]
    Journal(String),
    #[error("inference recall: {0}")]
    Recall(String),
}

/// Retained host state. Journals serialize this beside Eidetic evidence;
/// execution IDs and in-flight flags are local materialization details.
#[derive(Clone, Serialize, Deserialize)]
pub struct RunRecord {
    #[serde(default)]
    weight_format: Option<String>,
    view: RunV1,
    actor_id: u64,
    request: GenerationRequest,
    checkpoint: Option<GenerationCheckpoint>,
    #[serde(default)]
    continuations: Vec<Continuation>,
    #[serde(default)]
    pause_reason: Option<String>,
    #[serde(skip)]
    executing: bool,
}

#[cfg(feature = "inference-model")]
impl Drop for RunRecord {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.view.question.zeroize();
        self.view.text.zeroize();
        self.request.clear_private();
    }
}

/// Synchronous durable barrier supplied by the persona's log owner.
pub trait RunJournal: Send + Sync {
    fn device(&self) -> &str;
    fn load(&self) -> Result<Vec<RunRecord>, String>;
    fn append(&self, record: &RunRecord) -> Result<(), String>;
    /// Historical reads can remain available after execution authority ends.
    fn execution_permitted(&self) -> bool {
        true
    }
    fn remote_revision(&self) -> u64 {
        0
    }
}

#[derive(Default)]
struct RunTable {
    observations: Option<observations::Observations>,
    weight_format: Option<String>,
    model: Option<String>,
    session: Option<esp::infer::provider::SessionIdentity>,
    load_error: Option<String>,
    disconnected: bool,
    closing: bool,
    actor_failure: Option<String>,
    journal_failures: Vec<String>,
    runs: Vec<RunRecord>,
    by_actor_id: HashMap<u64, usize>,
    next_actor_id: u64,
    locked: bool,
    seen_remote_revision: u64,
    #[cfg(feature = "inference-model")]
    seen_recall_revision: Option<[u8; 32]>,
    /// Bumped on every change a client can see.
    generation: u64,
}

fn capability_weight_format(capability: &esp::infer::ModelCapability) -> Option<String> {
    capability
        .quantization
        .clone()
        .or_else(|| capability.loader.starts_with("burn-").then(|| "f32".into()))
}

impl RunTable {
    fn observe(&mut self, id: u64, phase: InferenceObservation) {
        if let Some(observations) = &mut self.observations {
            observations.note(id, phase);
        }
    }
    fn view(&self) -> InferenceRunsV1 {
        InferenceRunsV1 {
            schema: INFERENCE_RUNS_SCHEMA.into(),
            model: self.model.clone(),
            session: self.session.clone(),
            load_error: self.load_error.clone(),
            runs: self
                .runs
                .iter()
                .map(|record| {
                    let mut view = record.view.clone();
                    view.tokens = record
                        .checkpoint
                        .as_ref()
                        .map_or(0, |cp| cp.token_ids.len());
                    view.continuations = record.continuations.clone();
                    view
                })
                .collect(),
        }
    }

    fn by_actor(&mut self, id: u64) -> Option<&mut RunV1> {
        let index = *self.by_actor_id.get(&id)?;
        Some(&mut self.runs[index].view)
    }

    /// Fold one actor update. Updates for a run that has already ended are
    /// dropped, so nothing reaches a cancelled run after its cancel lands.
    /// Returns admission to this run table, not successful domain completion.
    fn apply(&mut self, update: InferUpdate) -> bool {
        let id = match &update {
            InferUpdate::Started { id }
            | InferUpdate::Paused { id }
            | InferUpdate::Checkpoint { id, .. }
            | InferUpdate::Fragment { id, .. }
            | InferUpdate::Finished { id, .. }
            | InferUpdate::Cancelled { id }
            | InferUpdate::Failed { id, .. } => Some(*id),
            _ => None,
        };
        if let Some(id) = id {
            let Some(&index) = self.by_actor_id.get(&id) else {
                return false;
            };
            if self.runs[index].view.state.is_terminal() {
                return false;
            }
        }
        match update {
            InferUpdate::Ready { capability } => {
                self.weight_format = capability_weight_format(&capability);
                for record in &mut self.runs {
                    if record.view.session.is_none() && !record.view.state.is_terminal() {
                        record.weight_format = self.weight_format.clone();
                    }
                }
                self.model = Some(capability.model_id);
            },
            InferUpdate::Session { identity } => {
                for record in &mut self.runs {
                    if record.view.session.is_none()
                        && matches!(record.view.state, RunStateV1::Queued | RunStateV1::Running)
                    {
                        record.view.session = Some(identity.clone());
                    }
                }
                self.session = Some(identity);
            },
            InferUpdate::LoadFailed { error } => self.load_error = Some(error.to_string()),
            InferUpdate::Started { id } => {
                if let Some(index) = self.by_actor_id.get(&id).copied() {
                    self.runs[index].executing = true;
                }
                if let Some(run) = self.by_actor(id).filter(|run| !run.state.is_terminal()) {
                    run.state = RunStateV1::Running;
                }
            },
            InferUpdate::Paused { id } => {
                if let Some(index) = self.by_actor_id.get(&id).copied() {
                    self.runs[index].executing = false;
                }
                if let Some(run) = self.by_actor(id).filter(|run| !run.state.is_terminal()) {
                    run.state = RunStateV1::Paused;
                }
            },
            // The synchronous commit barrier already published this state.
            // Replaying an earlier queued update would regress its token count.
            InferUpdate::Checkpoint { .. } => {},
            InferUpdate::Fragment { id, text } => {
                if let Some(run) = self.by_actor(id).filter(|run| !run.state.is_terminal()) {
                    run.text.push_str(&text);
                }
            },
            InferUpdate::Finished { id, text } => {
                if let Some(index) = self.by_actor_id.get(&id).copied() {
                    self.runs[index].executing = false;
                }
                if let Some(run) = self.by_actor(id).filter(|run| !run.state.is_terminal()) {
                    run.state = RunStateV1::Finished;
                    run.text = text;
                }
            },
            InferUpdate::Cancelled { id } => {
                if let Some(index) = self.by_actor_id.get(&id).copied() {
                    self.runs[index].executing = false;
                }
                if let Some(run) = self.by_actor(id).filter(|run| !run.state.is_terminal()) {
                    run.state = RunStateV1::Cancelled;
                }
            },
            InferUpdate::Failed { id, error } => {
                if let Some(index) = self.by_actor_id.get(&id).copied() {
                    self.runs[index].executing = false;
                }
                if let Some(run) = self.by_actor(id).filter(|run| !run.state.is_terminal()) {
                    run.state = RunStateV1::Failed;
                    run.error = Some(error.to_string());
                }
            },
        }
        self.generation += 1;
        true
    }
}

struct Shared {
    table: Arc<Mutex<RunTable>>,
    journal: Option<Arc<dyn RunJournal>>,
    actor: Mutex<Option<ActorHandle<InferCommand>>>,
    bridge: Mutex<Option<JoinHandle<()>>>,
    close_result: Mutex<Option<Result<(), String>>>,
    max_tokens: usize,
    caches: Mutex<Vec<std::sync::Weak<Mutex<Option<(Vec<u8>, Vec<u8>)>>>>>,
    #[cfg(feature = "inference-model")]
    recall: Mutex<Option<RecallBinding>>,
    #[cfg(feature = "inference-embedding")]
    embedding: Mutex<Option<Arc<recall_embedding::RecallEmbedding>>>,
    #[cfg(feature = "inference-model")]
    published: Mutex<PublishedRecall>,
}

#[cfg(feature = "inference-model")]
struct RecallBinding {
    corpus: Arc<dyn recall::RecallCorpus>,
    config: recall::RecallConfig,
    index: Option<recall::RecallIndex>,
}

#[cfg(feature = "inference-model")]
#[derive(Default)]
struct PublishedRecall {
    config: recall::RecallConfig,
    owners: std::collections::BTreeMap<String, PublishedBinding>,
}
#[cfg(feature = "inference-model")]
struct PublishedBinding {
    publisher: ProjectionSession,
    binding: RecallBinding,
}
#[cfg(feature = "inference-model")]
enum RecallSelection {
    Local(
        Vec<eidetic::models::recall::RecallSourceId>,
        recall::AnswerFormat,
    ),
    Published {
        owner: String,
        publisher: ProjectionSession,
        answer_format: recall::AnswerFormat,
        scope: Vec<eidetic::models::recall::RecallSourceId>,
    },
}

impl Shared {
    // The update stream closing is a transport/lifetime fact, never a successful
    // completion for unfinished domain work. Keep retained user pauses intact.
    fn record_disconnect(&self, table: &mut RunTable) {
        if table.disconnected {
            return;
        }
        table.disconnected = true;
        let message = "inference actor disconnected before completing work";
        if !table.closing {
            table.load_error.get_or_insert_with(|| message.into());
            table.actor_failure.get_or_insert_with(|| message.into());
        }
        for record in &mut table.runs {
            if record.executing
                || matches!(record.view.state, RunStateV1::Queued | RunStateV1::Running)
            {
                record.executing = false;
                record.view.state = RunStateV1::Failed;
                record.view.error = Some(message.into());
                table.actor_failure.get_or_insert_with(|| message.into());
                if let Some(journal) = &self.journal {
                    if let Err(error) = journal.append(record) {
                        let error = format!("actor disconnect checkpoint failed: {error}");
                        record.view.error = Some(error.clone());
                        table.journal_failures.push(error);
                    }
                }
                if let Some(observations) = &mut table.observations {
                    observations.note(record.actor_id, InferenceObservation::RunFailed);
                }
            }
        }
        table.generation += 1;
    }
    fn require_actor(
        &self,
        actor: &ActorHandle<InferCommand>,
        table: &RunTable,
    ) -> Result<(), InferenceLaneError> {
        if table.disconnected || actor.is_finished() {
            // Only the bridge adjudicates domain termination, after draining all
            // updates already emitted before the actor exited.
            Err(InferenceLaneError::Closed)
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "inference-model")]
    fn build_recall(
        &self,
        corpus: Arc<dyn recall::RecallCorpus>,
        config: recall::RecallConfig,
    ) -> Result<recall::RecallIndex, String> {
        #[cfg(feature = "inference-embedding")]
        if let Some(runtime) = self.embedding.lock().unwrap().as_ref() {
            return recall::RecallIndex::build_with_embedding(corpus, config, runtime);
        }
        recall::RecallIndex::build(corpus, config)
    }

    #[cfg(feature = "inference-model")]
    fn observe_recall(&self, table: &mut RunTable) -> recall::ObservedSources {
        let mut observed = recall::ObservedSources::default();
        if table.locked {
            return observed;
        }
        let mut sources = std::collections::BTreeSet::new();
        for record in &table.runs {
            if let Some(evidence) = &record.view.recall {
                sources.extend(evidence.passages.iter().map(|p| p.passage.source.clone()));
                sources.extend(evidence.stale.iter().map(|p| p.source.clone()));
            }
        }
        if sources.is_empty() {
            table.seen_recall_revision = None;
            return observed;
        }
        let binding = self.recall.lock().unwrap();
        let published = self.published.lock().unwrap();
        for source in sources {
            let source_binding = match source.owner.as_deref() {
                Some(owner) => published.owners.get(owner).map(|slot| &slot.binding),
                None => binding.as_ref(),
            };
            let current = source_binding
                .ok_or_else(|| "source owner is unavailable".to_string())
                .and_then(|binding| binding.corpus.current_version(&source));
            observed.0.insert(source, current);
        }
        let encoded = zeroize::Zeroizing::new(
            serde_json::to_vec(&observed.0.iter().collect::<Vec<_>>())
                .expect("source revision serialization"),
        );
        let revision = *eidetic::Hash::of(&encoded).as_bytes();
        if table
            .seen_recall_revision
            .is_some_and(|previous| previous != revision)
        {
            table.generation += 1;
        }
        table.seen_recall_revision = Some(revision);
        observed
    }
    fn view(&self, table: &mut RunTable) -> InferenceRunsV1 {
        #[cfg(feature = "inference-model")]
        let observed = self.observe_recall(table);
        #[allow(unused_mut)]
        let mut view = table.view();
        #[cfg(feature = "inference-model")]
        for run in &mut view.runs {
            if let Some(evidence) = &mut run.recall {
                if run.state == RunStateV1::Finished {
                    evidence.support = evidence.assess_answer(&run.text, &run.question);
                }
                evidence.validate(&run.text, Some(&observed));
            }
        }
        view
    }
    fn table(&self) -> MutexGuard<'_, RunTable> {
        self.table.lock().expect("the run table is never poisoned")
    }
}

/// One inference actor and the runs asked of it. Cheap to clone; every clone
/// is the same lane.
#[derive(Clone)]
pub struct InferenceLane {
    shared: Arc<Shared>,
}

impl InferenceLane {
    /// Start the actor. `build` runs on the actor's own thread and constructs
    /// or loads the provider there.
    pub fn open<F>(build: F) -> Self
    where
        F: FnOnce() -> Box<dyn InferenceProvider> + Send + 'static,
    {
        Self::try_open(move || Ok(build()))
    }

    /// Start a model-backed actor; load failures remain visible on the route.
    pub fn try_open<F>(build: F) -> Self
    where
        F: FnOnce() -> Result<Box<dyn InferenceProvider>, esp::infer::InferError> + Send + 'static,
    {
        Self::try_open_with_max_tokens(GenerationRequest::default().max_tokens, build)
    }

    fn try_open_with_max_tokens<F>(max_tokens: usize, build: F) -> Self
    where
        F: FnOnce() -> Result<Box<dyn InferenceProvider>, esp::infer::InferError> + Send + 'static,
    {
        Self::try_open_with_journal(max_tokens, build, None)
            .expect("an ephemeral lane has no journal to fail")
    }

    /// Open retained runs before admitting work; local interrupted asks resume.
    pub fn try_open_with_journal<F>(
        max_tokens: usize,
        build: F,
        journal: Option<Arc<dyn RunJournal>>,
    ) -> Result<Self, String>
    where
        F: FnOnce() -> Result<Box<dyn InferenceProvider>, esp::infer::InferError> + Send + 'static,
    {
        Self::try_open_with_journal_and_bridge_gate(
            max_tokens,
            build,
            journal,
            #[cfg(test)]
            None,
        )
    }

    fn try_open_with_journal_and_bridge_gate<F>(
        max_tokens: usize,
        build: F,
        journal: Option<Arc<dyn RunJournal>>,
        #[cfg(test)] bridge_gate: Option<Receiver<()>>,
    ) -> Result<Self, String>
    where
        F: FnOnce() -> Result<Box<dyn InferenceProvider>, esp::infer::InferError> + Send + 'static,
    {
        let mut table = RunTable::default();
        let mut recover = Vec::new();
        if let Some(journal) = &journal {
            for mut record in journal.load()? {
                record.actor_id = table.next_actor_id;
                table.next_actor_id += 1;
                let local = record
                    .continuations
                    .last()
                    .is_some_and(|visit| visit.device == journal.device());
                if !record.view.state.is_terminal() {
                    if local
                        && journal.execution_permitted()
                        && (record.view.state != RunStateV1::Paused
                            || record.pause_reason.as_deref() == Some("shutdown")
                            || record.pause_reason.as_deref() == Some("lock"))
                    {
                        recover.push(record.actor_id);
                    }
                    record.view.state = RunStateV1::Paused;
                }
                let index = table.runs.len();
                table.by_actor_id.insert(record.actor_id, index);
                table.runs.push(record);
            }
        }
        let table = Arc::new(Mutex::new(table));
        let commit_table = table.clone();
        let commit_journal = journal.clone();
        let wake: Wake = Arc::new(|| {});
        let ready_table = table.clone();
        let build = move || {
            let provider = build()?;
            // The first durable commit can outrun the asynchronous Ready
            // observer. Publish exact format/session facts on the actor's
            // construction path before it can execute a queued request.
            let mut table = ready_table.lock().unwrap();
            table.weight_format = capability_weight_format(provider.capability());
            table.session = provider.session_identity();
            Ok(provider)
        };
        let (actor, updates) =
            spawn_inference_actor_with_checkpoints(wake, build, move |id, checkpoint| {
                if commit_journal.is_some() && checkpoint.session.is_none() {
                    return Err(esp::infer::InferError::InvalidRequest(
                        "durable asks require an exact bound model session".into(),
                    ));
                }
                let mut table = commit_table.lock().unwrap();
                let index = *table
                    .by_actor_id
                    .get(&id)
                    .ok_or_else(|| esp::infer::InferError::Backend("missing durable run".into()))?;
                let weight_format = if table.session == checkpoint.session {
                    table.weight_format.clone()
                } else {
                    None
                };
                let record = &mut table.runs[index];
                let mut next = record.clone();
                if next.weight_format.is_none() {
                    next.weight_format = weight_format;
                }
                for placement in &checkpoint.placements {
                    if !next.continuations.iter().any(|visit| {
                        visit.at_token == placement.at_token
                            && visit.device == placement.device
                            && visit.reason == placement.reason
                    }) {
                        next.continuations.push(Continuation {
                            device: placement.device.clone(),
                            at_token: placement.at_token,
                            reason: placement.reason.clone(),
                            first_differing_token: None,
                            compared_token_ids_hash: None,
                        });
                    }
                }
                next.checkpoint = Some(checkpoint.clone());
                next.view.text = checkpoint.text.clone();
                next.view.session = checkpoint.session.clone();
                // A GPU provider still has to synchronize and release its
                // guard after its final token. The actor ends the run.
                next.view.state = RunStateV1::Running;
                if let Some(journal) = &commit_journal {
                    journal
                        .append(&next)
                        .map_err(esp::infer::InferError::Backend)?;
                }
                *record = next;
                table.generation += 1;
                Ok(())
            });
        let shared = Arc::new(Shared {
            table,
            journal,
            actor: Mutex::new(Some(actor)),
            bridge: Mutex::new(None),
            close_result: Mutex::new(None),
            max_tokens,
            caches: Mutex::new(Vec::new()),
            #[cfg(feature = "inference-model")]
            recall: Mutex::new(None),
            #[cfg(feature = "inference-embedding")]
            embedding: Mutex::new(None),
            #[cfg(feature = "inference-model")]
            published: Mutex::new(PublishedRecall::default()),
        });
        let bridge = std::thread::Builder::new()
            .name("inference-bridge".into())
            .spawn({
                let shared = Arc::clone(&shared);
                move || {
                    #[cfg(test)]
                    if let Some(gate) = bridge_gate {
                        let _ = gate.recv();
                    }
                    bridge(&shared, &updates)
                }
            })
            .expect("spawn the inference bridge thread");
        *shared
            .bridge
            .lock()
            .expect("the bridge slot is never poisoned") = Some(bridge);
        let lane = Self { shared };
        for id in recover {
            let name = lane
                .shared
                .table()
                .runs
                .iter()
                .find(|record| record.actor_id == id)
                .unwrap()
                .view
                .run
                .clone();
            if let Err(error) = lane.resume(&name, "restart") {
                let _ = lane.close();
                return Err(error.to_string());
            }
        }
        Ok(lane)
    }

    /// Ask a question as run `run`.
    pub fn ask(&self, request: AskRequestV1) -> Result<(), InferenceLaneError> {
        let generation = GenerationRequest {
            prompt: request.question.clone(),
            max_tokens: self.shared.max_tokens,
            ..GenerationRequest::default()
        };
        self.ask_with_generation(request, generation)
    }

    /// Sampling parameters are recorded with the ask; the owner still caps it.
    pub fn ask_with_generation(
        &self,
        request: AskRequestV1,
        generation: GenerationRequest,
    ) -> Result<(), InferenceLaneError> {
        self.ask_internal(
            request,
            generation,
            #[cfg(feature = "inference-model")]
            None,
        )
    }

    #[cfg(feature = "inference-embedding")]
    pub fn configure_embedding_recall(
        &self,
        runtime: Arc<recall_embedding::RecallEmbedding>,
    ) -> Result<(), String> {
        let mut table = self.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        if self.shared.recall.lock().unwrap().is_some()
            || !self.shared.published.lock().unwrap().owners.is_empty()
        {
            return Err("configure the embedding space before attaching retained sources".into());
        }
        *self.shared.embedding.lock().unwrap() = Some(runtime);
        table.generation += 1;
        Ok(())
    }

    /// Borrow a retained corpus from its owner and build a disposable BM25 index.
    #[cfg(feature = "inference-model")]
    pub fn configure_recall(
        &self,
        corpus: Arc<dyn recall::RecallCorpus>,
        config: recall::RecallConfig,
    ) -> Result<(), String> {
        let mut table = self.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        let index = self.shared.build_recall(corpus.clone(), config.clone())?;
        *self.shared.recall.lock().unwrap() = Some(RecallBinding {
            corpus,
            config,
            index: Some(index),
        });
        table.generation += 1;
        Ok(())
    }

    /// Explain the local retained-source ranking through the same scoped,
    /// current-version and provider-bound path used for prompt assembly.
    #[cfg(feature = "inference-model")]
    pub fn recall_rankings(
        &self,
        question: &str,
        scope: &[eidetic::models::recall::RecallSourceId],
    ) -> Result<recall::RecallRankings, String> {
        let table = self.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        let binding = self.shared.recall.lock().unwrap();
        binding
            .as_ref()
            .and_then(|b| b.index.as_ref())
            .ok_or("no retained recall index is attached")?
            .rankings(question, scope)
    }

    /// Host-side explanation of an app's current published recall index.
    /// The explicit scope must stay within one live owner. This does not
    /// create an app route or grant access to another publisher's material.
    #[cfg(feature = "inference-model")]
    pub fn published_recall_rankings(
        &self,
        owner: &str,
        question: &str,
        scope: &[eidetic::models::recall::RecallSourceId],
    ) -> Result<recall::RecallRankings, String> {
        let table = self.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        if scope.is_empty()
            || scope
                .iter()
                .any(|source| source.owner.as_deref() != Some(owner))
        {
            return Err("ranking scope must explicitly name this source owner".into());
        }
        let mut published = self.shared.published.lock().unwrap();
        let slot = published
            .owners
            .get_mut(owner)
            .ok_or("retained source owner is unavailable")?;
        if slot.binding.index.is_none() {
            slot.binding.index = Some(
                self.shared
                    .build_recall(slot.binding.corpus.clone(), slot.binding.config.clone())?,
            );
        }
        slot.binding
            .index
            .as_ref()
            .unwrap()
            .rankings(question, scope)
    }

    #[cfg(feature = "inference-model")]
    pub fn ask_recalled(
        &self,
        request: recall::RecallAskRequestV1,
    ) -> Result<(), InferenceLaneError> {
        let generation = GenerationRequest {
            max_tokens: self.shared.max_tokens,
            ..Default::default()
        };
        self.ask_internal(
            request.ask,
            generation,
            Some(RecallSelection::Local(request.scope, request.answer_format)),
        )
    }

    /// Explicit, question-anchored fact order for compound requests outside the
    /// conservative question-only notation. The authenticated host chooses it.
    #[cfg(feature = "inference-model")]
    pub fn ask_recalled_with_facts(
        &self,
        request: recall::RecallAskRequestV1,
        facts: eidetic::models::requested::RequestedFactSet,
    ) -> Result<(), InferenceLaneError> {
        if request.answer_format != recall::AnswerFormat::ClaimSupportV1 {
            return Err(InferenceLaneError::Recall(
                "requested facts require structured answers".into(),
            ));
        }
        facts
            .validate(&request.ask.question)
            .map_err(InferenceLaneError::Recall)?;
        let generation = GenerationRequest {
            max_tokens: self.shared.max_tokens,
            ..Default::default()
        };
        self.ask_internal_planned(
            request.ask,
            generation,
            Some(RecallSelection::Local(request.scope, request.answer_format)),
            Some(facts),
        )
    }

    /// Host-selected recall bounds for app-owned publications.
    #[cfg(feature = "inference-model")]
    pub fn configure_published_recall(&self, config: recall::RecallConfig) -> Result<(), String> {
        config.validate()?;
        let mut table = self.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        let mut published = self.shared.published.lock().unwrap();
        for slot in published.owners.values_mut() {
            slot.binding.config = config.clone();
            slot.binding.index = None;
        }
        published.config = config;
        table.generation += 1;
        Ok(())
    }

    fn ask_internal(
        &self,
        request: AskRequestV1,
        generation: GenerationRequest,
        #[cfg(feature = "inference-model")] recall_scope: Option<RecallSelection>,
    ) -> Result<(), InferenceLaneError> {
        self.ask_internal_planned(
            request,
            generation,
            #[cfg(feature = "inference-model")]
            recall_scope,
            #[cfg(feature = "inference-model")]
            None,
        )
    }
    fn ask_internal_planned(
        &self,
        request: AskRequestV1,
        mut generation: GenerationRequest,
        #[cfg(feature = "inference-model")] recall_scope: Option<RecallSelection>,
        #[cfg(feature = "inference-model")] requested_facts: Option<
            eidetic::models::requested::RequestedFactSet,
        >,
    ) -> Result<(), InferenceLaneError> {
        generation.prompt = request.question.clone();
        generation.max_tokens = generation.max_tokens.min(self.shared.max_tokens);
        if generation.max_tokens == 0
            || !generation.temperature.is_finite()
            || generation.temperature < 0.0
            || generation
                .top_p
                .is_some_and(|p| !p.is_finite() || p <= 0.0 || p > 1.0)
        {
            return Err(InferenceLaneError::Journal(
                "invalid token limit, temperature or top-p".into(),
            ));
        }
        if request.run.is_empty() {
            return Err(InferenceLaneError::Empty("run"));
        }
        if request.question.is_empty() {
            return Err(InferenceLaneError::Empty("question"));
        }
        let actor = self
            .shared
            .actor
            .lock()
            .expect("the actor slot is never poisoned");
        let actor = actor.as_ref().ok_or(InferenceLaneError::Closed)?;
        let mut table = self.shared.table();
        self.shared.require_actor(actor, &table)?;
        if table.locked {
            return Err(InferenceLaneError::Locked);
        }
        if table
            .runs
            .iter()
            .any(|record| record.view.run == request.run)
        {
            return Err(InferenceLaneError::DuplicateRun(request.run));
        }
        #[allow(unused_mut)]
        let mut recall_failure: Option<String> = None;
        #[cfg(feature = "inference-model")]
        let recall_evidence = if let Some(scope) = recall_scope {
            if table.session.is_none() || self.shared.journal.is_none() {
                return Err(InferenceLaneError::Recall("recall requires a durable exact model session; stub inference cannot answer retained material".into()));
            }
            let mut local = self.shared.recall.lock().unwrap();
            let mut published = self.shared.published.lock().unwrap();
            let (binding, scope, owner, answer_format) = match scope {
                RecallSelection::Local(scope, answer_format) => (
                    local.as_mut().ok_or_else(|| {
                        InferenceLaneError::Recall("no retained corpus is attached".into())
                    })?,
                    scope,
                    None,
                    answer_format,
                ),
                RecallSelection::Published {
                    owner,
                    publisher,
                    answer_format,
                    scope,
                } => {
                    let slot = published.owners.get_mut(&owner).ok_or_else(|| {
                        InferenceLaneError::Recall("retained source owner is unavailable".into())
                    })?;
                    if slot.publisher != publisher {
                        return Err(InferenceLaneError::Recall(
                            "this session does not own the retained publication".into(),
                        ));
                    }
                    if scope
                        .iter()
                        .any(|source| source.owner.as_deref() != Some(owner.as_str()))
                    {
                        return Err(InferenceLaneError::Recall(
                            "scope names another source owner".into(),
                        ));
                    }
                    (&mut slot.binding, scope, Some(owner), answer_format)
                },
            };
            if binding.index.is_none() {
                binding.index = Some(
                    self.shared
                        .build_recall(binding.corpus.clone(), binding.config.clone())
                        .map_err(InferenceLaneError::Recall)?,
                );
            }
            let (mut evidence, mut context) = binding
                .index
                .as_ref()
                .unwrap()
                .prepare(&request.question, &scope)
                .map_err(InferenceLaneError::Recall)?;
            evidence.owner = owner;
            evidence.answer_format = answer_format;
            if answer_format == recall::AnswerFormat::ClaimSupportV1 {
                let (contract, prompt, supplied_context) = match requested_facts.as_ref() {
                    Some(facts) => {
                        answer_contract::prepare_with_facts(&request.question, &evidence, facts)
                    },
                    None => answer_contract::prepare(&request.question, &evidence),
                }
                .map_err(InferenceLaneError::Recall)?;
                evidence.answer_contract = Some(contract);
                generation.output_grammar = answer_contract::grammar(
                    &evidence,
                    &evidence.answer_contract.as_ref().unwrap().method,
                )
                .map_err(InferenceLaneError::Recall)?;
                generation.prompt = prompt;
                context = supplied_context;
            }
            if evidence.passages.is_empty()
                && answer_format == recall::AnswerFormat::ClaimSupportV1
                && evidence.stale.is_empty()
            {
                generation.context = context;
            } else if evidence.passages.is_empty() {
                recall_failure = Some(if evidence.stale.is_empty() {
                    "No current retained passages match this question and scope."
                } else {
                    "Retained sources changed or became unavailable. Refresh the retained corpus before asking again."
                }.into());
            } else {
                generation.context = context;
            }
            Some(evidence)
        } else {
            None
        };
        let actor_id = table.next_actor_id;
        table.next_actor_id += 1;
        let index = table.runs.len();
        let session = table.session.clone();
        let weight_format = table.weight_format.clone();
        table.runs.push(RunRecord {
            weight_format,
            view: RunV1 {
                run: request.run,
                question: request.question,
                state: if recall_failure.is_some() {
                    RunStateV1::Failed
                } else {
                    RunStateV1::Queued
                },
                text: String::new(),
                error: recall_failure.clone(),
                session,
                tokens: 0,
                continuations: Vec::new(),
                #[cfg(feature = "inference-model")]
                recall: recall_evidence,
                #[cfg(all(feature = "inference-wire", not(feature = "inference-model")))]
                recall: None,
            },
            actor_id,
            request: generation.clone(),
            checkpoint: None,
            continuations: self
                .shared
                .journal
                .as_ref()
                .map(|journal| {
                    vec![Continuation {
                        device: journal.device().into(),
                        at_token: 0,
                        reason: "ask".into(),
                        first_differing_token: None,
                        compared_token_ids_hash: None,
                    }]
                })
                .unwrap_or_default(),
            pause_reason: None,
            executing: false,
        });
        if let Some(journal) = &self.shared.journal {
            if let Err(error) = journal.append(&table.runs[index]) {
                table.runs.pop();
                return Err(InferenceLaneError::Journal(error));
            }
        }
        table.by_actor_id.insert(actor_id, index);
        if recall_failure.is_some() {
            table.observe(actor_id, InferenceObservation::RunFailed);
            table.generation += 1;
            return Ok(());
        }
        let command = if self.shared.journal.is_some() {
            InferCommand::Checkpointed {
                id: actor_id,
                request: generation,
                resume: None,
            }
        } else {
            InferCommand::Generate {
                id: actor_id,
                request: generation,
            }
        };
        if !actor.command(command) {
            return Err(InferenceLaneError::Closed);
        }
        table.observe(actor_id, InferenceObservation::Queued);
        table.generation += 1;
        Ok(())
    }

    /// Cancel run `run`. Cancelling a run that has already ended is accepted
    /// and changes nothing.
    pub fn cancel(&self, request: &CancelRequestV1) -> Result<(), InferenceLaneError> {
        let actor = self
            .shared
            .actor
            .lock()
            .expect("the actor slot is never poisoned");
        let actor = actor.as_ref().ok_or(InferenceLaneError::Closed)?;
        let mut table = self.shared.table();
        self.shared.require_actor(actor, &table)?;
        if table.locked {
            return Err(InferenceLaneError::Locked);
        }
        let record = table
            .runs
            .iter_mut()
            .find(|record| record.view.run == request.run)
            .ok_or_else(|| InferenceLaneError::UnknownRun(request.run.clone()))?;
        if record.view.state == RunStateV1::Paused {
            let mut next = record.clone();
            next.view.state = RunStateV1::Cancelled;
            if let Some(journal) = &self.shared.journal {
                journal.append(&next).map_err(InferenceLaneError::Journal)?;
            }
            *record = next;
            let id = record.actor_id;
            table.observe(id, InferenceObservation::RunCancelled);
            table.generation += 1;
        } else if !record.view.state.is_terminal() {
            if !actor.command(InferCommand::Cancel {
                id: record.actor_id,
            }) {
                return Err(InferenceLaneError::Closed);
            }
            let id = record.actor_id;
            table.observe(id, InferenceObservation::CancelRequested);
        }
        Ok(())
    }

    /// Pause a run without cancelling its retained answer.
    pub fn pause(&self, run: &str, reason: &str) -> Result<(), InferenceLaneError> {
        let actor = self.shared.actor.lock().unwrap();
        let actor = actor.as_ref().ok_or(InferenceLaneError::Closed)?;
        let mut table = self.shared.table();
        self.shared.require_actor(actor, &table)?;
        if table.locked && reason != "lock" && reason != "shutdown" {
            return Err(InferenceLaneError::Locked);
        }
        let record = table
            .runs
            .iter_mut()
            .find(|record| record.view.run == run)
            .ok_or_else(|| InferenceLaneError::UnknownRun(run.into()))?;
        if !record.view.state.is_terminal() {
            record.pause_reason = Some(reason.into());
            if !actor.command(InferCommand::Pause {
                id: record.actor_id,
            }) {
                return Err(InferenceLaneError::Closed);
            }
        }
        Ok(())
    }

    /// Continue a paused run from its exact retained token and draw position.
    pub fn resume(&self, run: &str, reason: &str) -> Result<(), InferenceLaneError> {
        let actor = self.shared.actor.lock().unwrap();
        let actor = actor.as_ref().ok_or(InferenceLaneError::Closed)?;
        let mut table = self.shared.table();
        self.shared.require_actor(actor, &table)?;
        if table.locked {
            return Err(InferenceLaneError::Locked);
        }
        let record = table
            .runs
            .iter_mut()
            .find(|record| record.view.run == run)
            .ok_or_else(|| InferenceLaneError::UnknownRun(run.into()))?;
        if record.view.state != RunStateV1::Paused {
            return Err(InferenceLaneError::Journal(
                "only paused runs can resume".into(),
            ));
        }
        let journal = self.shared.journal.as_ref().ok_or_else(|| {
            InferenceLaneError::Journal("this lane has no durable token journal".into())
        })?;
        let mut next = record.clone();
        next.continuations.push(Continuation {
            device: journal.device().into(),
            at_token: record
                .checkpoint
                .as_ref()
                .map_or(0, |cp| cp.token_ids.len()),
            reason: reason.into(),
            first_differing_token: None,
            compared_token_ids_hash: None,
        });
        let previous = record.clone();
        next.view.state = RunStateV1::Queued;
        next.pause_reason = None;
        journal.append(&next).map_err(InferenceLaneError::Journal)?;
        *record = next;
        if !actor.command(InferCommand::Checkpointed {
            id: record.actor_id,
            request: record.request.clone(),
            resume: record.checkpoint.clone(),
        }) {
            // Admission can fail after the preflight check. Keep the retained
            // pause and its independent reason instead of a queued continuation.
            let journal_error = journal.append(&previous).err();
            *record = previous;
            if let Some(error) = journal_error {
                table.journal_failures.push(error.clone());
                return Err(InferenceLaneError::Journal(error));
            }
            return Err(InferenceLaneError::Closed);
        }
        table.generation += 1;
        Ok(())
    }

    /// Refuse route access immediately, pause work, and wait for the durable
    /// boundary. Unlock resumes only runs paused by this lock.
    pub fn set_locked(&self, locked: bool) -> Result<(), String> {
        let actor = self.shared.actor.lock().unwrap();
        let mut table = self.shared.table();
        table.locked = locked;
        if table.disconnected || actor.as_ref().is_none_or(|actor| actor.is_finished()) {
            // The host's explicit lock bit remains authoritative. An exited
            // actor alone says nothing about buffered updates or durable saves.
            if !table.disconnected {
                return Err(
                    "inference actor is unavailable; durable completion is unconfirmed".into(),
                );
            }
            if !table.journal_failures.is_empty() {
                return Err(format!(
                    "inference journal failures: {}",
                    table.journal_failures.join("; ")
                ));
            }
            return if !locked
                && table.runs.iter().any(|record| {
                    record.view.state == RunStateV1::Paused
                        && record.pause_reason.as_deref() == Some("lock")
                }) {
                Err("inference unlocked; unavailable actor cannot resume paused runs".into())
            } else {
                Ok(())
            };
        }
        drop(table);
        drop(actor);
        let names: Vec<_> = self
            .shared
            .table()
            .runs
            .iter()
            .filter(|record| {
                !record.view.state.is_terminal()
                    && (if locked {
                        record.executing
                            || matches!(record.view.state, RunStateV1::Running | RunStateV1::Queued)
                    } else {
                        record.pause_reason.as_deref() == Some("lock")
                    })
            })
            .map(|record| record.view.run.clone())
            .collect();
        if locked {
            for name in &names {
                self.pause(name, "lock")
                    .map_err(|error| error.to_string())?;
            }
            loop {
                if self
                    .shared
                    .actor
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_none_or(|actor| actor.is_finished())
                {
                    return Err("inference actor disconnected while locking".into());
                }
                if self.shared.table().runs.iter().all(|record| {
                    !record.executing
                        && !matches!(record.view.state, RunStateV1::Running | RunStateV1::Queued)
                }) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        } else if self
            .shared
            .journal
            .as_ref()
            .is_none_or(|journal| journal.execution_permitted())
        {
            for name in names {
                self.resume(&name, "unlock")
                    .map_err(|error| error.to_string())?;
            }
        }
        let table = self.shared.table();
        if !table.journal_failures.is_empty() {
            return Err(format!(
                "inference journal failures: {}",
                table.journal_failures.join("; ")
            ));
        }
        Ok(())
    }

    /// Drop materialized persona data after a lock boundary. Actor IDs remain
    /// monotonic; an unlock rehydrates records through the unlocked journal.
    #[cfg(feature = "inference-model")]
    pub fn forget_locked_materialization(&self) {
        let mut table = self.shared.table();
        assert!(table.locked);
        table.runs.clear();
        table.by_actor_id.clear();
        table.generation += 1;
        for cache in self
            .shared
            .caches
            .lock()
            .unwrap()
            .iter()
            .filter_map(|cache| cache.upgrade())
        {
            if let Some((mut card, mut runs)) = cache.lock().unwrap().take() {
                use zeroize::Zeroize;
                card.zeroize();
                runs.zeroize();
            }
        }
        #[cfg(feature = "inference-model")]
        if let Some(binding) = self.shared.recall.lock().unwrap().as_mut() {
            binding.index = None;
        }
        #[cfg(feature = "inference-model")]
        self.shared.published.lock().unwrap().owners.clear();
        #[cfg(feature = "inference-model")]
        {
            table.seen_recall_revision = None;
        }
    }

    fn refresh_remote(&self) -> Result<(), String> {
        if self.is_locked() {
            return Ok(());
        }
        let Some(journal) = &self.shared.journal else {
            return Ok(());
        };
        let revision = journal.remote_revision();
        if self.shared.table().seen_remote_revision == revision {
            return Ok(());
        }
        match self.refresh_retained(false) {
            Err(error) if error.contains("missing parent") => return Ok(()),
            result => result?,
        }
        self.shared.table().seen_remote_revision = revision;
        Ok(())
    }

    /// Rehydrate the current log heads, retaining active local execution IDs.
    pub fn refresh_retained_runs(&self) -> Result<(), String> {
        self.refresh_retained(true)
    }

    fn refresh_retained(&self, allow_locked: bool) -> Result<(), String> {
        let Some(journal) = &self.shared.journal else {
            return Ok(());
        };
        let records = journal.load()?;
        let mut table = self.shared.table();
        if table.locked && !allow_locked {
            return Ok(());
        }
        for mut record in records {
            if let Some(existing) = table
                .runs
                .iter_mut()
                .find(|existing| existing.view.run == record.view.run)
            {
                if matches!(
                    existing.view.state,
                    RunStateV1::Running | RunStateV1::Queued
                ) {
                    continue;
                }
                record.actor_id = existing.actor_id;
                if !record.view.state.is_terminal() {
                    record.view.state = RunStateV1::Paused;
                }
                *existing = record;
            } else {
                record.actor_id = table.next_actor_id;
                table.next_actor_id += 1;
                if !record.view.state.is_terminal() {
                    record.view.state = RunStateV1::Paused;
                }
                let index = table.runs.len();
                table.by_actor_id.insert(record.actor_id, index);
                table.runs.push(record);
            }
        }
        table.generation += 1;
        Ok(())
    }

    /// Whether the route currently refuses persona-data access.
    pub fn is_locked(&self) -> bool {
        self.shared.table().locked
    }

    /// The run table now.
    pub fn runs(&self) -> InferenceRunsV1 {
        self.shared.view(&mut self.shared.table())
    }

    /// The table's change counter.
    pub fn generation(&self) -> u64 {
        #[allow(unused_mut)]
        let mut table = self.shared.table();
        #[cfg(feature = "inference-model")]
        self.shared.observe_recall(&mut table);
        table.generation
    }

    /// Cancel every unfinished run, stop the actor and wait for both of the
    /// lane's threads. Idempotent.
    pub fn close(&self) -> Result<(), String> {
        let mut closed = self
            .shared
            .close_result
            .lock()
            .expect("close result mutex poisoned");
        if let Some(result) = &*closed {
            return result.clone();
        }
        let mut failures = Vec::new();
        self.shared.table().closing = true;
        let actor = self
            .shared
            .actor
            .lock()
            .expect("the actor slot is never poisoned")
            .take();
        if let Some(actor) = actor {
            // Dropping the handle alone would let queued runs drain first.
            let open: Vec<u64> = self
                .shared
                .table()
                .runs
                .iter()
                .filter(|record| {
                    matches!(record.view.state, RunStateV1::Running | RunStateV1::Queued)
                })
                .map(|record| record.actor_id)
                .collect();
            for id in open {
                if self.shared.journal.is_some() {
                    let mut table = self.shared.table();
                    let index = table.by_actor_id[&id];
                    let previous_reason = table.runs[index].pause_reason.clone();
                    table.runs[index].pause_reason = Some("shutdown".into());
                    if !actor.command(InferCommand::Pause { id }) {
                        table.runs[index].pause_reason = previous_reason;
                    }
                } else {
                    actor.command(InferCommand::Cancel { id });
                }
            }
            if let Err(error) = actor.join() {
                failures.push(error.to_string());
            }
        }
        let bridge = self
            .shared
            .bridge
            .lock()
            .expect("the bridge slot is never poisoned")
            .take();
        if let Some(bridge) = bridge {
            if bridge.join().is_err() {
                failures.push("the inference bridge thread panicked".into());
            }
        }
        let mut table = self.shared.table();
        // Also reconcile after a bridge panic: both threads have been joined,
        // so buffered update processing can no longer race this finalization.
        self.shared.record_disconnect(&mut table);
        if let Some(error) = &table.actor_failure {
            failures.push(error.clone());
        }
        failures.extend(table.journal_failures.iter().cloned());
        drop(table);
        let result = if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        };
        *closed = Some(result.clone());
        result
    }

    /// The route endpoint for one admitted session.
    pub fn endpoint(&self, session: ProjectionSession) -> InferenceAskEndpoint {
        let current = Arc::new(Mutex::new(None));
        self.shared
            .caches
            .lock()
            .unwrap()
            .retain(|cache| cache.strong_count() > 0);
        self.shared
            .caches
            .lock()
            .unwrap()
            .push(Arc::downgrade(&current));
        InferenceAskEndpoint {
            lane: self.clone(),
            session,
            noticed: None,
            current,
            #[cfg(feature = "inference-model")]
            publication: None,
        }
    }
}

/// Fold updates until the actor is gone and its channel closes.
fn bridge(shared: &Shared, updates: &Receiver<InferUpdate>) {
    while let Ok(update) = updates.recv() {
        let observed = observations::worker_update(&update);
        let changed = match &update {
            InferUpdate::Paused { id }
            | InferUpdate::Cancelled { id }
            | InferUpdate::Failed { id, .. }
            | InferUpdate::Finished { id, .. } => Some(*id),
            _ => None,
        };
        let mut table = shared.table();
        if let Some((id, phase)) = observed {
            table.observe(id, phase);
        }
        let accepted = table.apply(update);
        if let (Some(id), Some(journal), true) = (changed, &shared.journal, accepted) {
            if let Some(index) = table.by_actor_id.get(&id).copied() {
                if let Err(error) = journal.append(&table.runs[index]) {
                    table
                        .journal_failures
                        .push(format!("checkpoint commit failed: {error}"));
                    table.runs[index].view.state = RunStateV1::Failed;
                    table.runs[index].view.error =
                        Some(format!("checkpoint commit failed: {error}"));
                }
            }
        }
        if let Some((id, _)) = observed {
            if accepted {
                if let Some(phase) = table
                    .by_actor_id
                    .get(&id)
                    .and_then(|&index| observations::accepted_state(&table.runs[index].view.state))
                {
                    table.observe(id, phase);
                }
            } else {
                table.observe(id, InferenceObservation::UpdateDiscarded);
            }
        }
    }
    shared.record_disconnect(&mut shared.table());
}

/// The `inference.ask` endpoint for one session.
pub struct InferenceAskEndpoint {
    lane: InferenceLane,
    session: ProjectionSession,
    noticed: Option<u64>,
    /// The bytes the last snapshot named, so its resources stay servable
    /// while the table moves on.
    current: Arc<Mutex<Option<(Vec<u8>, Vec<u8>)>>>,
    #[cfg(feature = "inference-model")]
    publication: Option<String>,
}

const CARD_LABEL: &str = "Inference runs";

fn score() -> Score {
    Score::new(Arrangement::Spiral(Default::default()))
}

impl InferenceAskEndpoint {
    fn own_session(&self, session: &ProjectionSession, what: &str) -> Result<(), String> {
        if self.lane.is_locked() {
            return Err("inference.ask is locked".into());
        }
        if *session == self.session {
            Ok(())
        } else {
            Err(format!("inference.ask {what} names another session"))
        }
    }

    #[cfg(feature = "inference-model")]
    fn publish(
        &mut self,
        request: recall::PublishCorpusRequestV1,
    ) -> Result<(), InferenceLaneError> {
        if request.owner.trim().is_empty()
            || request.owner.len() > 256
            || self
                .publication
                .as_ref()
                .is_some_and(|owner| owner != &request.owner)
        {
            return Err(InferenceLaneError::Recall(
                "publication must name this session's stable source owner".into(),
            ));
        }
        if request
            .sources
            .iter()
            .any(|entry| entry.source.owner.as_ref() != Some(&request.owner))
        {
            return Err(InferenceLaneError::Recall(
                "source identity names another owner".into(),
            ));
        }
        for entry in &request.sources {
            if entry.source.kind == eidetic::models::recall::RecallSourceKind::PageText
                && entry.version != eidetic::Hash::of(entry.text.as_bytes()).to_string()
            {
                return Err(InferenceLaneError::Recall(
                    "retained page-text bytes do not match their version".into(),
                ));
            }
        }
        let mut table = self.lane.shared.table();
        if table.locked {
            return Err(InferenceLaneError::Locked);
        }
        let mut published = self.lane.shared.published.lock().unwrap();
        if published
            .owners
            .get(&request.owner)
            .is_some_and(|slot| slot.publisher != self.session)
        {
            return Err(InferenceLaneError::Recall(
                "this source owner already has a connected publisher".into(),
            ));
        }
        let corpus: Arc<dyn recall::RecallCorpus> =
            Arc::new(recall::PublishedCorpus(request.sources));
        let config = published.config.clone();
        let index = self
            .lane
            .shared
            .build_recall(corpus.clone(), config.clone())
            .map_err(InferenceLaneError::Recall)?;
        self.publication = Some(request.owner.clone());
        published.owners.insert(
            request.owner,
            PublishedBinding {
                publisher: self.session.clone(),
                binding: RecallBinding {
                    corpus,
                    config,
                    index: Some(index),
                },
            },
        );
        table.generation += 1;
        Ok(())
    }

    fn card_snapshot(&self, generation: u64, card_bytes: &[u8]) -> ProjectionSnapshot {
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new(self.session.0.as_str(), "runs"));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: Size2::new(1.0, 1.0),
            },
            representation: Representation::Card,
            layer: 0,
            visible: false,
            hit: None,
            channels: Vec::new(),
        });
        let key = PresentationKey(INFERENCE_RUNS_SCHEMA.into());
        let mut presentation = PresentationManifest::default();
        presentation.bindings.push(PresentationBinding {
            instance: InstanceId(0),
            key: key.clone(),
        });
        presentation.offers.insert(
            key,
            vec![PresentationOffer {
                codec: PresentationCodec::PortableCardV1,
                resource: ContentHash::of(card_bytes),
                byte_size: card_bytes.len() as u64,
                requires: PresentationCapability::PortableCard,
                semantics: PresentationSemantics {
                    label: CARD_LABEL.into(),
                    role: SemanticRole::Graphic,
                    bounds: BoundsRelationship::FitWithinFootprint,
                    actions: Vec::new(),
                },
            }],
        );
        ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(generation), scene)
                .expect("a one-item scene is dense"),
            presentation,
            cache_policy: CachePolicy::default(),
        }
    }
}

#[cfg(feature = "inference-model")]
impl Drop for InferenceAskEndpoint {
    fn drop(&mut self) {
        if let Some(owner) = &self.publication {
            let mut table = self.lane.shared.table();
            let mut published = self.lane.shared.published.lock().unwrap();
            if published
                .owners
                .get(owner)
                .is_some_and(|slot| slot.publisher == self.session)
            {
                published.owners.remove(owner);
                table.generation += 1;
            }
        }
    }
}

impl ProjectionCatalog for InferenceAskEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Resident inference".into(),
            projections: vec![ProjectionOffer {
                label: CARD_LABEL.into(),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session: self.session.clone(),
                    score: score(),
                },
            }],
        }
    }
}

impl ProjectionSource for InferenceAskEndpoint {
    type Error = String;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        self.own_session(&request.session, "snapshot")?;
        self.lane.refresh_remote()?;
        let mut table = self.lane.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        let (runs, generation) = (self.lane.shared.view(&mut table), table.generation);
        let runs_bytes = serde_json::to_vec(&runs).map_err(|error| error.to_string())?;
        let finished = runs
            .runs
            .iter()
            .filter(|run| run.state.is_terminal())
            .count();
        let value = |label: &str, value: String| CardValueV1 {
            label: label.into(),
            value,
        };
        let card = PortableCardV1 {
            title: CARD_LABEL.into(),
            values: vec![
                value("model", runs.model.clone().unwrap_or_default()),
                value("runs", runs.runs.len().to_string()),
                value("ended", finished.to_string()),
            ],
            badges: Vec::new(),
            media: vec![ContentHash::of(&runs_bytes)],
        };
        let card_bytes = serde_json::to_vec(&card).map_err(|error| error.to_string())?;
        let snapshot = self.card_snapshot(generation, &card_bytes);
        self.noticed = Some(generation);
        *self.current.lock().unwrap() = Some((card_bytes, runs_bytes));
        Ok(snapshot)
    }
}

impl PresentationSource for InferenceAskEndpoint {
    type Error = String;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        self.own_session(&request.session, "resource")?;
        let table = self.lane.shared.table();
        if table.locked {
            return Err("inference.ask is locked".into());
        }
        let current = self.current.lock().unwrap();
        let Some((card, runs)) = current.as_ref() else {
            return Err("request an inference.ask snapshot first".into());
        };
        let bytes = [card, runs]
            .into_iter()
            .find(|bytes| ContentHash::of(bytes) == request.resource)
            .ok_or("that resource is not in the current inference.ask snapshot")?
            .clone();
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}

impl IntentSink for InferenceAskEndpoint {
    type Error = String;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        self.own_session(&intent.session, "intent")?;
        let rejected = |reason: String| Ok(IntentResult::Rejected { reason });
        let outcome = match intent.intent.as_str() {
            #[cfg(feature = "inference-model")]
            recall::PUBLISH_CORPUS_INTENT => {
                if intent.payload.len() > recall::MAX_PUBLISHED_CORPUS_BYTES {
                    return rejected("retained publication exceeds the inline transport bound; choose a smaller corpus".into());
                }
                match serde_json::from_slice::<recall::PublishCorpusRequestV1>(&intent.payload) {
                    Ok(request) => self.publish(request),
                    Err(error) => return rejected(error.to_string()),
                }
            },
            #[cfg(feature = "inference-model")]
            recall::ASK_PUBLISHED_INTENT => {
                match serde_json::from_slice::<recall::PublishedAskRequestV1>(&intent.payload) {
                    Ok(request) => {
                        let generation = GenerationRequest {
                            max_tokens: self.lane.shared.max_tokens,
                            ..Default::default()
                        };
                        self.lane.ask_internal(
                            request.ask,
                            generation,
                            Some(RecallSelection::Published {
                                owner: request.owner,
                                publisher: self.session.clone(),
                                answer_format: request.answer_format,
                                scope: request.scope,
                            }),
                        )
                    },
                    Err(error) => return rejected(error.to_string()),
                }
            },
            #[cfg(feature = "inference-model")]
            recall::RECALL_ASK_INTENT => {
                match serde_json::from_slice::<recall::RecallAskRequestV1>(&intent.payload) {
                    Ok(ask) => self.lane.ask_recalled(ask),
                    Err(error) => return rejected(error.to_string()),
                }
            },
            ASK_INTENT => match serde_json::from_slice::<AskRequestV1>(&intent.payload) {
                Ok(ask) => self.lane.ask(ask),
                Err(error) => return rejected(format!("{ASK_INTENT} payload: {error}")),
            },
            ASK_SAMPLED_INTENT => {
                match serde_json::from_slice::<SampledAskRequestV1>(&intent.payload) {
                    Ok(sampled) => self.lane.ask_with_generation(
                        sampled.ask,
                        GenerationRequest {
                            max_tokens: sampled.max_tokens,
                            temperature: sampled.temperature,
                            top_p: sampled.top_p,
                            seed: sampled.seed,
                            ..Default::default()
                        },
                    ),
                    Err(error) => return rejected(error.to_string()),
                }
            },
            PAUSE_INTENT => match serde_json::from_slice::<CancelRequestV1>(&intent.payload) {
                Ok(run) => self.lane.pause(&run.run, "user"),
                Err(error) => return rejected(error.to_string()),
            },
            RESUME_INTENT => match serde_json::from_slice::<CancelRequestV1>(&intent.payload) {
                Ok(run) => self.lane.resume(&run.run, "user"),
                Err(error) => return rejected(error.to_string()),
            },
            CANCEL_INTENT => match serde_json::from_slice::<CancelRequestV1>(&intent.payload) {
                Ok(cancel) => self.lane.cancel(&cancel),
                Err(error) => return rejected(format!("{CANCEL_INTENT} payload: {error}")),
            },
            other => {
                return rejected(format!(
                    "inference.ask takes {ASK_INTENT} or {CANCEL_INTENT}, not {other:?}"
                ));
            },
        };
        match outcome {
            Ok(()) => Ok(IntentResult::Accepted),
            Err(error) => rejected(error.to_string()),
        }
    }
}

impl ProjectionNoticeSource for InferenceAskEndpoint {
    type Error = String;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
        if self.lane.is_locked() {
            self.current.lock().unwrap().take();
            return Err("inference.ask is locked".into());
        }
        self.lane.refresh_remote()?;
        let generation = self.lane.generation();
        if self.noticed == Some(generation) {
            return Ok(None);
        }
        self.noticed = Some(generation);
        Ok(Some(CarrierNotice {
            session: self.session.clone(),
            epoch: SceneEpoch(1),
            revision: Revision(generation),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;
    use std::sync::mpsc::{SyncSender, sync_channel};
    use std::time::Instant;

    use esp::infer::{InferError, ModelCapability, StubInferenceProvider};

    use super::*;

    fn session() -> ProjectionSession {
        ProjectionSession("test-session".into())
    }

    fn ask(run: &str, question: &str) -> AskRequestV1 {
        AskRequestV1 {
            run: run.into(),
            question: question.into(),
        }
    }

    fn run<'a>(runs: &'a InferenceRunsV1, name: &str) -> &'a RunV1 {
        runs.runs
            .iter()
            .find(|run| run.run == name)
            .expect("the run is in the table")
    }

    /// Poll until `done` holds or a generous deadline passes.
    fn wait_for(lane: &InferenceLane, done: impl Fn(&InferenceRunsV1) -> bool) -> InferenceRunsV1 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let runs = lane.runs();
            if done(&runs) {
                return runs;
            }
            assert!(Instant::now() < deadline, "timed out; table: {runs:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Emits one fragment each time the test lets it, so a cancel can land
    /// mid-stream deterministically.
    struct GatedProvider {
        capability: ModelCapability,
        gate: Mutex<Receiver<()>>,
        emitted: SyncSender<usize>,
    }

    impl InferenceProvider for GatedProvider {
        fn capability(&self) -> &ModelCapability {
            &self.capability
        }

        fn generate_streaming(
            &self,
            _request: &GenerationRequest,
            on_token: &mut dyn FnMut(&str) -> ControlFlow<()>,
        ) -> Result<String, InferError> {
            let gate = self.gate.lock().unwrap();
            let mut out = String::new();
            for i in 0..100 {
                if gate.recv().is_err() {
                    break;
                }
                let fragment = format!("t{i} ");
                if on_token(&fragment).is_break() {
                    break;
                }
                out.push_str(&fragment);
                let _ = self.emitted.send(i);
            }
            Ok(out)
        }
    }

    #[test]
    fn retained_asks_do_not_auto_execute_without_current_authority() {
        struct Historical;
        impl RunJournal for Historical {
            fn device(&self) -> &str {
                "retired"
            }
            fn execution_permitted(&self) -> bool {
                false
            }
            fn load(&self) -> Result<Vec<RunRecord>, String> {
                Ok(vec![RunRecord {
                    weight_format: None,
                    view: RunV1 {
                        run: "private".into(),
                        question: "retained question".into(),
                        state: RunStateV1::Paused,
                        text: "prefix".into(),
                        error: None,
                        session: None,
                        tokens: 0,
                        #[cfg(feature = "inference-wire")]
                        recall: None,
                        continuations: Vec::new(),
                    },
                    actor_id: 0,
                    request: GenerationRequest::default(),
                    checkpoint: None,
                    continuations: vec![Continuation {
                        device: "retired".into(),
                        at_token: 0,
                        reason: "ask".into(),
                        first_differing_token: None,
                        compared_token_ids_hash: None,
                    }],
                    pause_reason: Some("lock".into()),
                    executing: false,
                }])
            }
            fn append(&self, _: &RunRecord) -> Result<(), String> {
                Err("not currently keyed".into())
            }
        }
        let lane = InferenceLane::try_open_with_journal(
            8,
            || Ok(Box::new(StubInferenceProvider::new())),
            Some(Arc::new(Historical)),
        )
        .unwrap();
        assert_eq!(lane.runs().runs[0].state, RunStateV1::Paused);
        lane.set_locked(true).unwrap();
        lane.set_locked(false).unwrap();
        assert_eq!(lane.runs().runs[0].state, RunStateV1::Paused);
        lane.close().unwrap();
    }

    #[test]
    fn a_stub_ask_streams_to_a_finished_answer() {
        let lane = InferenceLane::open(|| {
            Box::new(StubInferenceProvider::new().with_response("why?", "because it is so"))
        });
        lane.ask(ask("a", "why?")).unwrap();
        let runs = wait_for(&lane, |runs| {
            runs.runs.first().is_some_and(|run| run.state.is_terminal())
        });
        let a = run(&runs, "a");
        assert_eq!(a.state, RunStateV1::Finished);
        assert_eq!(a.text, "because it is so");
        assert_eq!(runs.model.as_deref(), Some("stub/echo"));
        lane.close().unwrap();
    }

    #[test]
    fn a_load_failure_is_visible_and_each_ask_fails() {
        let lane = InferenceLane::try_open(|| {
            Err(esp::infer::InferError::InvalidConfig(
                "missing template".into(),
            ))
        });
        lane.ask(ask("a", "why?")).unwrap();
        let runs = wait_for(&lane, |runs| run(runs, "a").state.is_terminal());
        assert!(runs.model.is_none());
        assert!(runs.session.is_none());
        assert!(
            runs.load_error
                .as_deref()
                .unwrap()
                .contains("missing template")
        );
        assert_eq!(run(&runs, "a").state, RunStateV1::Failed);
        assert!(run(&runs, "a").text.is_empty());
        lane.close().unwrap();
    }

    #[test]
    fn close_cancels_an_ask_waiting_for_a_gpu_worker_to_stop() {
        struct WaitingProvider {
            stub: StubInferenceProvider,
            access: crate::gpu::GpuAccess,
        }
        impl InferenceProvider for WaitingProvider {
            fn capability(&self) -> &esp::infer::ModelCapability {
                self.stub.capability()
            }
            fn generate_streaming(
                &self,
                _: &GenerationRequest,
                _: &mut dyn FnMut(&str) -> std::ops::ControlFlow<()>,
            ) -> Result<String, esp::infer::InferError> {
                unreachable!("actor must supply cooperative cancellation")
            }
            fn generate_streaming_controlled(
                &self,
                _: &GenerationRequest,
                _: &mut dyn FnMut(&str) -> std::ops::ControlFlow<()>,
                cancelled: &mut dyn FnMut() -> bool,
            ) -> Result<String, esp::infer::InferError> {
                let request = self.access.request();
                assert!(
                    request.acquire(cancelled).is_none(),
                    "the worker still owns the GPU"
                );
                Ok(String::new())
            }
        }
        let access = crate::gpu::GpuAccess::default();
        let worker = access.try_background().unwrap();
        let provider_access = access.clone();
        let lane = InferenceLane::open(move || {
            Box::new(WaitingProvider {
                stub: StubInferenceProvider::new(),
                access: provider_access,
            })
        });
        lane.ask(ask("waiting", "why?")).unwrap();
        for _ in 0..100 {
            if access.foreground_requested() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(access.foreground_requested());
        lane.close().unwrap();
        assert_eq!(run(&lane.runs(), "waiting").state, RunStateV1::Cancelled);
        assert!(!access.foreground_requested());
        assert!(
            access.try_background().is_none(),
            "close cannot release another worker's guard"
        );
        drop(worker);
        assert!(access.try_background().is_some());
    }

    #[test]
    fn a_cancel_mid_stream_keeps_partial_text_and_nothing_after() {
        let (open_gate, gate) = sync_channel::<()>(0);
        let (emitted, fragments) = sync_channel::<usize>(100);
        let lane = InferenceLane::open(move || {
            Box::new(GatedProvider {
                capability: StubInferenceProvider::new().capability().clone(),
                gate: Mutex::new(gate),
                emitted,
            })
        });
        lane.ask(ask("a", "go")).unwrap();
        for expected in 0..3 {
            open_gate.send(()).unwrap();
            assert_eq!(fragments.recv().unwrap(), expected);
        }
        let partial = wait_for(&lane, |runs| run(runs, "a").text == "t0 t1 t2 ");
        assert_eq!(run(&partial, "a").state, RunStateV1::Running);

        lane.cancel(&CancelRequestV1 { run: "a".into() }).unwrap();
        // The provider asks for one more fragment; the actor sees the cancel
        // there and drops it.
        open_gate.send(()).unwrap();
        let runs = wait_for(&lane, |runs| run(runs, "a").state.is_terminal());
        let a = run(&runs, "a");
        assert_eq!(a.state, RunStateV1::Cancelled);
        assert_eq!(a.text, "t0 t1 t2 ");

        // Nothing reaches the run after its cancel.
        drop(open_gate);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(run(&lane.runs(), "a").text, "t0 t1 t2 ");
        lane.close().unwrap();
    }

    #[test]
    fn duplicate_and_unknown_runs_are_refused() {
        let lane = InferenceLane::open(|| Box::new(StubInferenceProvider::new()));
        lane.ask(ask("a", "one")).unwrap();
        assert_eq!(
            lane.ask(ask("a", "two")),
            Err(InferenceLaneError::DuplicateRun("a".into()))
        );
        assert_eq!(
            lane.cancel(&CancelRequestV1 { run: "b".into() }),
            Err(InferenceLaneError::UnknownRun("b".into()))
        );
        assert_eq!(
            lane.ask(ask("", "q")),
            Err(InferenceLaneError::Empty("run"))
        );
        lane.close().unwrap();
    }

    #[test]
    fn close_cancels_queued_runs_and_refuses_later_asks() {
        let (open_gate, gate) = sync_channel::<()>(0);
        let (emitted, _fragments) = sync_channel::<usize>(100);
        let lane = InferenceLane::open(move || {
            Box::new(GatedProvider {
                capability: StubInferenceProvider::new().capability().clone(),
                gate: Mutex::new(gate),
                emitted,
            })
        });
        lane.ask(ask("a", "first")).unwrap();
        lane.ask(ask("b", "second")).unwrap();
        wait_for(&lane, |runs| run(runs, "a").state == RunStateV1::Running);
        // Unblock the provider once close has sent its cancels, so the actor
        // sees them at the next fragment rather than finishing the run.
        let feeder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            while open_gate.send(()).is_ok() {}
        });
        lane.close().unwrap();
        feeder.join().unwrap();
        let runs = lane.runs();
        assert_eq!(run(&runs, "a").state, RunStateV1::Cancelled);
        assert_eq!(run(&runs, "b").state, RunStateV1::Cancelled);
        assert_eq!(lane.ask(ask("c", "late")), Err(InferenceLaneError::Closed));
        lane.close().unwrap();
    }

    #[test]
    fn actor_panic_is_reported_even_after_all_runs_become_terminal() {
        struct Panics(StubInferenceProvider);
        impl InferenceProvider for Panics {
            fn capability(&self) -> &esp::infer::ModelCapability {
                self.0.capability()
            }
            fn generate_streaming(
                &self,
                request: &GenerationRequest,
                _: &mut dyn FnMut(&str) -> ControlFlow<()>,
            ) -> Result<String, InferError> {
                if request.prompt == "trigger" {
                    panic!("provider panic fixture")
                }
                Ok("complete".into())
            }
        }

        let lane = InferenceLane::open(|| Box::new(Panics(StubInferenceProvider::new())));
        lane.ask(ask("done", "complete")).unwrap();
        let runs = wait_for(&lane, |runs| run(runs, "done").state.is_terminal());
        assert_eq!(run(&runs, "done").state, RunStateV1::Finished);
        lane.ask(ask("panic", "trigger")).unwrap();
        let runs = wait_for(&lane, |runs| run(runs, "panic").state.is_terminal());
        assert!(!lane.is_locked(), "actor loss is not an owner lock signal");
        assert_eq!(run(&runs, "panic").state, RunStateV1::Failed);
        assert_eq!(
            run(&runs, "panic").error.as_deref(),
            Some("inference actor disconnected before completing work")
        );
        lane.set_locked(true).unwrap();
        assert!(lane.is_locked());
        lane.set_locked(false).unwrap();
        assert!(
            !lane.is_locked(),
            "explicit host unlock remains authoritative"
        );
        assert_eq!(
            lane.ask(ask("after", "must refuse")),
            Err(InferenceLaneError::Closed)
        );
        assert!(lane.runs().runs.iter().all(|run| run.run != "after"));
        assert!(lane.close().unwrap_err().contains("actor panicked"));
        // Close results are stable when queried again.
        assert!(lane.close().unwrap_err().contains("actor panicked"));
    }

    #[test]
    fn buffered_finished_update_survives_a_later_actor_panic() {
        struct FinishThenPanic(StubInferenceProvider);
        impl InferenceProvider for FinishThenPanic {
            fn capability(&self) -> &esp::infer::ModelCapability {
                self.0.capability()
            }
            fn generate_streaming(
                &self,
                request: &GenerationRequest,
                _: &mut dyn FnMut(&str) -> ControlFlow<()>,
            ) -> Result<String, InferError> {
                if request.prompt == "panic" {
                    panic!("buffered completion panic fixture");
                }
                Ok("finished before the actor panic".into())
            }
        }

        // Hold the bridge before it receives any updates. The actor can then
        // buffer the first run's Finished update and panic on the second run.
        let (release_bridge, bridge_gate) = std::sync::mpsc::channel();
        let lane = InferenceLane::try_open_with_journal_and_bridge_gate(
            8,
            || Ok(Box::new(FinishThenPanic(StubInferenceProvider::new()))),
            None,
            Some(bridge_gate),
        )
        .unwrap();
        lane.ask(ask("completed", "finish")).unwrap();
        lane.ask(ask("panic", "panic")).unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let finished = lane
                .shared
                .actor
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(ActorHandle::is_finished);
            if finished {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "actor did not finish after panic"
            );
            std::thread::sleep(Duration::from_millis(2));
        }

        assert_eq!(
            lane.ask(ask("after", "must be refused")),
            Err(InferenceLaneError::Closed)
        );
        assert_eq!(
            lane.cancel(&CancelRequestV1 {
                run: "completed".into()
            }),
            Err(InferenceLaneError::Closed)
        );
        assert_eq!(
            lane.pause("completed", "user"),
            Err(InferenceLaneError::Closed)
        );
        assert_eq!(
            lane.resume("completed", "user"),
            Err(InferenceLaneError::Closed)
        );
        assert_eq!(run(&lane.runs(), "completed").state, RunStateV1::Queued);
        assert!(
            lane.set_locked(true).is_err(),
            "buffered updates are not a durable boundary"
        );
        assert!(lane.is_locked());
        assert!(lane.set_locked(false).is_err());
        assert!(!lane.is_locked());
        release_bridge.send(()).unwrap();

        let runs = wait_for(&lane, |runs| {
            run(runs, "completed").state.is_terminal() && run(runs, "panic").state.is_terminal()
        });
        assert_eq!(run(&runs, "completed").state, RunStateV1::Finished);
        assert_eq!(
            run(&runs, "completed").text,
            "finished before the actor panic"
        );
        assert_eq!(run(&runs, "panic").state, RunStateV1::Failed);
        assert!(lane.close().unwrap_err().contains("actor panicked"));
    }

    #[test]
    fn disconnected_durable_ask_is_failed_and_later_refusal_is_not_journaled() {
        #[derive(Default)]
        struct MemoryJournal(Mutex<Vec<RunRecord>>);
        impl RunJournal for MemoryJournal {
            fn device(&self) -> &str {
                "fixture"
            }
            fn load(&self) -> Result<Vec<RunRecord>, String> {
                Ok(self.0.lock().unwrap().clone())
            }
            fn append(&self, record: &RunRecord) -> Result<(), String> {
                self.0.lock().unwrap().push(record.clone());
                Ok(())
            }
        }
        struct Panics(StubInferenceProvider);
        impl InferenceProvider for Panics {
            fn capability(&self) -> &esp::infer::ModelCapability {
                self.0.capability()
            }
            fn generate_checkpointed(
                &self,
                _: &GenerationRequest,
                _: Option<&GenerationCheckpoint>,
                _: &mut dyn FnMut(&GenerationCheckpoint) -> Result<(), InferError>,
                _: &mut dyn FnMut() -> bool,
            ) -> Result<GenerationCheckpoint, InferError> {
                panic!("durable provider panic fixture")
            }
            fn generate_streaming(
                &self,
                _: &GenerationRequest,
                _: &mut dyn FnMut(&str) -> ControlFlow<()>,
            ) -> Result<String, InferError> {
                unreachable!("a journaled run uses the checkpointed provider entry point")
            }
        }

        let journal = Arc::new(MemoryJournal::default());
        let lane = InferenceLane::try_open_with_journal(
            8,
            || Ok(Box::new(Panics(StubInferenceProvider::new()))),
            Some(journal.clone()),
        )
        .unwrap();
        lane.ask(ask("durable", "trigger")).unwrap();
        let runs = wait_for(&lane, |runs| run(runs, "durable").state.is_terminal());
        assert_eq!(run(&runs, "durable").state, RunStateV1::Failed);
        let saved = journal.0.lock().unwrap();
        assert_eq!(saved.last().unwrap().view.state, RunStateV1::Failed);
        let saved_count = saved.len();
        drop(saved);
        assert_eq!(
            lane.ask(ask("after", "must refuse")),
            Err(InferenceLaneError::Closed)
        );
        assert_eq!(journal.0.lock().unwrap().len(), saved_count);
        assert!(lane.close().unwrap_err().contains("actor panicked"));
    }

    #[test]
    fn close_retains_every_terminal_journal_failure() {
        struct FailingTerminalJournal;
        impl RunJournal for FailingTerminalJournal {
            fn device(&self) -> &str {
                "journal-failure-control"
            }
            fn load(&self) -> Result<Vec<RunRecord>, String> {
                Ok(Vec::new())
            }
            fn append(&self, record: &RunRecord) -> Result<(), String> {
                if record.view.state.is_terminal() {
                    Err(format!("cannot retain {}", record.view.run))
                } else {
                    Ok(())
                }
            }
        }
        let lane = InferenceLane::try_open_with_journal(
            8,
            || Ok(Box::new(StubInferenceProvider::new())),
            Some(Arc::new(FailingTerminalJournal)),
        )
        .unwrap();
        for name in ["first", "second"] {
            lane.ask(ask(name, "question")).unwrap();
            wait_for(&lane, |runs| run(runs, name).state.is_terminal());
        }
        let error = lane.set_locked(true).unwrap_err();
        assert!(lane.is_locked(), "the host lock remains authoritative");
        assert!(error.contains("cannot retain first"), "{error}");
        assert!(error.contains("cannot retain second"), "{error}");
        let error = lane.close().unwrap_err();
        assert!(error.contains("cannot retain first"), "{error}");
        assert!(error.contains("cannot retain second"), "{error}");
        assert_eq!(lane.close(), Err(error));
    }

    fn invoke(
        endpoint: &mut InferenceAskEndpoint,
        intent: &str,
        payload: &impl Serialize,
    ) -> IntentResult {
        endpoint
            .invoke(IntentInvocation {
                session: session(),
                target: InstanceId(0),
                observed_epoch: SceneEpoch(1),
                observed_revision: Revision(0),
                intent: intent.into(),
                payload: serde_json::to_vec(payload).unwrap(),
            })
            .unwrap()
    }

    fn served_runs(endpoint: &mut InferenceAskEndpoint) -> InferenceRunsV1 {
        let request = endpoint.describe().projections[0].request.clone();
        let snapshot = endpoint.snapshot(request).unwrap();
        let offer = snapshot.presentation.offers.values().next().unwrap()[0].clone();
        let card_bytes = endpoint
            .resource(ResourceRequest {
                session: session(),
                resource: offer.resource,
            })
            .unwrap()
            .bytes;
        let card: PortableCardV1 = serde_json::from_slice(&card_bytes).unwrap();
        let runs_bytes = endpoint
            .resource(ResourceRequest {
                session: session(),
                resource: card.media[0],
            })
            .unwrap()
            .bytes;
        serde_json::from_slice(&runs_bytes).unwrap()
    }

    #[test]
    fn the_route_asks_streams_and_rings() {
        let lane = InferenceLane::open(|| {
            Box::new(StubInferenceProvider::new().with_response("why?", "because"))
        });
        let mut endpoint = lane.endpoint(session());
        // Let the actor's Ready land first, so the notices below are ours.
        wait_for(&lane, |runs| runs.model.is_some());
        assert!(endpoint.poll_notice().unwrap().is_some());
        let _ = served_runs(&mut endpoint);
        assert!(endpoint.poll_notice().unwrap().is_none());

        assert_eq!(
            invoke(&mut endpoint, ASK_INTENT, &ask("a", "why?")),
            IntentResult::Accepted
        );
        wait_for(&lane, |runs| run(runs, "a").state.is_terminal());
        let notice = endpoint.poll_notice().unwrap().expect("the table moved");
        assert_eq!(notice.session, session());
        let runs = served_runs(&mut endpoint);
        assert_eq!(runs.schema, INFERENCE_RUNS_SCHEMA);
        assert_eq!(run(&runs, "a").text, "because");
        assert_eq!(run(&runs, "a").state, RunStateV1::Finished);
        assert!(endpoint.poll_notice().unwrap().is_none());

        assert!(matches!(
            invoke(&mut endpoint, ASK_INTENT, &ask("a", "again")),
            IntentResult::Rejected { .. }
        ));
        assert!(matches!(
            invoke(&mut endpoint, "something-else", &ask("x", "y")),
            IntentResult::Rejected { .. }
        ));
        assert!(matches!(
            invoke(&mut endpoint, CANCEL_INTENT, &serde_json::json!({"run": 3})),
            IntentResult::Rejected { .. }
        ));
        lane.close().unwrap();
    }

    #[test]
    fn another_sessions_requests_are_errors() {
        let lane = InferenceLane::open(|| Box::new(StubInferenceProvider::new()));
        let mut endpoint = lane.endpoint(session());
        let request = ProjectionRequest {
            version: ProtocolVersion::V1,
            session: ProjectionSession("other".into()),
            score: score(),
        };
        assert!(endpoint.snapshot(request).is_err());
        lane.close().unwrap();
    }
}

#[cfg(all(test, feature = "inference-model"))]
mod publication_tests {
    use super::*;
    use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
    fn publication(owner: &str, text: &str) -> recall::PublishCorpusRequestV1 {
        recall::PublishCorpusRequestV1 {
            owner: owner.into(),
            sources: vec![RetainedSourceText {
                source: RecallSourceId {
                    kind: RecallSourceKind::PageText,
                    owner: Some(owner.into()),
                    source: "https://retained.test/note".into(),
                    node: None,
                },
                version: eidetic::Hash::of(text.as_bytes()).to_string(),
                title: None,
                text: text.into(),
            }],
        }
    }
    #[test]
    fn published_rankings_follow_explicit_scope_republication_lock_and_withdrawal() {
        let lane = InferenceLane::open(|| Box::new(StubInferenceProvider::new()));
        let mut endpoint = lane.endpoint(ProjectionSession("publisher".into()));
        let mut request = publication("turnstone/one", "retained nebula amberstone");
        let first = request.sources[0].source.clone();
        let mut second = request.sources[0].clone();
        second.source.source.push_str("/second");
        second.text = "retained nebula indigo".into();
        second.version = eidetic::Hash::of(second.text.as_bytes()).to_string();
        let second_id = second.source.clone();
        request.sources.push(second);
        endpoint.publish(request).unwrap();
        let old = lane
            .published_recall_rankings("turnstone/one", "nebula", &[first.clone()])
            .unwrap();
        assert_eq!(old.lexical.len(), 1);
        assert_eq!(old.fused, old.lexical);
        assert!(old.vector.is_empty());
        assert!(
            lane.published_recall_rankings("turnstone/one", "nebula", &[])
                .is_err()
        );
        let mut foreign = first.clone();
        foreign.owner = Some("turnstone/two".into());
        assert!(
            lane.published_recall_rankings("turnstone/one", "nebula", &[foreign])
                .is_err()
        );
        assert_eq!(
            lane.published_recall_rankings(
                "turnstone/one",
                "nebula",
                &[first.clone(), second_id.clone()]
            )
            .unwrap()
            .lexical
            .len(),
            2
        );
        lane.configure_published_recall(Default::default()).unwrap();
        assert_eq!(
            lane.published_recall_rankings("turnstone/one", "nebula", &[first.clone()])
                .unwrap()
                .lexical,
            old.lexical
        );
        endpoint
            .publish(publication("turnstone/one", "retained nebula violet"))
            .unwrap();
        let changed = lane
            .published_recall_rankings("turnstone/one", "nebula", &[first.clone()])
            .unwrap();
        assert_eq!(changed.lexical.len(), 1);
        assert_ne!(changed.lexical, old.lexical);
        assert!(
            lane.published_recall_rankings("turnstone/one", "nebula", &[second_id])
                .unwrap()
                .lexical
                .is_empty()
        );
        lane.set_locked(true).unwrap();
        assert!(
            lane.published_recall_rankings("turnstone/one", "nebula", &[first.clone()])
                .is_err()
        );
        lane.set_locked(false).unwrap();
        drop(endpoint);
        assert!(
            lane.published_recall_rankings("turnstone/one", "nebula", &[first])
                .is_err()
        );
        lane.close().unwrap();
    }
    #[test]
    fn publishers_cannot_replace_another_live_owner_and_disconnect_withdraws_heads() {
        let lane = InferenceLane::open(|| Box::new(StubInferenceProvider::new()));
        let mut a = lane.endpoint(ProjectionSession("a".into()));
        let mut b = lane.endpoint(ProjectionSession("b".into()));
        a.publish(publication("turnstone/one", "retained nebula amberstone"))
            .unwrap();
        assert!(
            b.publish(publication("turnstone/one", "retained nebula indigo"))
                .is_err()
        );
        b.publish(publication("turnstone/two", "retained nebula indigo"))
            .unwrap();
        let mut previous = {
            let published = lane.shared.published.lock().unwrap();
            published.owners["turnstone/one"]
                .binding
                .index
                .as_ref()
                .unwrap()
                .prepare("nebula", &[])
                .unwrap()
                .0
        };
        let old = previous.passages[0].passage.clone();
        previous.validate(
            "amberstone [1]",
            Some(
                lane.shared.published.lock().unwrap().owners["turnstone/one"]
                    .binding
                    .corpus
                    .as_ref(),
            ),
        );
        assert_eq!(previous.citations.len(), 1);
        a.publish(publication("turnstone/one", "retained nebula violet"))
            .unwrap();
        previous.validate(
            "amberstone [1]",
            Some(
                lane.shared.published.lock().unwrap().owners["turnstone/one"]
                    .binding
                    .corpus
                    .as_ref(),
            ),
        );
        assert!(previous.citations.is_empty());
        assert_eq!(previous.passages[0].passage, old);
        drop(a);
        let published = lane.shared.published.lock().unwrap();
        assert!(!published.owners.contains_key("turnstone/one"));
        assert!(published.owners.contains_key("turnstone/two"));
        drop(published);
        drop(b);
        lane.close().unwrap();
    }
    #[test]
    fn publications_require_owner_and_exact_page_text_bytes_and_never_make_a_stub_answer() {
        let lane = InferenceLane::open(|| Box::new(StubInferenceProvider::new()));
        let mut endpoint = lane.endpoint(ProjectionSession("publisher".into()));
        let mut wrong = publication("turnstone/one", "nebula amberstone");
        wrong.sources[0].source.owner = Some("turnstone/two".into());
        assert!(endpoint.publish(wrong).is_err());
        let mut wrong = publication("turnstone/one", "nebula amberstone");
        wrong.sources[0].text.push_str(" changed");
        assert!(endpoint.publish(wrong).is_err());
        endpoint
            .publish(publication("turnstone/one", "nebula amberstone"))
            .unwrap();
        let result = lane.ask_internal(
            AskRequestV1 {
                run: "published".into(),
                question: "nebula".into(),
            },
            Default::default(),
            Some(RecallSelection::Published {
                owner: "turnstone/one".into(),
                publisher: endpoint.session.clone(),
                answer_format: recall::AnswerFormat::Prose,
                scope: vec![],
            }),
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("stub inference cannot answer")
        );
        assert!(lane.runs().runs.is_empty());
        drop(endpoint);
        lane.close().unwrap();
    }
}
