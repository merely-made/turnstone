// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::research::{Bridge, ClipAccess, Context};
use crate::research_records::{Binding, Candidate, QueryRequest, ScopeToken};
use crate::research_search::Member;
use eidetic::records::{
    Cardinality, Evidence, PropertyDefinitionV1, PropertyRecordV1, PropertyValue, RecordFilters,
    RecordOperation, RecordQuery, RecordSettings, ValueType,
};
use std::sync::{Arc, mpsc};

fn actor_call<T>(
    actor: &armillary::ActorHandle<TrailCommand>,
    session: uuid::Uuid,
    generation: uuid::Uuid,
    make: impl FnOnce(mpsc::SyncSender<Result<T, String>>) -> record_actor::Kind,
) -> Result<T, String> {
    let (reply, receive) = mpsc::sync_channel(1);
    assert!(
        actor.command(TrailCommand::ResearchRecords(record_actor::Command {
            session,
            generation,
            kind: make(reply)
        }))
    );
    receive.recv_timeout(Duration::from_secs(5)).unwrap()
}

#[test]
fn model_free_owner_enumeration_deduplicates_projection_and_refuses_ambiguous_nonce() {
    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let graph_id = crate::panes::GraphId::new();
    let directory = root.path().join(session.as_uuid().to_string());
    let memory = directory.join("memory");
    std::fs::create_dir_all(&memory).unwrap();
    let mut legacy = FjallStore::open(&memory).unwrap();
    let html = "<html><main>Ferry leaves at 09:30</main></html>";
    let capture = capture_source_document_in(
        &mut legacy,
        &crate::content::FetchedDocument {
            bytes: html.as_bytes().to_vec(),
            body: html.into(),
            content_type: Some("text/html".into()),
            effective_url: Some("https://fixture.test/ferry".into()),
            acquired_at_ms: 1000,
        },
    )
    .unwrap();
    drop(legacy);
    let admission = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            session,
            graph: graph_id,
            node: uuid::Uuid::new_v4(),
            annotation_manifest: capture.annotation_manifest,
        },
        occurrence: 0,
        title: "Ferry".into(),
        url: "https://fixture.test/ferry".into(),
        error: None,
    };
    let unavailable = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            annotation_manifest: eidetic::ManifestId::of_blob(b"missing").to_string(),
            ..admission.key.clone()
        },
        title: "Missing".into(),
        ..admission.clone()
    };
    let generation = uuid::Uuid::new_v4();
    let context = Context {
        session,
        generation,
        admissions: vec![admission.clone(), unavailable],
        clips: vec![],
    };
    let custody = custody_test_fixture::custody();
    let (actor, _) = spawn_trail_for_session(
        Arc::new(|| {}),
        Some(memory),
        Default::default(),
        generation,
    );
    assert!(actor.command(TrailCommand::SetCustody(Some(custody.clone()))));
    let (ready, received) = mpsc::sync_channel(1);
    assert!(actor.command(TrailCommand::CustodyReady(ready)));
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    bridge.access(Some(ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    let (_, revision) = bridge.context_revision().unwrap();
    let selected = Some(vec![Member::Capture(admission.key.clone())]);
    let mut graph = mere::kernel::graph::Graph::new();
    let node = mere::kernel::graph::apply::add_node(
        &mut graph,
        Some(uuid::Uuid::new_v4()),
        "https://fixture.test/entity".into(),
        Default::default(),
    );
    let entity = graph.shown_resource_id(node).unwrap();
    let settings = RecordSettings::default();
    let mut scope = ScopeToken {
        context: context.clone(),
        revision,
        custody_epoch: 1,
        graphs: vec![(graph_id, graph.revision())],
        graph_instances: vec![(graph_id, uuid::Uuid::new_v4())],
        selected: selected.clone(),
        graph_manifests: vec![],
        owner_bindings: vec![],
        settings: settings.clone(),
    };
    let lexical = crate::research_search::Request {
        id: uuid::Uuid::new_v4(),
        context: context.clone(),
        revision,
        epoch: 1,
        query_revision: 1,
        selected: selected.clone(),
        query: Default::default(),
        settings: eidetic_search::query::SearchSettings {
            limit: 1,
            ..Default::default()
        },
    };
    let sources = crate::research_search::snapshot(&bridge, &actor, &lexical).unwrap();
    let source = &sources.sources[0].retained;
    let definition = PropertyDefinitionV1 {
        id: "fixture.departure".into(),
        revision: "1".into(),
        value_type: ValueType::Integer,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec!["approved".into()],
    };
    let mut evidence = crate::research::target(&source.source, &source.version).unwrap();
    evidence.set_selector(crate::saved_sources::Selector { start: 0, end: 5 });
    let mut limited_scope = scope.clone();
    limited_scope.settings.max_input_bytes = 1024;
    let author = crate::research_records::AuthorRequest {
        id: uuid::Uuid::new_v4(),
        scope: limited_scope,
        selected: selected.clone(),
        property: crate::research_records::ManualProperty {
            graph: graph_id,
            entity,
            definition: definition.clone(),
            value: PropertyValue::Integer(930),
            context: "x".repeat(2048),
            valid_time: None,
            status: Some("approved".into()),
            evidence,
        },
        author: graph.write_author().asserter_iri(),
        recorded_at: 1,
    };
    let refused = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::Deposit(author),
    );
    assert!(
        matches!(refused, crate::research_records::JobResult::Deposit { result: Err(error), .. }
        if error.contains("author input byte limit"))
    );
    let persistence = crate::session_persistence::SessionPersistence::<
        pandect::graph_placement::PlacementProfile,
    >::writable(directory.clone(), None)
    .unwrap();
    let graph_file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let mut candidates = Vec::new();
    for _ in 0..2 {
        let nonce = uuid::Uuid::new_v4();
        let record = PropertyRecordV1 {
            id: nonce.to_string(),
            entity: entity.to_string(),
            property: definition.id.clone(),
            schema_revision: definition.revision.clone(),
            definition_manifest: definition.manifest_id().unwrap(),
            context: "route".into(),
            value: PropertyValue::Integer(930),
            valid_time: None,
            status: Some("approved".into()),
            author: graph.write_author().asserter_iri(),
            method: "https://merely.computer/ns/research#manual-property-v1".into(),
            evidence: Evidence::Passage {
                source: source.source.clone(),
                version: source.version.clone(),
                hash: eidetic::Hash::of(source.text.as_bytes()),
                selector: eidetic::models::recall::PassageSelector { start: 0, end: 5 },
            },
            recorded_at: Some(1),
        };
        let deposit = record_actor::DepositRequest {
            nonce,
            session: *session.as_uuid(),
            graph: *graph_id.as_uuid(),
            generation,
            definition: definition.clone(),
            record,
        };
        let manifest = actor_call(&actor, *session.as_uuid(), generation, |reply| {
            record_actor::Kind::Deposit(deposit.clone(), reply)
        })
        .unwrap();
        let binding = crate::research_record_graph::attach(&mut graph, entity, manifest).unwrap();
        let assertion = crate::research_record_graph::assertion_for(&graph, &binding).unwrap();
        actor_call(&actor, *session.as_uuid(), generation, |reply| {
            record_actor::Kind::MarkAttached(
                deposit.clone(),
                manifest,
                binding.statement_id.clone(),
                assertion,
                reply,
            )
        })
        .unwrap();
        persistence
            .save_graph_with(
                &directory,
                &graph,
                || Ok(()),
                |placement| {
                    crate::session_graph_file::save_profiled(&graph_file, &graph, placement)
                },
            )
            .unwrap();
        actor_call(&actor, *session.as_uuid(), generation, |reply| {
            record_actor::Kind::Acknowledge(
                deposit.clone(),
                manifest,
                binding.statement_id.clone(),
                reply,
            )
        })
        .unwrap();
        candidates.push(Candidate {
            binding: Binding {
                graph: graph_id,
                entity,
                statement_id: binding.statement_id,
                manifest,
            },
            author: binding.author,
        });
    }
    scope.graphs[0].1 = graph.revision();
    scope.graph_manifests = candidates
        .iter()
        .map(|candidate| (graph_id, candidate.binding.manifest))
        .collect();
    scope.owner_bindings = candidates
        .iter()
        .map(|candidate| crate::research_records::OwnerBinding {
            binding: candidate.binding.clone(),
            author: candidate.author.clone(),
            withdrawn: false,
        })
        .collect();
    let mut request = QueryRequest {
        id: uuid::Uuid::new_v4(),
        scope: scope.clone(),
        selected: selected.clone(),
        query: RecordQuery {
            operation: RecordOperation::GroupValues,
            filters: RecordFilters::default(),
        },
        settings: settings.clone(),
        candidates: vec![
            candidates[0].clone(),
            candidates[0].clone(),
            candidates[1].clone(),
        ],
        result_anchor: None,
    };
    let result = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::Query(request.clone()),
    );
    let crate::research_records::JobResult::Query {
        result: Ok(result), ..
    } = result
    else {
        panic!("complete query refused")
    };
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].record_ids.len(), 2);
    assert_ne!(result.rows[0].record_ids[0], result.rows[0].record_ids[1]);
    let open = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::OpenSource {
            id: uuid::Uuid::new_v4(),
            scope: scope.clone(),
            generation: uuid::Uuid::new_v4(),
            source: source.source.clone(),
            version: source.version.clone(),
        },
    );
    assert!(
        matches!(open, crate::research_records::JobResult::OpenSource { result: Ok(target), .. } if target.text_hash() == source.version)
    );
    let record = result
        .snapshot
        .records
        .iter()
        .find(|record| record.id == result.rows[0].record_ids[0])
        .unwrap()
        .clone();
    let binding = candidates
        .iter()
        .find(|candidate| {
            candidate.binding.manifest
                == eidetic::ManifestId::of_blob(
                    &eidetic::TypedPayload::serialize_to_bytes(&record).unwrap(),
                )
        })
        .unwrap()
        .binding
        .clone();
    let open = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::OpenEvidence {
            id: uuid::Uuid::new_v4(),
            scope: scope.clone(),
            generation: None,
            binding,
            record,
        },
    );
    assert!(
        matches!(open, crate::research_records::JobResult::OpenEvidence { result: Ok(target), .. } if target.selector().as_ref().is_some_and(|selector| selector.start == 0 && selector.end == 5))
    );
    let mut copied = candidates[0].clone();
    copied.binding.graph = crate::panes::GraphId::new();
    request.candidates.push(copied);
    let result = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::Query(request.clone()),
    );
    assert!(
        matches!(result, crate::research_records::JobResult::Query { result: Err(error), .. } if error.contains("Ambiguous property nonce"))
    );
    request.candidates = candidates;
    request.selected = Some(vec![]);
    request.scope.selected = Some(vec![]);
    let empty = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::Query(request.clone()),
    );
    let crate::research_records::JobResult::Query {
        result: Ok(empty), ..
    } = empty
    else {
        panic!("empty selected set must evaluate without consulting unavailable sources")
    };
    assert!(empty.snapshot.sources.is_empty());
    assert!(empty.snapshot.records.is_empty());
    assert!(empty.rows.is_empty());
    request.selected = None;
    request.scope.selected = None;
    let result = super::super::research_records_worker::execute(
        &bridge,
        &actor,
        crate::research_records::Job::Query(request),
    );
    assert!(
        matches!(result, crate::research_records::JobResult::Query { result: Err(error), .. } if error.contains("incomplete"))
    );
}
