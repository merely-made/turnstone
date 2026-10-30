// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Structured evaluation over the complete admitted source snapshot.
use crate::research::{self, Target};
use crate::research_records::*;
use crate::trail_memory::{TrailCommand, record_actor};
use eidetic::models::recall::PassageSelector;
use eidetic::records::{
    Evidence, LifecycleObservation, LifecycleState, PropertyRecordV1, RecordSnapshot, SourceRecord,
    TagIdentity,
};
use eidetic_search::query::{Coverage, ProducerKind, SearchSettings};
use std::sync::mpsc;
use std::time::Duration;

const MANUAL: &str = "https://merely.computer/ns/research#manual-property-v1";
fn current(bridge: &research::Bridge, scope: &ScopeToken) -> Result<(), String> {
    if bridge.context_revision()? != (scope.context.clone(), scope.revision)
        || (bridge.clip_access()?.epoch)() != Some(scope.custody_epoch)
    {
        return Err("Structured research admission or custody changed".into());
    }
    Ok(())
}
fn command<T>(
    trail: &armillary::ActorHandle<TrailCommand>,
    scope: &ScopeToken,
    make: impl FnOnce(mpsc::SyncSender<Result<T, String>>) -> record_actor::Kind,
) -> Result<T, String> {
    let (send, receive) = mpsc::sync_channel(1);
    if !trail.command(TrailCommand::ResearchRecords(record_actor::Command {
        session: *scope.context.session.as_uuid(),
        generation: scope.context.generation,
        kind: make(send),
    })) {
        return Err("Structured record owner unavailable".into());
    }
    receive
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| "Structured record owner did not answer")?
}
fn sources(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<TrailCommand>,
    scope: &ScopeToken,
    selected: Option<Vec<crate::research_search::Member>>,
    max_sources: usize,
    max_bytes: usize,
    except: Option<uuid::Uuid>,
) -> Result<eidetic_search::query::ResearchSnapshot, String> {
    current(bridge, scope)?;
    let bindings = scope
        .graph_manifests
        .iter()
        .map(|(graph, manifest)| (*graph.as_uuid(), *manifest))
        .collect();
    command(trail, scope, |reply| {
        record_actor::Kind::Pending(
            bindings,
            scope.settings.max_records,
            max_bytes,
            except,
            reply,
        )
    })?;
    let mut settings = SearchSettings::default();
    settings.max_sources = max_sources;
    settings.max_corpus_chars = max_bytes;
    settings.max_metadata_chars = max_bytes;
    let request = crate::research_search::Request {
        id: uuid::Uuid::new_v4(),
        context: scope.context.clone(),
        revision: scope.revision,
        epoch: scope.custody_epoch,
        query_revision: 0,
        selected,
        query: Default::default(),
        settings,
    };
    let snapshot = crate::research_search::snapshot(bridge, trail, &request)?;
    if !matches!(snapshot.coverage, Coverage::Complete) {
        return Err("Structured input is incomplete".into());
    }
    let total = snapshot
        .sources
        .iter()
        .try_fold(0usize, |total, source| {
            total.checked_add(source.retained.text.len())
        })
        .ok_or("Structured source size overflow")?;
    if total > max_bytes {
        return Err("Structured source byte limit exceeded".into());
    }
    current(bridge, scope)?;
    Ok(snapshot)
}
fn validate_evidence(
    record: &PropertyRecordV1,
    source: &eidetic_search::query::ResearchSource,
) -> Result<(), String> {
    if record.evidence.source() != &source.retained.source
        || record.evidence.version() != source.retained.version
    {
        return Err("Property evidence version changed".into());
    }
    match &record.evidence {
        Evidence::Passage { hash, selector, .. } => {
            if *hash != eidetic::Hash::of(source.retained.text.as_bytes())
                || selector.start >= selector.end
                || selector.end > source.retained.text.chars().count()
            {
                return Err("Property evidence selector or digest is invalid".into());
            }
            Ok(())
        },
        Evidence::Field { .. } => Err("Typed field evidence is unsupported by this host".into()),
    }
}
fn validate_saved_inputs(
    trail: &armillary::ActorHandle<TrailCommand>,
    scope: &ScopeToken,
    inputs: &eidetic_search::query::ResearchSnapshot,
    value: &eidetic::records::RecordResult,
) -> Result<(), String> {
    for source in &value.snapshot.sources {
        if !inputs.sources.iter().any(|held| {
            held.retained.source == source.source && held.retained.version == source.version
        }) {
            return Err("Saved input is no longer selected and admitted".into());
        }
    }
    for record in &value.snapshot.records {
        let source = inputs
            .sources
            .iter()
            .find(|held| held.retained.source == *record.evidence.source())
            .ok_or("Saved evidence is no longer admitted")?;
        validate_evidence(record, source)?;
        let mut observations = value
            .snapshot
            .lifecycle
            .iter()
            .filter(|observation| observation.record_id == record.id);
        let observation = observations
            .next()
            .ok_or("Saved property lifecycle unavailable")?;
        if observations.next().is_some() {
            return Err("Saved property lifecycle is ambiguous".into());
        }
        let (session, graph, statement, manifest) = parse_lifecycle(&observation.lifecycle_ref)?;
        if session != *scope.context.session.as_uuid() {
            return Err("Saved property belongs to another session".into());
        }
        let owner = scope
            .owner_bindings
            .iter()
            .find(|owner| {
                *owner.binding.graph.as_uuid() == graph
                    && owner.binding.statement_id == statement
                    && owner.binding.manifest == manifest
                    && owner.binding.entity.to_string() == record.entity
            })
            .ok_or("Saved property assertion is unavailable")?;
        if owner.author != record.author {
            return Err("Saved property attribution changed".into());
        }
        let (held_record, definition) = command(trail, scope, |reply| {
            record_actor::Kind::ReadRecord(manifest, reply)
        })?;
        if held_record != *record || !value.snapshot.definitions.contains(&definition) {
            return Err("Saved property payload is unavailable or changed".into());
        }
    }
    Ok(())
}
fn lifecycle(scope: &ScopeToken, binding: &Binding) -> Result<String, String> {
    serde_json::to_string(&(
        *scope.context.session.as_uuid(),
        *binding.graph.as_uuid(),
        &binding.statement_id,
        binding.manifest,
    ))
    .map_err(|e| e.to_string())
}
pub(crate) fn parse_lifecycle(
    value: &str,
) -> Result<(uuid::Uuid, uuid::Uuid, String, eidetic::ManifestId), String> {
    let decoded: (uuid::Uuid, uuid::Uuid, String, eidetic::ManifestId) =
        serde_json::from_str(value).map_err(|_| "Invalid property lifecycle reference")?;
    if decoded.2.trim().is_empty()
        || serde_json::to_string(&decoded).map_err(|e| e.to_string())? != value
    {
        return Err("Invalid property lifecycle identity".into());
    }
    Ok(decoded)
}
fn query(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<TrailCommand>,
    request: &QueryRequest,
) -> Result<eidetic::records::RecordResult, String> {
    let inputs = sources(
        bridge,
        trail,
        &request.scope,
        request.selected.clone(),
        request.settings.max_sources,
        request.settings.max_input_bytes,
        None,
    )?;
    if request.candidates.len() > request.settings.max_records {
        return Err("Structured record limit exceeded".into());
    }
    let mut snapshot = RecordSnapshot {
        scope_generation: serde_json::to_string(&(
            &request.scope.context.generation,
            request.scope.revision,
            &request.scope.graphs,
            &request.scope.graph_instances,
            request.scope.custody_epoch,
            &request.scope.selected,
        ))
        .map_err(|e| e.to_string())?,
        complete: true,
        exclusions: vec![],
        sources: vec![],
        definitions: vec![],
        records: vec![],
        lifecycle: vec![],
    };
    for source in &inputs.sources {
        let producer = match source.producer {
            ProducerKind::Capture => "capture",
            ProducerKind::Selection => "selection",
            ProducerKind::Note => "note",
            ProducerKind::Other => "other",
        };
        snapshot.sources.push(SourceRecord {
            source: source.retained.source.clone(),
            version: source.retained.version.clone(),
            producer: producer.into(),
            captured_at: source.date.map(|ms| ms.div_euclid(1000)),
            tags: source
                .tags
                .iter()
                .map(|tag| TagIdentity {
                    owner: tag.owner.clone(),
                    concept: tag.concept.clone(),
                })
                .collect(),
        });
    }
    let mut projections = std::collections::BTreeMap::new();
    for candidate in &request.candidates {
        let key = (
            *candidate.binding.graph.as_uuid(),
            candidate.binding.statement_id.clone(),
        );
        if let Some(previous) = projections.insert(key, candidate) {
            if previous != candidate {
                return Err("Conflicting property assertion projection".into());
            }
        }
    }
    let mut nonce_bindings = std::collections::BTreeMap::new();
    for candidate in projections.into_values() {
        let (record, definition) = command(trail, &request.scope, |reply| {
            record_actor::Kind::ReadRecord(candidate.binding.manifest, reply)
        })?;
        if record.entity != candidate.binding.entity.to_string()
            || record.author != candidate.author
            || record.method != MANUAL
        {
            return Err("Property assertion and sealed record disagree".into());
        }
        let Some(source) = inputs
            .sources
            .iter()
            .find(|source| source.retained.source == *record.evidence.source())
        else {
            continue;
        };
        validate_evidence(&record, source)?;
        if nonce_bindings
            .insert(record.id.clone(), &candidate.binding)
            .is_some()
        {
            return Err("Ambiguous property nonce across distinct graph assertions".into());
        }
        if !snapshot.definitions.iter().any(|d| d == &definition) {
            snapshot.definitions.push(definition);
        }
        snapshot.lifecycle.push(LifecycleObservation {
            record_id: record.id.clone(),
            lifecycle_ref: lifecycle(&request.scope, &candidate.binding)?,
            state: LifecycleState::Active,
        });
        snapshot.records.push(record);
    }
    current(bridge, &request.scope)?;
    eidetic_search::records::evaluate(&snapshot, &request.query, &request.settings)
        .map_err(|e| format!("Structured query refused: {e:?}"))
}
fn bounded_json_size(value: &impl serde::Serialize, limit: usize) -> Result<usize, String> {
    struct Counter {
        used: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.used = self
                .used
                .checked_add(bytes.len())
                .filter(|size| *size <= self.limit)
                .ok_or_else(|| std::io::Error::other("structured input limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { used: 0, limit };
    serde_json::to_writer(&mut counter, value)
        .map_err(|_| "Structured author input byte limit exceeded".to_string())?;
    Ok(counter.used)
}
fn author_input_size(request: &AuthorRequest) -> Result<usize, String> {
    let property = &request.property;
    bounded_json_size(
        &property.definition,
        request.scope.settings.max_validation_bytes,
    )?;
    bounded_json_size(
        &(
            &property.definition,
            &property.value,
            &property.context,
            &property.valid_time,
            &property.status,
            &property.evidence,
            &request.author,
        ),
        request.scope.settings.max_input_bytes,
    )
}
pub(crate) fn record_request(
    request: &AuthorRequest,
) -> Result<record_actor::DepositRequest, String> {
    author_input_size(request)?;
    let mut target = request.property.evidence.clone();
    let selector = target.selector().clone().ok_or("Select an exact passage")?;
    if selector.start >= selector.end {
        return Err("Select a nonempty exact passage".into());
    }
    let hash =
        eidetic::Hash::parse(target.text_hash()).map_err(|_| "Invalid retained text digest")?;
    match &mut target {
        Target::Capture(t) => t.selector = None,
        Target::Clip(t) => t.selector = None,
    }
    let source = research::source_id(&target)?;
    let definition = request.property.definition.clone();
    let record = PropertyRecordV1 {
        id: request.id.to_string(),
        entity: request.property.entity.to_string(),
        property: definition.id.clone(),
        schema_revision: definition.revision.clone(),
        definition_manifest: definition.manifest_id().map_err(|e| e.to_string())?,
        context: request.property.context.clone(),
        value: request.property.value.clone(),
        valid_time: request.property.valid_time.clone(),
        status: request.property.status.clone(),
        author: request.author.clone(),
        method: MANUAL.into(),
        evidence: Evidence::Passage {
            source,
            version: target.text_hash().to_owned(),
            hash,
            selector: PassageSelector {
                start: selector.start,
                end: selector.end,
            },
        },
        recorded_at: Some(request.recorded_at),
    };
    bounded_json_size(
        &(&definition, &record),
        request.scope.settings.max_input_bytes,
    )?;
    record
        .validate(&definition)
        .map_err(|e| format!("Property refused: {e:?}"))?;
    Ok(record_actor::DepositRequest {
        nonce: request.id,
        session: *request.scope.context.session.as_uuid(),
        graph: *request.property.graph.as_uuid(),
        generation: request.scope.context.generation,
        definition,
        record,
    })
}
pub(crate) fn result_request(
    request: &QueryRequest,
    manifest: eidetic::ManifestId,
) -> Result<record_actor::ResultRequest, String> {
    let (graph, entity) = request
        .result_anchor
        .ok_or("Saved result anchor unavailable")?;
    Ok(record_actor::ResultRequest {
        nonce: request.id,
        session: *request.scope.context.session.as_uuid(),
        graph: *graph.as_uuid(),
        entity,
        generation: request.scope.context.generation,
        manifest,
    })
}
#[path = "research_records_dispatch.rs"]
mod dispatch;
pub(crate) use dispatch::execute;
