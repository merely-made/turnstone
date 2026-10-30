// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::trail_memory::sealed_record_receipts::{
    DepositRequest, acknowledge, deposit_request, mark_attached, read_receipt,
};
use eidetic::models::recall::{PassageSelector, RecallSourceId, RecallSourceKind};
use eidetic::records::{Cardinality, Evidence, PropertyRecordV1, PropertyValue, ValueType};
use std::sync::atomic::{AtomicBool, Ordering};

struct CountingSealer<'a> {
    inner: &'a Sealer,
    unseals: std::sync::atomic::AtomicUsize,
}
impl PayloadSealer for CountingSealer<'_> {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        self.inner.seal(hash, clear)
    }
    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        data: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        self.unseals.fetch_add(1, Ordering::SeqCst);
        self.inner.unseal(hash, marker, data)
    }
}

struct Sealer(AtomicBool);
impl PayloadSealer for Sealer {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        if !self.0.load(Ordering::SeqCst) {
            return Err(eidetic::Error::new("locked"));
        }
        let mut data = hash.to_hex().into_bytes();
        data.extend(clear.iter().map(|v| v ^ 0x73));
        Ok((
            data,
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([3; 16]),
                format: "fixture".into(),
            },
        ))
    }
    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        data: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if !self.0.load(Ordering::SeqCst)
            || marker.format != "fixture"
            || !data.starts_with(hash.to_hex().as_bytes())
        {
            return Err(eidetic::Error::new("refused"));
        }
        let clear: Vec<_> = data[64..].iter().map(|v| v ^ 0x73).collect();
        if eidetic::Hash::of(&clear) != *hash {
            return Err(eidetic::Error::new("invalid hash"));
        }
        Ok(clear)
    }
}

#[test]
fn model_free_record_reopens_only_with_exact_sealed_definition() {
    let root = tempfile::tempdir().unwrap();
    let sealer = Sealer(AtomicBool::new(true));
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let definition = PropertyDefinitionV1 {
        id: "fixture.departure".into(),
        revision: "1".into(),
        value_type: ValueType::Text,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec!["approved".into()],
    };
    let definition_id = deposit_definition(&mut store, &sealer, &definition).unwrap();
    let record = PropertyRecordV1 {
        id: "distinct-nonce-1".into(),
        entity: "entity-1".into(),
        property: definition.id.clone(),
        schema_revision: definition.revision.clone(),
        definition_manifest: definition_id,
        context: "route-1".into(),
        value: PropertyValue::Text("PRIVATE-DEPARTURE".into()),
        valid_time: None,
        status: Some("approved".into()),
        author: "owner".into(),
        method: "manual".into(),
        evidence: Evidence::Passage {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: Some("owner".into()),
                source: "retained-source".into(),
                node: None,
            },
            version: "v1".into(),
            hash: eidetic::Hash::of(b"passage"),
            selector: PassageSelector { start: 0, end: 7 },
        },
        recorded_at: Some(100),
    };
    let id = deposit_record(&mut store, &sealer, &record).unwrap();
    assert_eq!(id, deposit_record(&mut store, &sealer, &record).unwrap());
    for key in pollster::block_on(store.list("")).unwrap() {
        let bytes = pollster::block_on(store.get(&key)).unwrap().unwrap();
        assert!(
            !bytes
                .windows("PRIVATE-DEPARTURE".len())
                .any(|v| v == b"PRIVATE-DEPARTURE")
        );
    }
    drop(store);
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    assert_eq!(
        load_record(&mut store, &sealer, id).unwrap(),
        (record.clone(), definition)
    );
    let mut invalid = record;
    invalid.schema_revision = "wrong".into();
    assert!(deposit_record(&mut store, &sealer, &invalid).is_err());
    sealer.0.store(false, Ordering::SeqCst);
    assert!(load_record(&mut store, &sealer, id).is_err());
    sealer.0.store(true, Ordering::SeqCst);
    let counting = CountingSealer {
        inner: &sealer,
        unseals: Default::default(),
    };
    let blob_key = format!("blob:{}", id.0.to_hex());
    pollster::block_on(store.put(&blob_key, &vec![0; 8192])).unwrap();
    assert!(load_record_bounded(&mut store, &counting, id, 4096, 4096).is_err());
    assert_eq!(counting.unseals.load(Ordering::SeqCst), 0);
}

#[test]
fn model_free_deposit_receipt_retries_exact_nonce_and_rejects_changed_request() {
    let root = tempfile::tempdir().unwrap();
    let sealer = Sealer(AtomicBool::new(true));
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let definition = PropertyDefinitionV1 {
        id: "fixture.count".into(),
        revision: "1".into(),
        value_type: ValueType::Integer,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec![],
    };
    let nonce = uuid::Uuid::new_v4();
    let record = PropertyRecordV1 {
        id: nonce.to_string(),
        entity: uuid::Uuid::new_v4().to_string(),
        property: definition.id.clone(),
        schema_revision: definition.revision.clone(),
        definition_manifest: definition.manifest_id().unwrap(),
        context: "count".into(),
        value: PropertyValue::Integer(5),
        valid_time: None,
        status: None,
        author: "owner".into(),
        method: "manual".into(),
        evidence: Evidence::Passage {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: Some("owner".into()),
                source: "source".into(),
                node: None,
            },
            version: "v1".into(),
            hash: eidetic::Hash::of(b"passage"),
            selector: PassageSelector { start: 0, end: 7 },
        },
        recorded_at: None,
    };
    let request = DepositRequest {
        nonce,
        session: uuid::Uuid::new_v4(),
        graph: uuid::Uuid::new_v4(),
        generation: uuid::Uuid::new_v4(),
        definition,
        record,
    };
    let first = deposit_request(&mut store, &sealer, &request).unwrap();
    drop(store);
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    assert_eq!(
        deposit_request(&mut store, &sealer, &request).unwrap(),
        first
    );
    let mut changed = request.clone();
    changed.record.value = PropertyValue::Integer(6);
    assert!(deposit_request(&mut store, &sealer, &changed).is_err());
    let receipt = read_receipt(&mut store, &sealer, nonce).unwrap().unwrap();
    assert_eq!(receipt.request, request);
    assert_eq!(receipt.manifest, Some(first));
    assert!(!receipt.saved);
    let custody = crate::trail_memory::SourceCustody {
        sealer: std::sync::Arc::new(Sealer(AtomicBool::new(true))),
        epoch: std::sync::Arc::new(|| Some(1)),
    };
    let actor_dir = root.path().join(request.session.to_string()).join("memory");
    let (reply, receive) = std::sync::mpsc::sync_channel(1);
    let mut actor_store = Some(store);
    crate::trail_memory::record_actor::handle(
        crate::trail_memory::record_actor::Command {
            session: request.session,
            generation: request.generation,
            max_bytes: 16 * 1024 * 1024,
            kind: crate::trail_memory::record_actor::Kind::Pending(
                vec![],
                100,
                1_000_000,
                None,
                reply,
            ),
        },
        &mut actor_store,
        Some(&custody),
        &actor_dir,
        request.generation,
    );
    assert!(
        receive.recv().unwrap().is_ok(),
        "unbound orphan must not block unrelated queries"
    );
    let mut store = actor_store.take().unwrap();
    assert!(acknowledge(&mut store, &sealer, &request, first, "statement-1").is_err());
    let statement = serde_json::json!({"statement_id":"statement-1","predicate":"fixture"});
    mark_attached(
        &mut store,
        &sealer,
        &request,
        first,
        "statement-1",
        &statement,
    )
    .unwrap();
    drop(store);
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let receipt = read_receipt(&mut store, &sealer, nonce).unwrap().unwrap();
    assert_eq!(receipt.assertion, Some(statement.clone()));
    assert!(!receipt.saved);
    assert!(
        mark_attached(
            &mut store,
            &sealer,
            &request,
            first,
            "statement-2",
            &statement
        )
        .is_err()
    );
    let pending_key = format!("turnstone/sealed-property-pending/v2/{nonce}");
    let interrupted_pending = pollster::block_on(store.get(&pending_key))
        .unwrap()
        .unwrap();
    acknowledge(&mut store, &sealer, &request, first, "statement-1").unwrap();
    // Simulate interruption after durable completion, before pending cleanup.
    pollster::block_on(store.put(&pending_key, &interrupted_pending)).unwrap();
    let interrupted = crate::trail_memory::sealed_record_receipts::inventory(
        &mut store,
        &sealer,
        request.session,
        1,
        100_000,
    )
    .unwrap();
    assert_eq!(interrupted[0].nonce(), nonce);
    acknowledge(&mut store, &sealer, &request, first, "statement-1").unwrap();
    for index in 0..8 {
        let mut complete = request.clone();
        complete.nonce = uuid::Uuid::new_v4();
        complete.record.id = complete.nonce.to_string();
        let id = deposit_request(&mut store, &sealer, &complete).unwrap();
        let statement_id = format!("statement-{index}");
        let assertion = serde_json::json!({"statement_id": statement_id.clone()});
        mark_attached(
            &mut store,
            &sealer,
            &complete,
            id,
            &statement_id,
            &assertion,
        )
        .unwrap();
        acknowledge(&mut store, &sealer, &complete, id, &statement_id).unwrap();
    }
    let mut pending = request.clone();
    pending.nonce = uuid::Uuid::new_v4();
    pending.record.id = pending.nonce.to_string();
    let pending_id = deposit_request(&mut store, &sealer, &pending).unwrap();
    let assertion = serde_json::json!({"statement_id": "pending"});
    mark_attached(
        &mut store, &sealer, &pending, pending_id, "pending", &assertion,
    )
    .unwrap();
    let inventory = crate::trail_memory::sealed_record_receipts::inventory(
        &mut store,
        &sealer,
        request.session,
        1,
        100_000,
    )
    .unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].nonce(), pending.nonce);
    assert_eq!(
        deposit_request(&mut store, &sealer, &request).unwrap(),
        first
    );
    let mut another = pending.clone();
    another.nonce = uuid::Uuid::new_v4();
    another.record.id = another.nonce.to_string();
    deposit_request(&mut store, &sealer, &another).unwrap();
    assert!(
        crate::trail_memory::sealed_record_receipts::inventory(
            &mut store,
            &sealer,
            request.session,
            1,
            100_000,
        )
        .is_err()
    );
}

#[test]
fn model_free_saved_result_reopens_and_replays_frozen_inputs() {
    use eidetic::records::{
        RecordFilters, RecordOperation, RecordQuery, RecordSettings, RecordSnapshot,
    };
    let root = tempfile::tempdir().unwrap();
    let sealer = Sealer(AtomicBool::new(true));
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let snapshot = RecordSnapshot {
        scope_generation: "session:revision".into(),
        complete: true,
        exclusions: vec![],
        sources: vec![],
        definitions: vec![],
        records: vec![],
        lifecycle: vec![],
    };
    let query = RecordQuery {
        operation: RecordOperation::SourceLookup,
        filters: RecordFilters::default(),
    };
    let result =
        eidetic_search::records::evaluate(&snapshot, &query, &RecordSettings::default()).unwrap();
    let id = deposit_result(&mut store, &sealer, &result).unwrap();
    drop(store);
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    assert_eq!(load_result(&mut store, &sealer, id).unwrap(), result);
    let session = uuid::Uuid::new_v4();
    for index in 0..8 {
        let request = crate::trail_memory::sealed_result_receipts::ResultRequest {
            nonce: uuid::Uuid::new_v4(),
            session,
            graph: uuid::Uuid::new_v4(),
            entity: uuid::Uuid::new_v4(),
            generation: uuid::Uuid::new_v4(),
            manifest: id,
        };
        let statement = format!("result-{index}");
        crate::trail_memory::sealed_result_receipts::deposit_result_request(
            &mut store, &sealer, &request, &result,
        )
        .unwrap();
        crate::trail_memory::sealed_result_receipts::mark_result_attached(
            &mut store,
            &sealer,
            &request,
            &statement,
            &serde_json::json!({"statement_id": statement.clone()}),
        )
        .unwrap();
        crate::trail_memory::sealed_result_receipts::acknowledge_result(
            &mut store, &sealer, &request, &statement,
        )
        .unwrap();
    }
    assert!(
        crate::trail_memory::sealed_record_receipts::inventory(
            &mut store, &sealer, session, 1, 100_000
        )
        .unwrap()
        .is_empty()
    );
    let mut forged = result;
    forged.operation_version = "forged".into();
    assert!(deposit_result(&mut store, &sealer, &forged).is_err());
    let counting = CountingSealer {
        inner: &sealer,
        unseals: Default::default(),
    };
    let blob_key = format!("blob:{}", id.0.to_hex());
    pollster::block_on(store.put(&blob_key, &vec![0; 8192])).unwrap();
    assert!(load_result_bounded(&mut store, &counting, id, 4096, None).is_err());
    assert_eq!(counting.unseals.load(Ordering::SeqCst), 0);
}

#[path = "sealed_records_limits_tests.rs"]
mod limits;
#[path = "sealed_records_sync_tests.rs"]
mod sync_tests;

#[test]
fn model_free_failed_save_keeps_pending_and_restores_exact_binding_without_losing_other_assertions()
{
    use crate::research_record_graph as owner;
    use crate::trail_memory::sealed_record_receipts::{RecoveryItem, inventory};
    let root = tempfile::tempdir().unwrap();
    let session = uuid::Uuid::new_v4();
    let directory = root.path().join(session.to_string());
    let memory = directory.join("memory");
    let sealed = directory.join("sealed");
    std::fs::create_dir_all(&memory).unwrap();
    let sealer = Sealer(AtomicBool::new(true));
    let persistence = crate::session_persistence::SessionPersistence::<
        pandect::graph_placement::PlacementProfile,
    >::writable(directory.clone(), None)
    .unwrap();
    let mut graph = mere::kernel::graph::Graph::new();
    let node = mere::kernel::graph::apply::add_node(
        &mut graph,
        Some(uuid::Uuid::new_v4()),
        "https://fixture.test/entity".into(),
        Default::default(),
    );
    let entity = graph.shown_resource_id(node).unwrap();
    let other = owner::attach(
        &mut graph,
        entity,
        eidetic::ManifestId::of_blob(b"unrelated"),
    )
    .unwrap();
    let graph_file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    persistence
        .save_graph_with(
            &directory,
            &graph,
            || Ok(()),
            |placement| crate::session_graph_file::save_profiled(&graph_file, &graph, placement),
        )
        .unwrap();
    let definition = PropertyDefinitionV1 {
        id: "fixture.departure".into(),
        revision: "1".into(),
        value_type: ValueType::Integer,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec![],
    };
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
        status: None,
        author: graph.write_author().asserter_iri(),
        method: "manual".into(),
        evidence: Evidence::Passage {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: None,
                source: "fixture".into(),
                node: None,
            },
            version: "v1".into(),
            hash: eidetic::Hash::of(b"passage"),
            selector: PassageSelector { start: 0, end: 7 },
        },
        recorded_at: Some(100),
    };
    let request = DepositRequest {
        nonce,
        session,
        graph: session,
        generation: uuid::Uuid::new_v4(),
        definition,
        record,
    };
    let mut store = muniment::FjallStore::open(&sealed).unwrap();
    let manifest = deposit_request(&mut store, &sealer, &request).unwrap();
    let binding = owner::attach(&mut graph, entity, manifest).unwrap();
    let assertion = owner::assertion_for(&graph, &binding).unwrap();
    mark_attached(
        &mut store,
        &sealer,
        &request,
        manifest,
        &binding.statement_id,
        &assertion,
    )
    .unwrap();
    let failed = persistence.save_graph_with(
        &directory,
        &graph,
        || Err(std::io::Error::other("fixture save failure")),
        |_| Ok(()),
    );
    assert!(failed.is_err());
    assert!(
        matches!(inventory(&mut store, &sealer, session, 100, 1_000_000).unwrap().as_slice(), [RecoveryItem::Property(receipt)] if !receipt.saved && receipt.statement_id.as_deref() == Some(binding.statement_id.as_str()))
    );
    let custody = crate::trail_memory::SourceCustody {
        sealer: std::sync::Arc::new(Sealer(AtomicBool::new(true))),
        epoch: std::sync::Arc::new(|| Some(1)),
    };
    let (reply, receive) = std::sync::mpsc::sync_channel(1);
    let mut actor_store = Some(store);
    crate::trail_memory::record_actor::handle(
        crate::trail_memory::record_actor::Command {
            session,
            generation: request.generation,
            max_bytes: 16 * 1024 * 1024,
            kind: crate::trail_memory::record_actor::Kind::Pending(
                vec![(session, manifest)],
                100,
                1_000_000,
                None,
                reply,
            ),
        },
        &mut actor_store,
        Some(&custody),
        &memory,
        request.generation,
    );
    assert!(
        receive.recv().unwrap().is_err(),
        "pending binding must refuse a query snapshot"
    );
    let store = actor_store.take().unwrap();
    drop(store);
    let mut reopened = crate::session_graph_file::load(&graph_file)
        .unwrap()
        .unwrap();
    assert!(reopened.find_assertion(&binding.statement_id).is_none());
    owner::restore(
        &mut reopened,
        &binding,
        &assertion,
        false,
        &Default::default(),
    )
    .unwrap();
    assert!(owner::bindings(&reopened, false).unwrap().contains(&other));
    persistence
        .save_graph_with(
            &directory,
            &reopened,
            || Ok(()),
            |placement| crate::session_graph_file::save_profiled(&graph_file, &reopened, placement),
        )
        .unwrap();
    let mut store = muniment::FjallStore::open(&sealed).unwrap();
    acknowledge(
        &mut store,
        &sealer,
        &request,
        manifest,
        &binding.statement_id,
    )
    .unwrap();
    assert!(
        inventory(&mut store, &sealer, session, 100, 1_000_000)
            .unwrap()
            .is_empty()
    );
    let reopened = crate::session_graph_file::load(&graph_file)
        .unwrap()
        .unwrap();
    assert!(
        owner::bindings(&reopened, false)
            .unwrap()
            .contains(&binding)
    );
    assert!(owner::bindings(&reopened, false).unwrap().contains(&other));
}
