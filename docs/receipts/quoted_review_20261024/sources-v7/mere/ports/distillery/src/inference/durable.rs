// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Signed, encrypted private ask logs over the resident's existing
//! p2panda endpoint. Each token appends only new IDs and text; immutable prompt
//! bytes appear once. Concurrent continuations remain a visible fork.
use super::*;
use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use eidetic::models::inference::{InferenceContinuation, InferenceRunRecord, InferenceRunStatus};
use muniment::Backend;
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_store::topics::TopicStore;
use personae::Ed25519Keypair;
use rand_core::{OsRng, RngCore};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use stickleback::{
    Admission, JoinedSpace, MunimentStore, OperationPolicy, OperationProcessor, Reject, StoreTarget,
};
use zeroize::{Zeroize, Zeroizing};

/// Signed address of one device's per-ask log; prompt text stays encrypted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunExt {
    pub space: [u8; 32],
    pub device: [u8; 32],
    pub run: [u8; 32],
    /// Public domain-specific proof of the writer's stable root. Omitted on
    /// existing persona-only operations so their signed bytes remain valid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authority: Vec<u8>,
}
/// The composing host supplies its existing group's protection and admission.
/// This seam cannot create a group, change its members, or export its keys.
pub trait RunGroupProtection: Send + Sync {
    /// This device currently holds a writing seat and the current group epoch.
    fn can_write(&self) -> bool;
    /// Changes when the host installs keys or changes admission.
    fn revision(&self) -> u64;
    /// Authenticate the signed address inside the group envelope.
    fn seal(&self, address: &RunExt, clear: &[u8]) -> Result<Vec<u8>, String>;
    /// Check live admission and return authenticated plaintext for domain
    /// validation before insertion. Historical reads use `open` instead.
    fn admit(&self, writer: [u8; 32], address: &RunExt, sealed: &[u8]) -> Result<Vec<u8>, String>;
    /// Open an already admitted operation without reapplying live membership.
    fn open(&self, writer: [u8; 32], address: &RunExt, sealed: &[u8]) -> Result<Vec<u8>, String>;
}

#[derive(Clone)]
struct Policy {
    space: [u8; 32],
    author: VerifyingKey,
    group: Option<Arc<dyn RunGroupProtection>>,
    key: Arc<Mutex<Option<Zeroizing<[u8; 32]>>>>,
    has_parent: Arc<dyn Fn(&Hash, &[u8; 32]) -> Result<bool, String> + Send + Sync>,
}
impl OperationPolicy<RunExt> for Policy {
    type LogId = ([u8; 32], [u8; 32]);
    fn admit(&self, op: &Operation<RunExt>) -> Result<Admission<Self::LogId>, Reject> {
        let key = self.key.lock().unwrap();
        if key.is_none() {
            return Err(Reject::new("inference-locked", "inference data is locked"));
        }
        if op.header.extensions.space != self.space {
            return Err(Reject::new(
                "wrong-space",
                "inference log addresses another space",
            ));
        }
        if op.header.payload_size > 1024 * 1024 || op.body.is_none() {
            return Err(Reject::new(
                "invalid-checkpoint",
                "missing or oversized encrypted checkpoint",
            ));
        }
        if let Some(group) = &self.group {
            // Verify the attested writer and authenticated group envelope before
            // insertion. Merely knowing this topic or endpoint grants nothing.
            let clear = Zeroizing::new(
                group
                    .admit(
                        *op.header.verifying_key.as_bytes(),
                        &op.header.extensions,
                        op.body.as_ref().unwrap().as_bytes(),
                    )
                    .map_err(|error| Reject::new("private-inference", error))?,
            );
            let mut delta: Delta = serde_json::from_slice(&clear)
                .map_err(|error| Reject::new("invalid-checkpoint", error.to_string()))?;
            // The parsed suffix is not needed for admission. Clear it even if
            // the address or parent check below refuses the operation.
            delta.tokens.zeroize();
            delta.text.zeroize();
            delta
                .check_semantics()
                .map_err(|error| Reject::new("checkpoint-semantics", error))?;
            // Never persist a dangling cross-device child. A crash while
            // catching up must leave a causally closed store that can reopen;
            // LogSync can offer the child again after its parent is admitted.
            if let Some(parent) = &delta.parent {
                let parent: Hash = parent
                    .parse()
                    .map_err(|_| Reject::new("invalid-checkpoint", "invalid checkpoint parent"))?;
                if !(self.has_parent)(&parent, &op.header.extensions.run)
                    .map_err(|error| Reject::new("checkpoint-store", error))?
                {
                    return Err(Reject::new(
                        "missing-parent",
                        "missing checkpoint parent; retry after its parent",
                    ));
                }
            }
            if delta.record.as_ref().is_some_and(|record| {
                *Hash::digest(record.view.run.as_bytes()).as_bytes() != op.header.extensions.run
            }) {
                return Err(Reject::new(
                    "invalid-checkpoint",
                    "checkpoint addresses another run",
                ));
            }
        } else if op.header.verifying_key != self.author
            || !op.header.extensions.authority.is_empty()
        {
            return Err(Reject::new(
                "wrong-persona",
                "inference log requires this persona's derived signer",
            ));
        }
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.space),
            (op.header.extensions.run, op.header.extensions.device),
        )))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Delta {
    /// Older checkpoint readers deny unknown fields. Keep this authenticated
    /// marker on every context-bearing entry so they refuse rather than lose
    /// queued context or passage evidence during deserialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    required_semantics: Option<String>,
    parent: Option<String>,
    /// Full state at creation and state/device transitions only.
    record: Option<RunRecord>,
    tokens: Vec<u32>,
    text: String,
    draws: u64,
    complete: bool,
    state: RunStateV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence: Option<InferenceRunRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence_schema: Option<eidetic::SchemaRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer_support: Option<eidetic::ManifestId>,
}
fn required_semantics(record: &RunRecord) -> Option<&'static str> {
    if record
        .view
        .recall
        .as_ref()
        .and_then(|r| r.answer_contract.as_ref())
        .is_some_and(|c| c.method == "distillery.quoted-review/v7")
    {
        return Some("recall/quoted-candidate-review-v1");
    }
    if record
        .view
        .recall
        .as_ref()
        .and_then(|r| r.answer_contract.as_ref())
        .is_some_and(|c| c.method == "distillery.candidate-review/v6")
    {
        return Some("recall/candidate-review-v1");
    }
    if record
        .view
        .recall
        .as_ref()
        .and_then(|r| r.answer_contract.as_ref())
        .and_then(|c| c.requested_facts.as_ref())
        .is_some()
    {
        Some("recall/requested-fact-set-v1")
    } else if record.request.output_grammar.is_some() {
        Some("recall/constrained-answer-v1")
    } else if record
        .view
        .recall
        .as_ref()
        .is_some_and(|e| e.answer_contract.is_some())
    {
        Some("recall/requested-facts-v1")
    } else if record
        .view
        .recall
        .as_ref()
        .is_some_and(|e| e.answer_format == recall::AnswerFormat::ClaimSupportV1)
    {
        Some("recall/claim-support-v1")
    } else if record
        .view
        .recall
        .as_ref()
        .is_some_and(|evidence| evidence.owner.is_some())
    {
        Some("recall/published-v1")
    } else if record.view.recall.is_some() || !record.request.context.is_empty() {
        Some("recall/v1")
    } else {
        None
    }
}
impl Delta {
    fn check_semantics(&self) -> Result<(), String> {
        if self.required_semantics.as_deref().is_some_and(|value| {
            value != "recall/v1"
                && value != "recall/published-v1"
                && value != "recall/claim-support-v1"
                && value != "recall/requested-facts-v1"
                && value != "recall/constrained-answer-v1"
                && value != "recall/requested-fact-set-v1"
                && value != "recall/candidate-review-v1"
                && value != "recall/quoted-candidate-review-v1"
        }) {
            return Err("unsupported inference checkpoint semantics".into());
        }
        if let Some(record) = &self.record {
            if required_semantics(record) != self.required_semantics.as_deref() {
                return Err("checkpoint recall semantics do not match its retained inputs".into());
            }
            check_answer_contract(record)?;
        }
        Ok(())
    }
}

fn check_answer_contract(record: &RunRecord) -> Result<(), String> {
    if record
        .checkpoint
        .as_ref()
        .is_some_and(|cp| cp.request.output_grammar != record.request.output_grammar)
    {
        return Err(
            "checkpoint decoding grammar differs from its retained generation request".into(),
        );
    }
    if let Some(contract) = record
        .view
        .recall
        .as_ref()
        .and_then(|r| r.answer_contract.as_ref())
    {
        if matches!(
            contract.method.as_str(),
            "distillery.requested-facts/v5"
                | "distillery.candidate-review/v6"
                | "distillery.quoted-review/v7"
        ) || contract.requested_facts.is_some()
        {
            let facts = contract
                .requested_facts
                .as_ref()
                .ok_or("retained requested-fact contract is missing its fact set")?;
            facts.validate(&record.view.question)?;
            let recall = record.view.recall.as_ref().unwrap();
            let (expected, prompt, context) = super::answer_contract::prepare_for_method(
                &record.view.question,
                recall,
                &contract.method,
            )?;
            if &expected != contract
                || prompt != record.request.prompt
                || context != record.request.context
            {
                return Err(
                    "retained requested facts do not match the exact generation task".into(),
                );
            }
        }
        if contract.question != eidetic::ManifestId::of_blob(record.view.question.as_bytes())
            || contract.prompt != eidetic::ManifestId::of_blob(record.request.prompt.as_bytes())
            || contract.context != eidetic::ManifestId::of_blob(record.request.context.as_bytes())
            || contract.output_grammar
                != record
                    .request
                    .output_grammar
                    .as_ref()
                    .map(|g| {
                        serde_json::to_vec(g).map(|bytes| eidetic::ManifestId::of_blob(&bytes))
                    })
                    .transpose()
                    .map_err(|e| e.to_string())?
        {
            return Err(
                "checkpoint answer contract does not match its exact generation inputs".into(),
            );
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Sealed {
    nonce: [u8; 24],
    bytes: Vec<u8>,
}

/// Durable encrypted log and its joined replication session. The host clears
/// the data key after pausing asks, and re-derives it at unlock.
pub struct PersonaRunJournal<B: Backend + Clone + Send + Sync + 'static> {
    store: MunimentStore<B, RunExt>,
    policy: Policy,
    signing: SigningKey,
    device: String,
    device_hash: [u8; 32],
    key: Arc<Mutex<Option<Zeroizing<[u8; 32]>>>>,
    heads: Mutex<BTreeMap<String, (String, RunRecord)>>,
    joined: Mutex<Option<JoinedSpace<RunExt>>>,
    sync_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    sync_wake: Arc<tokio::sync::Notify>,
    sync_stop: AtomicBool,
    sync_retry: AtomicBool,
    prior_sync: Mutex<stickleback::SyncStatus>,
    /// Serializes local append and explicit refresh, including log frontier reads.
    writer: Mutex<()>,
    authority: Vec<u8>,
    import: Mutex<Option<Arc<Self>>>,
}
impl<B: Backend + Clone + Send + Sync + 'static> PersonaRunJournal<B> {
    /// Bind a local store to the persona signer, stable device ID and data key.
    pub fn new(
        backend: B,
        space: [u8; 32],
        author: &Ed25519Keypair,
        device: String,
        key: [u8; 32],
    ) -> Self {
        let signing = SigningKey::from_bytes(&author.to_seed());
        let key = Arc::new(Mutex::new(Some(Zeroizing::new(key))));
        let store = MunimentStore::new(backend);
        let parent_store = store.clone();
        Self {
            store,
            policy: Policy {
                space,
                author: signing.verifying_key(),
                group: None,
                key: key.clone(),
                has_parent: Arc::new(move |hash, run| {
                    match pollster::block_on(parent_store.get_operation(hash))
                        .map_err(|error| error.to_string())?
                    {
                        None => Ok(false),
                        Some(parent) if parent.header.extensions.run == *run => Ok(true),
                        Some(_) => Err("checkpoint parent addresses another run".into()),
                    }
                }),
            },
            signing,
            device_hash: *Hash::digest(device.as_bytes()).as_bytes(),
            device,
            key,
            heads: Mutex::new(BTreeMap::new()),
            joined: Mutex::new(None),
            sync_task: Mutex::new(None),
            sync_wake: Arc::new(tokio::sync::Notify::new()),
            sync_stop: AtomicBool::new(false),
            sync_retry: AtomicBool::new(false),
            prior_sync: Mutex::new(stickleback::SyncStatus::default()),
            writer: Mutex::new(()),
            authority: Vec::new(),
            import: Mutex::new(None),
        }
    }
    /// Bind separately rooted devices through a host-owned encryption group.
    /// The local key seals Eidetic materializations only; it never leaves this
    /// device. Existing local-only history is imported when the group is keyed.
    pub fn for_group(
        backend: B,
        space: [u8; 32],
        author: &Ed25519Keypair,
        device: String,
        local_key: [u8; 32],
        authority: Vec<u8>,
        group: Arc<dyn RunGroupProtection>,
        import: Option<Arc<Self>>,
    ) -> Self {
        let mut journal = Self::new(backend, space, author, device, local_key);
        journal.authority = authority;
        journal.policy.group = Some(group);
        journal.import = Mutex::new(import);
        journal
    }

    fn import_local(&self) -> Result<(), String> {
        let mut pending = self.import.lock().unwrap();
        let Some(source) = pending.as_ref() else {
            return Ok(());
        };
        if !self
            .policy
            .group
            .as_ref()
            .is_some_and(|group| group.can_write())
        {
            return Ok(());
        }
        let records = source.load()?;
        {
            let _writer = self.writer.lock().unwrap();
            *self.heads.lock().unwrap() = self.fold()?;
        }
        for record in records {
            let existing = self.heads.lock().unwrap().get(&record.view.run).cloned();
            if let Some((_, shared)) = existing {
                let same_origin = record.view.question == shared.view.question
                    && record.request == shared.request
                    && record.continuations.len() <= shared.continuations.len()
                    && record
                        .continuations
                        .iter()
                        .zip(&shared.continuations)
                        .all(|(a, b)| {
                            a.device == b.device && a.at_token == b.at_token && a.reason == b.reason
                        })
                    && record.checkpoint.as_ref().is_none_or(|local| {
                        shared.checkpoint.as_ref().is_some_and(|current| {
                            local.session == current.session
                                && local.request == current.request
                                && local.prompt_ids == current.prompt_ids
                                && current.token_ids.starts_with(&local.token_ids)
                        })
                    });
                if !same_origin {
                    return Err(format!(
                        "local inference run {} collides with personal graph history; the local source is retained",
                        record.view.run
                    ));
                }
            } else {
                self.append(&record)?;
            }
        }
        *pending = None;
        Ok(())
    }

    /// Join the ask lane on the resident-owned endpoint and gossip overlay.
    pub async fn join(
        self: &Arc<Self>,
        endpoint: stickleback::Endpoint,
        gossip: stickleback::Gossip,
    ) -> Result<(), String> {
        self.open_sync(endpoint.clone(), gossip.clone()).await?;
        let weak = Arc::downgrade(self);
        let wake = self.sync_wake.clone();
        let mut revision = self
            .policy
            .group
            .as_ref()
            .map_or(0, |group| group.revision());
        *self.sync_task.lock().unwrap() = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = wake.notified() => {},
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
                }
                let Some(journal) = weak.upgrade() else { break };
                if journal.sync_stop.load(Ordering::Acquire) {
                    break;
                }
                let current = journal
                    .policy
                    .group
                    .as_ref()
                    .map_or(0, |group| group.revision());
                if current == revision && !journal.sync_retry.load(Ordering::Acquire) {
                    continue;
                }
                if journal.key.lock().unwrap().is_none() {
                    continue;
                }
                revision = current;
                journal.sync_retry.store(false, Ordering::Release);
                // LogSync has no public force-reconcile hook. Reopening its
                // session advertises the retained frontier, so refused locked
                // or causally early operations can be fetched again.
                let previous = journal.joined.lock().unwrap().take();
                if let Some(previous) = previous {
                    let status = previous.sync_status();
                    {
                        let mut prior = journal.prior_sync.lock().unwrap();
                        prior.ops_received += status.ops_received;
                        prior.sync_rounds += status.sync_rounds;
                        prior.last_activity_ms = status.last_activity_ms.or(prior.last_activity_ms);
                    }
                    if let Err(error) = previous.leave_and_wait().await {
                        eprintln!("close inference reconciliation before retry: {error}");
                    }
                }
                if let Err(error) = journal.open_sync(endpoint.clone(), gossip.clone()).await {
                    eprintln!("reopen inference reconciliation: {error}");
                    journal.sync_retry.store(true, Ordering::Release);
                }
            }
        }));
        Ok(())
    }

    async fn open_sync(
        self: &Arc<Self>,
        endpoint: stickleback::Endpoint,
        gossip: stickleback::Gossip,
    ) -> Result<(), String> {
        let journal = Arc::downgrade(self);
        let joined = JoinedSpace::join::<_, ([u8; 32], [u8; 32]), _, _>(
            stickleback::lane_id("inference/v1", self.policy.space),
            self.store.clone(),
            endpoint,
            gossip,
            self.policy.space,
            move |op| {
                let journal = journal.clone();
                async move {
                    match journal.upgrade() {
                        Some(journal) => journal.accept(&op).await.unwrap_or(false),
                        None => false,
                    }
                }
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        *self.joined.lock().unwrap() = Some(joined);
        Ok(())
    }
    /// End replication and await release of captured store handles.
    pub async fn shutdown(&self) -> Result<(), String> {
        self.sync_stop.store(true, Ordering::Release);
        self.sync_wake.notify_one();
        let task = self.sync_task.lock().unwrap().take();
        if let Some(task) = task {
            task.await.map_err(|error| error.to_string())?;
        }
        let joined = self.joined.lock().unwrap().take();
        if let Some(joined) = joined {
            joined
                .leave_and_wait()
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    /// Clear decrypted heads and the retained data key.
    pub fn lock(&self) {
        let pending = self.import.lock().unwrap();
        let _writer = self.writer.lock().unwrap();
        self.heads.lock().unwrap().clear();
        self.key.lock().unwrap().take();
        if let Some(source) = pending.as_ref() {
            source.lock();
        }
    }
    /// Install the freshly re-derived persona data key.
    pub fn unlock(&self, key: [u8; 32]) {
        let pending = self.import.lock().unwrap();
        let _writer = self.writer.lock().unwrap();
        *self.key.lock().unwrap() = Some(Zeroizing::new(key));
        if let Some(source) = pending.as_ref() {
            source.unlock(key);
        }
        self.sync_retry.store(true, Ordering::Release);
        self.sync_wake.notify_one();
    }
    /// Read actual LogSync activity for this lane.
    pub fn sync_status(&self) -> Option<stickleback::SyncStatus> {
        let mut status = self.prior_sync.lock().unwrap().clone();
        if let Some(joined) = self.joined.lock().unwrap().as_ref() {
            let live = joined.sync_status();
            status.ops_received += live.ops_received;
            status.sync_rounds += live.sync_rounds;
            status.syncing = live.syncing;
            status.last_activity_ms = live.last_activity_ms.or(status.last_activity_ms);
        }
        Some(status)
    }
    /// Read the underlying signed-operation store.
    pub fn store(&self) -> &MunimentStore<B, RunExt> {
        &self.store
    }
    /// Verify persona authority and commit a received signed operation.
    pub async fn accept(&self, op: &Operation<RunExt>) -> Result<bool, String> {
        if self.key.lock().unwrap().is_none() {
            return Err("inference data is locked".into());
        }
        let result = OperationProcessor::new(self.store.clone(), self.policy.clone())
            .process(op)
            .await
            .map(|outcome| outcome.inserted())
            .map_err(|error| error.to_string());
        if result
            .as_ref()
            .err()
            .is_some_and(|error| error.contains("missing checkpoint parent"))
        {
            self.sync_retry.store(true, Ordering::Release);
        }
        result
    }
    /// Read all retained operations under this persona topic.
    pub async fn operations(&self) -> Result<Vec<Operation<RunExt>>, String> {
        let logs: BTreeMap<VerifyingKey, Vec<([u8; 32], [u8; 32])>> = self
            .store
            .resolve(&Topic::from(self.policy.space))
            .await
            .map_err(|error| error.to_string())?;
        let mut ops = Vec::new();
        for (author, logs) in logs {
            for log in logs {
                if let Some(entries) = self
                    .store
                    .get_log_entries(&author, &log, None, None)
                    .await
                    .map_err(|error| error.to_string())?
                {
                    ops.extend(entries.into_iter().map(|(op, _)| op));
                }
            }
        }
        Ok(ops)
    }
    /// Project a retained run through Eidetic's inference evidence schema.
    pub fn retained_record(&self, run: &str) -> Result<InferenceRunRecord, String> {
        let _writer = self.writer.lock().unwrap();
        let heads = self.fold()?;
        let id = self.materialize_evidence(&heads.get(run).ok_or("unknown retained run")?.1)?;
        let mut backend = self.store.backend().clone();
        pollster::block_on(eidetic::typed::load_typed_sealed(
            &mut backend,
            &mut eidetic::NoFetcher,
            Some(self),
            id,
        ))
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "retained evidence disappeared".into())
    }
    pub fn retained_answer_support(
        &self,
        run: &str,
    ) -> Result<Option<eidetic::models::answer::AnswerSupport>, String> {
        let record = self
            .load()?
            .into_iter()
            .find(|r| r.view.run == run)
            .ok_or("unknown retained run")?;
        if record.view.state != RunStateV1::Finished {
            return Ok(None);
        }
        let Some(support) = record
            .view
            .recall
            .as_ref()
            .and_then(|r| r.assess_answer(&record.view.text, &record.view.question))
        else {
            return Ok(None);
        };
        let mut backend = self.store.backend().clone();
        let stored: Option<eidetic::models::answer::AnswerSupport> =
            pollster::block_on(eidetic::typed::load_typed_sealed(
                &mut backend,
                &mut eidetic::NoFetcher,
                Some(self),
                support.id(),
            ))
            .map_err(|e| e.to_string())?;
        if stored.as_ref() != Some(&support) {
            return Err("sealed answer support is missing or inconsistent".into());
        }
        Ok(stored)
    }
    pub fn retained_requested_facts(
        &self,
        run: &str,
    ) -> Result<Option<eidetic::models::requested::RequestedFactSet>, String> {
        let record = self
            .load()?
            .into_iter()
            .find(|r| r.view.run == run)
            .ok_or("unknown retained run")?;
        let Some(facts) = record
            .view
            .recall
            .as_ref()
            .and_then(|r| r.answer_contract.as_ref())
            .and_then(|c| c.requested_facts.as_ref())
        else {
            return Ok(None);
        };
        facts.validate(&record.view.question)?;
        let mut backend = self.store.backend().clone();
        let stored: Option<eidetic::models::requested::RequestedFactSet> =
            pollster::block_on(eidetic::typed::load_typed_sealed(
                &mut backend,
                &mut eidetic::NoFetcher,
                Some(self),
                facts.id(),
            ))
            .map_err(|e| e.to_string())?;
        if stored.as_ref() != Some(facts) {
            return Err("sealed requested-fact set is missing or inconsistent".into());
        }
        Ok(stored)
    }
    /// Resolve an exact reference through its sealed Eidetic blob manifest.
    pub fn resolve_evidence_blob(
        &self,
        reference: eidetic::ManifestId,
    ) -> Result<Option<Vec<u8>>, String> {
        let _writer = self.writer.lock().unwrap();
        for (_, (_, record)) in self.fold()? {
            self.materialize_evidence(&record)?;
        }
        let mut backend = self.store.backend().clone();
        pollster::block_on(eidetic::typed::load_typed_sealed::<
            eidetic::models::OpaqueBlob,
        >(
            &mut backend,
            &mut eidetic::NoFetcher,
            Some(self),
            reference,
        ))
        .map(|blob| blob.map(|blob| blob.0))
        .map_err(|error| error.to_string())
    }
    /// Open the immutable native passage artifact named by a retained run.
    pub fn resolve_passage(
        &self,
        id: eidetic::ManifestId,
    ) -> Result<Option<eidetic::models::recall::InferencePassage>, String> {
        let _writer = self.writer.lock().unwrap();
        for (_, (_, record)) in self.fold()? {
            self.materialize_evidence(&record)?;
        }
        let mut backend = self.store.backend().clone();
        pollster::block_on(eidetic::typed::load_typed_sealed(
            &mut backend,
            &mut eidetic::NoFetcher,
            Some(self),
            id,
        ))
        .map_err(|e| e.to_string())
    }
    #[cfg(feature = "inference-embedding")]
    pub fn load_embedding_projection(
        &self,
        envelope: eidetic::ManifestId,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, String> {
        let _writer = self.writer.lock().unwrap();
        let mut backend = self.store.backend().clone();
        let key = format!("inference/embedding-projection/v1/{envelope}");
        let Some(reference) = pollster::block_on(backend.get(&key)).map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        if reference.len() > 256 {
            return Err("invalid embedding cache reference".into());
        }
        let id: eidetic::ManifestId =
            serde_json::from_slice(&reference).map_err(|e| e.to_string())?;
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(&mut backend, id))
            .map_err(|e| e.to_string())?
            .ok_or("embedding cache manifest is unavailable")?;
        if manifest.byte_size > max_bytes as u64
            || eidetic::seal::seal_marker(&manifest)
                .map_err(|e| e.to_string())?
                .is_none()
        {
            return Err("embedding cache exceeds its limit or lacks private custody".into());
        }
        pollster::block_on(eidetic::typed::load_typed_sealed::<
            eidetic::models::OpaqueBlob,
        >(
            &mut backend, &mut eidetic::NoFetcher, Some(self), id
        ))
        .map(|body| body.map(|b| b.0))
        .map_err(|e| e.to_string())
    }
    #[cfg(feature = "inference-embedding")]
    pub fn save_embedding_projection(
        &self,
        envelope: eidetic::ManifestId,
        bytes: &[u8],
    ) -> Result<(), String> {
        let _writer = self.writer.lock().unwrap();
        if !self.execution_permitted() {
            return Err("embedding cache writing is not admitted".into());
        }
        let mut backend = self.store.backend().clone();
        let body = eidetic::models::OpaqueBlob(bytes.to_vec());
        let id = pollster::block_on(eidetic::typed::save_typed_sealed(
            &mut backend,
            Some(self),
            &body,
            vec![],
            eidetic::PrivacyClass::LocalOnly,
            eidetic::ProvenanceRecord::self_imported("resident recall embedding projection"),
            eidetic::TrustEnvelope::self_asserted(),
            eidetic::Timestamp::ZERO,
        ))
        .map_err(|e| e.to_string())?;
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(&mut backend, id))
            .map_err(|e| e.to_string())?
            .ok_or("embedding projection manifest missing")?;
        if eidetic::seal::seal_marker(&manifest)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("embedding projection requires private custody".into());
        }
        // Publish the cache pointer after the immutable private artifact commits.
        // A failed pointer write leaves only an unreferenced derived artifact.
        pollster::block_on(backend.put(
            &format!("inference/embedding-projection/v1/{envelope}"),
            &serde_json::to_vec(&id).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())
    }

    fn save_evidence<T: eidetic::TypedPayload>(
        &self,
        value: &T,
    ) -> Result<eidetic::ManifestId, String> {
        let bytes = Zeroizing::new(
            value
                .serialize_to_bytes()
                .map_err(|error| error.to_string())?,
        );
        let id = eidetic::ManifestId::of_blob(&bytes);
        let mut backend = self.store.backend().clone();
        if pollster::block_on(eidetic::manifest::load_manifest(&mut backend, id))
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(id);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64;
        pollster::block_on(eidetic::typed::save_typed_sealed(
            &mut backend,
            Some(self),
            value,
            vec![],
            eidetic::PrivacyClass::LocalOnly,
            eidetic::ProvenanceRecord::self_imported("inference"),
            eidetic::TrustEnvelope::self_asserted(),
            eidetic::Timestamp(now),
        ))
        .map_err(|error| error.to_string())
    }
    fn materialize_evidence(&self, record: &RunRecord) -> Result<eidetic::ManifestId, String> {
        self.save_evidence(&eidetic::models::OpaqueBlob(
            record.view.question.as_bytes().to_vec(),
        ))?;
        self.save_evidence(&eidetic::models::OpaqueBlob(
            record.view.text.as_bytes().to_vec(),
        ))?;
        if let Some(cp) = &record.checkpoint {
            self.save_evidence(&eidetic::models::OpaqueBlob(
                serde_json::to_vec(cp).map_err(|error| error.to_string())?,
            ))?;
        }
        if let Some(recall) = &record.view.recall {
            for passage in &recall.passages {
                self.save_evidence(&passage.passage)?;
            }
        }
        if record.view.state == RunStateV1::Finished {
            if let Some(recall) = &record.view.recall {
                if let Some(support) =
                    recall.assess_answer(&record.view.text, &record.view.question)
                {
                    self.save_evidence(&support)?;
                }
            }
        }
        self.save_evidence(&Self::evidence(record)?)
    }
    /// Attach a real uninterrupted comparison. Absence of a comparison hash
    /// means unmeasured; a hash with no differing position means equal IDs.
    pub fn record_comparison(
        &self,
        run: &str,
        reference: &GenerationCheckpoint,
    ) -> Result<Option<usize>, String> {
        let mut record = self
            .load()?
            .into_iter()
            .find(|record| record.view.run == run)
            .ok_or("unknown run")?;
        let actual = record
            .checkpoint
            .as_ref()
            .ok_or("run has no model checkpoint")?;
        if actual.session != reference.session
            || actual.request != reference.request
            || !actual.complete
            || !reference.complete
        {
            return Err(
                "comparison requires completed runs of the same exact session and request".into(),
            );
        }
        let first = actual
            .token_ids
            .iter()
            .zip(&reference.token_ids)
            .position(|(a, b)| a != b)
            .or_else(|| {
                (actual.token_ids.len() != reference.token_ids.len())
                    .then_some(actual.token_ids.len().min(reference.token_ids.len()))
            });
        let visit = record
            .continuations
            .last_mut()
            .ok_or("run has no device visit")?;
        visit.first_differing_token = first;
        visit.compared_token_ids_hash = Some(
            eidetic::Hash::of(
                &serde_json::to_vec(&reference.token_ids).map_err(|error| error.to_string())?,
            )
            .to_string(),
        );
        self.append(&record)?;
        Ok(first)
    }

    fn crypt(
        &self,
        bytes: &[u8],
        nonce: [u8; 24],
        address: &RunExt,
        encrypt: bool,
    ) -> Result<Vec<u8>, String> {
        let key = self.key.lock().unwrap();
        let key = key.as_ref().ok_or("inference data is locked")?;
        let cipher = XChaCha20Poly1305::new(&Key::try_from(&key[..]).unwrap());
        let nonce = XNonce::try_from(&nonce[..]).unwrap();
        let aad = serde_json::to_vec(address).map_err(|error| error.to_string())?;
        let payload = Payload {
            msg: bytes,
            aad: &aad,
        };
        if encrypt {
            cipher.encrypt(&nonce, payload)
        } else {
            cipher.decrypt(&nonce, payload)
        }
        .map_err(|_| "inference checkpoint authentication failed".into())
    }
    fn decode(&self, op: &Operation<RunExt>) -> Result<Delta, String> {
        let body = op.body.as_ref().ok_or("checkpoint body missing")?;
        let key = self.key.lock().unwrap();
        if key.is_none() {
            return Err("inference data is locked".into());
        }
        let clear = if let Some(group) = &self.policy.group {
            Zeroizing::new(group.open(
                *op.header.verifying_key.as_bytes(),
                &op.header.extensions,
                body.as_bytes(),
            )?)
        } else {
            drop(key);
            let sealed: Sealed =
                serde_json::from_slice(body.as_bytes()).map_err(|error| error.to_string())?;
            Zeroizing::new(self.crypt(&sealed.bytes, sealed.nonce, &op.header.extensions, false)?)
        };
        let delta: Delta = serde_json::from_slice(&clear).map_err(|error| error.to_string())?;
        delta.check_semantics()?;
        Ok(delta)
    }
    fn fold(&self) -> Result<BTreeMap<String, (String, RunRecord)>, String> {
        let mut pending: BTreeMap<String, (RunExt, Delta)> = pollster::block_on(self.operations())?
            .into_iter()
            .map(|op| {
                self.decode(&op)
                    .map(|delta| (op.hash.to_string(), (op.header.extensions, delta)))
            })
            .collect::<Result<_, _>>()?;
        let mut states: BTreeMap<String, RunRecord> = BTreeMap::new();
        // Keep each parent only until all its children have been replayed.
        // A long linear ask must not retain every full token prefix in memory.
        let mut children = BTreeMap::<String, usize>::new();
        for (_, delta) in pending.values() {
            if let Some(parent) = &delta.parent {
                *children.entry(parent.clone()).or_default() += 1;
            }
        }
        while !pending.is_empty() {
            let ready: Vec<_> = pending
                .iter()
                .filter(|(_, (_, delta))| {
                    delta
                        .parent
                        .as_ref()
                        .is_none_or(|parent| states.contains_key(parent))
                })
                .map(|(id, _)| id.clone())
                .collect();
            if ready.is_empty() {
                return Err("inference checkpoint log has a missing parent".into());
            }
            for id in ready {
                let (address, delta) = pending.remove(&id).unwrap();
                let mut record = match delta.record {
                    Some(record) => record,
                    None => states
                        .get(delta.parent.as_ref().ok_or("token delta has no parent")?)
                        .ok_or("missing checkpoint parent")?
                        .clone(),
                };
                if *Hash::digest(record.view.run.as_bytes()).as_bytes() != address.run {
                    return Err("checkpoint addresses another run".into());
                }
                record.view.state = delta.state;
                if let Some(cp) = &mut record.checkpoint {
                    cp.token_ids.extend(delta.tokens);
                    cp.text.push_str(&delta.text);
                    cp.draws = delta.draws;
                    cp.complete = delta.complete;
                    record.view.text = cp.text.clone();
                }
                let support = if record.view.state == RunStateV1::Finished {
                    record
                        .view
                        .recall
                        .as_ref()
                        .and_then(|r| r.assess_answer(&record.view.text, &record.view.question))
                } else {
                    None
                };
                if delta.answer_support != support.as_ref().map(|s| s.id()) {
                    return Err(
                        "retained answer support does not match raw output and supplied passages"
                            .into(),
                    );
                }
                if let Some(parent) = delta.parent {
                    let remaining = children
                        .get_mut(&parent)
                        .ok_or("missing parent child count")?;
                    *remaining -= 1;
                    if *remaining == 0 {
                        states.remove(&parent);
                    }
                }
                states.insert(id, record);
            }
        }
        let mut heads = BTreeMap::new();
        for (id, record) in states {
            {
                if heads
                    .insert(record.view.run.clone(), (id, record))
                    .is_some()
                {
                    return Err(
                        "inference run has concurrent continuations; choose a branch explicitly"
                            .into(),
                    );
                }
            }
        }
        Ok(heads)
    }
    /// Evidence projection is typed by Eidetic, independent of the decoder's
    /// checkpoint wire. Content addresses refer to the exact bytes in this log.
    pub fn evidence(record: &RunRecord) -> Result<InferenceRunRecord, String> {
        let cp = record.checkpoint.as_ref();
        Ok(InferenceRunRecord {
            run: record.view.run.clone(),
            question_blob: eidetic::ManifestId::of_blob(record.view.question.as_bytes()),
            supplied_passages: record
                .view
                .recall
                .as_ref()
                .map(|recall| recall.passages.iter().map(|p| p.passage.id()).collect())
                .unwrap_or_default(),
            session_id: record
                .view
                .session
                .as_ref()
                .map(|session| session.session_id.clone()),
            prompt_template_hash: record
                .view
                .session
                .as_ref()
                .map(|session| session.prompt_template_hash.clone()),
            weight_format: record.weight_format.clone(),
            request: serde_json::to_value(cp.map_or(&record.request, |cp| &cp.request))
                .map_err(|error| error.to_string())?,
            output_blob: eidetic::ManifestId::of_blob(record.view.text.as_bytes()),
            checkpoint_blob: cp
                .map(|cp| serde_json::to_vec(cp).map(|bytes| eidetic::ManifestId::of_blob(&bytes)))
                .transpose()
                .map_err(|error| error.to_string())?,
            status: match record.view.state {
                RunStateV1::Queued => InferenceRunStatus::Queued,
                RunStateV1::Running => InferenceRunStatus::Running,
                RunStateV1::Paused => InferenceRunStatus::Paused,
                RunStateV1::Finished => InferenceRunStatus::Finished,
                RunStateV1::Cancelled => InferenceRunStatus::Cancelled,
                RunStateV1::Failed => InferenceRunStatus::Failed,
            },
            continuations: record
                .continuations
                .iter()
                .map(|visit| InferenceContinuation {
                    device: visit.device.clone(),
                    at_token: visit.at_token,
                    reason: visit.reason.clone(),
                    first_differing_token: visit.first_differing_token,
                    compared_token_ids_hash: visit.compared_token_ids_hash.clone(),
                })
                .collect(),
            error: record.view.error.clone(),
        })
    }
}
// The all-zero device address is reserved for Eidetic blob sealing. It
// separates content-hash AAD from every installed device's log address.
impl<B: Backend + Clone + Send + Sync + 'static> eidetic::PayloadSealer for PersonaRunJournal<B> {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        let address = RunExt {
            space: self.policy.space,
            device: [0; 32],
            run: *hash.as_bytes(),
            authority: Vec::new(),
        };
        let mut nonce = [0; 24];
        OsRng.fill_bytes(&mut nonce);
        let bytes = self
            .crypt(clear, nonce, &address, true)
            .map_err(eidetic::Error::new)?;
        let sealed = serde_json::to_vec(&Sealed { nonce, bytes })
            .map_err(|error| eidetic::Error::new(error.to_string()))?;
        Ok((
            sealed,
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([1; 16]),
                format: eidetic::seal::SEAL_FORMAT_V1.into(),
            },
        ))
    }
    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        sealed: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if marker.epoch != eidetic::SealEpochId([1; 16])
            || marker.format != eidetic::seal::SEAL_FORMAT_V1
        {
            return Err(eidetic::Error::new(
                "unsupported inference blob seal epoch or format",
            ));
        }
        let sealed: Sealed = serde_json::from_slice(sealed)
            .map_err(|error| eidetic::Error::new(error.to_string()))?;
        let address = RunExt {
            space: self.policy.space,
            device: [0; 32],
            run: *hash.as_bytes(),
            authority: Vec::new(),
        };
        self.crypt(&sealed.bytes, sealed.nonce, &address, false)
            .map_err(eidetic::Error::new)
    }
}

impl<B: Backend + Clone + Send + Sync + 'static> RunJournal for PersonaRunJournal<B> {
    fn execution_permitted(&self) -> bool {
        self.policy
            .group
            .as_ref()
            .is_none_or(|group| group.can_write())
    }
    fn remote_revision(&self) -> u64 {
        self.sync_status()
            .map_or(0, |status| status.ops_received)
            .wrapping_add(
                self.policy
                    .group
                    .as_ref()
                    .map_or(0, |group| group.revision()),
            )
    }
    fn device(&self) -> &str {
        &self.device
    }
    fn load(&self) -> Result<Vec<RunRecord>, String> {
        self.import_local()?;
        let _writer = self.writer.lock().unwrap();
        let heads = self.fold()?;
        let runs = heads.values().map(|(_, record)| record.clone()).collect();
        *self.heads.lock().unwrap() = heads;
        Ok(runs)
    }
    fn append(&self, record: &RunRecord) -> Result<(), String> {
        check_answer_contract(record)?;
        let _writer = self.writer.lock().unwrap();
        let mut heads = self.heads.lock().unwrap();
        let previous = heads.get(&record.view.run);
        let mut delta = Delta {
            required_semantics: required_semantics(record).map(str::to_string),
            parent: previous.map(|(id, _)| id.clone()),
            record: Some(record.clone()),
            tokens: Vec::new(),
            text: String::new(),
            draws: record.checkpoint.as_ref().map_or(0, |cp| cp.draws),
            complete: record.checkpoint.as_ref().is_some_and(|cp| cp.complete),
            evidence: None,
            evidence_schema: None,
            answer_support: None,
            state: record.view.state,
        };
        if let Some((_, previous)) = previous {
            if previous.view.recall != record.view.recall {
                return Err("retained recall passages changed during an ask".into());
            }
            if previous.view.question != record.view.question
                || previous.request != record.request
                || record.continuations.len() < previous.continuations.len()
                || !previous
                    .continuations
                    .iter()
                    .zip(&record.continuations)
                    .all(|(a, b)| {
                        a.device == b.device && a.at_token == b.at_token && a.reason == b.reason
                    })
                || previous.checkpoint.as_ref().is_some_and(|old| {
                    record.checkpoint.as_ref().is_none_or(|new| {
                        old.session != new.session
                            || old.request != new.request
                            || !new.token_ids.starts_with(&old.token_ids)
                    })
                })
            {
                return Err("stale or incompatible inference continuation; refresh the retained run before resuming".into());
            }
            if let (Some(old), Some(new)) = (&previous.checkpoint, &record.checkpoint) {
                if old.request == new.request
                    && old.session == new.session
                    && old.prompt_ids == new.prompt_ids
                    && new.token_ids.starts_with(&old.token_ids)
                    && new.text.starts_with(&old.text)
                    && previous.continuations == record.continuations
                    && previous.pause_reason == record.pause_reason
                    && record.view.state == RunStateV1::Running
                {
                    delta.record = None;
                    delta.tokens = new.token_ids[old.token_ids.len()..].to_vec();
                    delta.text = new.text[old.text.len()..].to_string();
                }
            }
        }
        if delta.record.is_some() {
            if let Some(facts) = record
                .view
                .recall
                .as_ref()
                .and_then(|r| r.answer_contract.as_ref())
                .and_then(|c| c.requested_facts.as_ref())
            {
                self.save_evidence(facts)?;
            }
        }
        if delta.record.is_some() || delta.complete {
            delta.evidence = Some(Self::evidence(record)?);
            delta.evidence_schema =
                Some(<InferenceRunRecord as eidetic::TypedPayload>::schema_ref());
        }
        if record.view.state == RunStateV1::Finished {
            if let Some(recall) = &record.view.recall {
                if let Some(support) =
                    recall.assess_answer(&record.view.text, &record.view.question)
                {
                    delta.answer_support = Some(self.save_evidence(&support)?);
                }
            }
        }
        let address = RunExt {
            space: self.policy.space,
            device: self.device_hash,
            run: *Hash::digest(record.view.run.as_bytes()).as_bytes(),
            authority: self.authority.clone(),
        };
        let clear = Zeroizing::new(serde_json::to_vec(&delta).map_err(|error| error.to_string())?);
        let body = if let Some(group) = &self.policy.group {
            let key = self.key.lock().unwrap();
            if key.is_none() {
                return Err("inference data is locked".into());
            }
            group.seal(&address, &clear)?
        } else {
            let mut nonce = [0; 24];
            OsRng.fill_bytes(&mut nonce);
            let sealed = Sealed {
                nonce,
                bytes: self.crypt(&clear, nonce, &address, true)?,
            };
            serde_json::to_vec(&sealed).map_err(|error| error.to_string())?
        };
        if body.len() > 1024 * 1024 {
            return Err("checkpoint exceeds the one MiB operation cap".into());
        }
        if delta.evidence.is_some() {
            self.materialize_evidence(record)?;
        }
        let latest = pollster::block_on(
            self.store
                .get_latest_entry(&self.policy.author, &(address.run, address.device)),
        )
        .map_err(|error| error.to_string())?;
        let sequence = latest.as_ref().map_or(Ok(0), |op| {
            op.header
                .seq_num
                .checked_add(1)
                .ok_or("checkpoint log sequence exhausted")
        })?;
        let header = Header::builder()
            .body(&body)
            .seq_num(sequence)
            .backlink(latest.map(|op| op.hash))
            .build(&self.signing, address);
        // A locally built header must also survive the wire/storage codec.
        // Reject unsupported extension sizes before committing a poisoned log.
        let header =
            Header::<RunExt>::decode(&header.encode()).map_err(|error| error.to_string())?;
        let op = Operation {
            hash: header.hash(),
            header,
            body: Some(Body::from_bytes(&body)),
        };
        pollster::block_on(self.accept(&op))?;
        heads.insert(
            record.view.run.clone(),
            (op.hash.to_string(), record.clone()),
        );
        // A disconnected live lane cannot undo a durable commit. LogSync
        // catches up from this same store after reconnection.
        if let Some(joined) = self.joined.lock().unwrap().as_ref() {
            let _ = joined.publish(op);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::MemoryBackend;
    fn journal(device: &str) -> PersonaRunJournal<MemoryBackend> {
        PersonaRunJournal::new(
            MemoryBackend::new(),
            [7; 32],
            &Ed25519Keypair::from_seed([2; 32]),
            device.into(),
            [9; 32],
        )
    }
    #[test]
    fn claim_support_is_sealed_reopened_and_bound_to_raw_output() {
        let journal = journal("a");
        let mut record = paused();
        record.view.state = RunStateV1::Queued;
        let mut evidence = recall::RecallEvidence {
            answer_format: recall::AnswerFormat::ClaimSupportV1,
            answer_contract: None,
            support: None,
            embedding: None,
            owner: None,
            scope: vec![],
            config: Default::default(),
            passages: vec![],
            stale: vec![],
            citations: vec![],
            unknown_citations: vec![],
        };
        let (contract, prompt, context) =
            super::super::answer_contract::prepare(&record.view.question, &evidence).unwrap();
        evidence.answer_contract = Some(contract.clone());
        record.request.output_grammar =
            super::super::answer_contract::grammar(&evidence, &contract.method).unwrap();
        record.request.prompt = prompt;
        record.request.context = context;
        record.view.recall = Some(evidence);
        assert_eq!(
            required_semantics(&record),
            Some("recall/quoted-candidate-review-v1")
        );
        journal.append(&record).unwrap();
        assert!(
            journal
                .retained_answer_support(&record.view.run)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            journal.retained_requested_facts(&record.view.run).unwrap(),
            contract.requested_facts
        );
        record.view.state = RunStateV1::Finished;
        record.view.text = serde_json::json!({"claims":[{"fact":contract.requested_facts.as_ref().unwrap().facts[0],"review":{"candidates":[],"agreement":"none"},"status":"insufficient","text":format!("The supplied evidence does not state {}.",contract.requested_facts.as_ref().unwrap().facts[0].text),"evidence":[]}]}).to_string();
        journal.append(&record).unwrap();
        let support = journal
            .retained_answer_support(&record.view.run)
            .unwrap()
            .unwrap();
        assert!(support.error.is_none());
        assert_eq!(
            support.claims[0].review.as_ref().unwrap().candidates.len(),
            0
        );
        assert_eq!(
            support.claims[0].review.as_ref().unwrap().agreement,
            eidetic::models::answer::CandidateAgreement::None
        );
        assert_eq!(
            support.raw_output,
            eidetic::ManifestId::of_blob(record.view.text.as_bytes())
        );
        let mut backend = journal.store.backend().clone();
        let manifest =
            pollster::block_on(eidetic::manifest::load_manifest(&mut backend, support.id()))
                .unwrap()
                .unwrap();
        assert!(eidetic::seal_marker(&manifest).unwrap().is_some());
        let reopened = PersonaRunJournal::new(
            backend,
            [7; 32],
            &Ed25519Keypair::from_seed([2; 32]),
            "a".into(),
            [9; 32],
        );
        assert_eq!(
            reopened.retained_answer_support(&record.view.run).unwrap(),
            Some(support.clone())
        );
        let restored = reopened.load().unwrap();
        assert_eq!(
            restored[0].view.recall.as_ref().unwrap().answer_contract,
            Some(contract.clone())
        );
        assert_eq!(
            reopened.retained_requested_facts(&record.view.run).unwrap(),
            contract.requested_facts
        );
        let mut altered = record.clone();
        altered
            .view
            .recall
            .as_mut()
            .unwrap()
            .answer_contract
            .as_mut()
            .unwrap()
            .requested_facts
            .as_mut()
            .unwrap()
            .facts[0]
            .text
            .push_str(" altered");
        assert!(reopened.append(&altered).is_err());
        let mut altered = record.clone();
        altered
            .view
            .recall
            .as_mut()
            .unwrap()
            .answer_contract
            .as_mut()
            .unwrap()
            .requested_facts = None;
        assert!(reopened.append(&altered).is_err());
        let mut altered = record.clone();
        altered
            .request
            .context
            .push_str("different generation evidence");
        assert!(reopened.append(&altered).is_err());
        let mut altered = record.clone();
        altered.request.output_grammar = None;
        assert!(reopened.append(&altered).is_err());
        let mut altered = record.clone();
        altered.checkpoint = Some(GenerationCheckpoint {
            placements: vec![],
            session: None,
            request: GenerationRequest::default(),
            prompt_ids: vec![],
            token_ids: vec![],
            draws: 0,
            text: String::new(),
            complete: false,
        });
        assert!(check_answer_contract(&altered).is_err());
        reopened.lock();
        assert!(reopened.retained_requested_facts(&record.view.run).is_err());
        assert!(reopened.retained_answer_support(&record.view.run).is_err());
        reopened.unlock([9; 32]);
        assert_eq!(
            reopened.retained_answer_support(&record.view.run).unwrap(),
            Some(support)
        );
    }
    #[test]
    fn recall_entries_refuse_legacy_readers_instead_of_losing_context() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct LegacyDelta {
            parent: Option<String>,
            record: Option<serde_json::Value>,
            tokens: Vec<u32>,
            text: String,
            draws: u64,
            complete: bool,
            state: RunStateV1,
            #[serde(default)]
            evidence: Option<serde_json::Value>,
            #[serde(default)]
            evidence_schema: Option<serde_json::Value>,
        }
        let delta = Delta {
            required_semantics: Some("recall/v1".into()),
            parent: None,
            record: None,
            tokens: vec![],
            text: String::new(),
            draws: 0,
            complete: false,
            state: RunStateV1::Queued,
            evidence: None,
            evidence_schema: None,
            answer_support: None,
        };
        let encoded = serde_json::to_vec(&delta).unwrap();
        assert!(
            serde_json::from_slice::<LegacyDelta>(&encoded)
                .err()
                .unwrap()
                .to_string()
                .contains("required_semantics")
        );
        let current: Delta = serde_json::from_slice(&encoded).unwrap();
        current.check_semantics().unwrap();
        let mut ordinary = delta;
        ordinary.required_semantics = None;
        let encoded = serde_json::to_vec(&ordinary).unwrap();
        serde_json::from_slice::<LegacyDelta>(&encoded).unwrap();
        let mut unsupported: Delta = serde_json::from_slice(&encoded).unwrap();
        unsupported.required_semantics = Some("future/v9".into());
        assert!(unsupported.check_semantics().is_err());
        let mut record = paused();
        record.request.context = "retained context".into();
        unsupported.required_semantics = None;
        unsupported.record = Some(record);
        assert!(unsupported.check_semantics().is_err());
    }
    fn paused() -> RunRecord {
        RunRecord {
            weight_format: None,
            view: RunV1 {
                run: "private-run".into(),
                question: "secret question".into(),
                state: RunStateV1::Paused,
                text: String::new(),
                error: None,
                session: None,
                tokens: 0,
                continuations: Vec::new(),
                recall: None,
            },
            actor_id: 0,
            request: GenerationRequest {
                prompt: "secret question".into(),
                ..Default::default()
            },
            checkpoint: None,
            continuations: vec![Continuation {
                device: "a".into(),
                at_token: 0,
                reason: "ask".into(),
                first_differing_token: None,
                compared_token_ids_hash: None,
            }],
            pause_reason: Some("user".into()),
            executing: false,
        }
    }
    #[test]
    fn encrypted_logs_refuse_wrong_keys_signers_and_modified_bodies() {
        let a = journal("a");
        a.append(&paused()).unwrap();
        let mut backend = a.store.backend().clone();
        let question = eidetic::ManifestId::of_blob(b"secret question");
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(&mut backend, question))
            .unwrap()
            .unwrap();
        assert!(eidetic::seal_marker(&manifest).unwrap().is_some());
        assert!(
            pollster::block_on(eidetic::typed::load_typed::<eidetic::models::OpaqueBlob>(
                &mut backend,
                &mut eidetic::NoFetcher,
                question
            ))
            .is_err(),
            "private evidence cannot be read without the sealer"
        );
        let operations = pollster::block_on(a.operations()).unwrap();
        let op = &operations[0];
        let encoded = op.body.as_ref().unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&encoded).contains("secret question"));
        let b = journal("b");
        assert!(pollster::block_on(b.accept(op)).unwrap());
        b.unlock([8; 32]);
        assert!(b.load().err().unwrap().contains("authentication"));
        b.unlock([9; 32]);
        assert_eq!(b.load().unwrap()[0].view.question, "secret question");
        let foreign = PersonaRunJournal::new(
            MemoryBackend::new(),
            [7; 32],
            &Ed25519Keypair::from_seed([3; 32]),
            "c".into(),
            [9; 32],
        );
        assert!(
            pollster::block_on(foreign.accept(op))
                .unwrap_err()
                .contains("wrong-persona")
        );
        let mut tampered = op.clone();
        tampered.body = Some(Body::from_bytes(b"changed ciphertext"));
        assert!(pollster::block_on(b.accept(&tampered)).is_err());
        b.lock();
        assert!(b.load().is_err());
    }
    #[test]
    fn two_continuations_are_a_visible_fork_and_never_a_silent_winner() {
        let a = journal("a");
        a.append(&paused()).unwrap();
        let operations = pollster::block_on(a.operations()).unwrap();
        let b = journal("b");
        let c = journal("c");
        for op in &operations {
            pollster::block_on(b.accept(op)).unwrap();
            pollster::block_on(c.accept(op)).unwrap();
        }
        for (journal, device) in [(&b, "b"), (&c, "c")] {
            let mut record = journal.load().unwrap().remove(0);
            record.continuations.push(Continuation {
                device: device.into(),
                at_token: 0,
                reason: "resume".into(),
                first_differing_token: None,
                compared_token_ids_hash: None,
            });
            record.view.state = RunStateV1::Queued;
            record.pause_reason = None;
            journal.append(&record).unwrap();
        }
        for op in pollster::block_on(c.operations()).unwrap() {
            pollster::block_on(b.accept(&op)).unwrap();
        }
        assert!(b.load().err().unwrap().contains("concurrent continuations"));
    }
}
