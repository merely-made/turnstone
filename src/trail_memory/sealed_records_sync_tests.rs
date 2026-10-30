// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::trail_memory::record_actor::{self, Kind};
use std::sync::{Arc, mpsc};

fn call<T>(
    store: &mut Option<muniment::FjallStore>,
    custody: &crate::trail_memory::SourceCustody,
    directory: &std::path::Path,
    session: uuid::Uuid,
    generation: uuid::Uuid,
    fail: bool,
    make: impl FnOnce(mpsc::SyncSender<Result<T, crate::research_storage::SourceReadError>>) -> Kind,
) -> Result<T, crate::research_storage::SourceReadError> {
    let (reply, receive) = mpsc::sync_channel(1);
    record_actor::handle_with_sync(
        record_actor::Command {
            session,
            generation,
            max_bytes: 16 * 1024 * 1024,
            budget: crate::research_storage::StorageLimits::default().budget(),
            kind: make(reply),
        },
        store,
        Some(custody),
        directory,
        generation,
        &|_| {
            if fail {
                Err(muniment::StoreError::Backend(
                    "injected sync failure".into(),
                ))
            } else {
                Ok(())
            }
        },
    );
    receive.recv().unwrap()
}

#[test]
fn model_free_property_sync_failure_fences_each_mutation_until_exact_retry() {
    let root = tempfile::tempdir().unwrap();
    let sealer = Arc::new(Sealer(AtomicBool::new(true)));
    let custody = crate::trail_memory::SourceCustody {
        sealer: sealer.clone(),
        epoch: Arc::new(|| Some(1)),
    };
    let session = uuid::Uuid::new_v4();
    let generation = uuid::Uuid::new_v4();
    let directory = root.path().join(session.to_string()).join("memory");
    let mut store = Some(
        muniment::FjallStore::open_with_limits(
            root.path().join("sealed"),
            crate::research_storage::StorageLimits::default().owner(),
        )
        .unwrap(),
    );
    let definition = PropertyDefinitionV1 {
        id: "fixture.sync".into(),
        revision: "1".into(),
        value_type: ValueType::Integer,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec![],
    };
    let nonce = uuid::Uuid::new_v4();
    let request = DepositRequest {
        nonce,
        session,
        graph: uuid::Uuid::new_v4(),
        generation,
        record: PropertyRecordV1 {
            id: nonce.to_string(),
            entity: uuid::Uuid::new_v4().to_string(),
            property: definition.id.clone(),
            schema_revision: definition.revision.clone(),
            definition_manifest: definition.manifest_id().unwrap(),
            context: "route".into(),
            value: PropertyValue::Integer(1),
            valid_time: None,
            status: None,
            author: "owner".into(),
            method: "manual".into(),
            evidence: Evidence::Passage {
                source: RecallSourceId {
                    kind: RecallSourceKind::Capture,
                    owner: None,
                    source: "source".into(),
                    node: None,
                },
                version: "v1".into(),
                hash: eidetic::Hash::of(b"passage"),
                selector: PassageSelector { start: 0, end: 1 },
            },
            recorded_at: None,
        },
        definition,
    };
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::Deposit(request.clone(), reply)
        )
        .is_err()
    );
    let manifest = call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| Kind::Deposit(request.clone(), reply),
    )
    .unwrap();
    let statement = "statement".to_string();
    let assertion = serde_json::json!({"statement_id": statement.clone()});
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::MarkAttached(
                request.clone(),
                manifest,
                statement.clone(),
                assertion.clone(),
                reply
            )
        )
        .is_err()
    );
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::Pending(
                vec![(request.graph, manifest)],
                1,
                16 * 1024 * 1024,
                None,
                reply
            )
        )
        .is_err()
    );
    call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| {
            Kind::MarkAttached(
                request.clone(),
                manifest,
                statement.clone(),
                assertion.clone(),
                reply,
            )
        },
    )
    .unwrap();
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::Acknowledge(request.clone(), manifest, statement.clone(), reply)
        )
        .is_err()
    );
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::Pending(
                vec![(request.graph, manifest)],
                1,
                16 * 1024 * 1024,
                None,
                reply
            )
        )
        .is_err()
    );
    call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| Kind::Acknowledge(request.clone(), manifest, statement.clone(), reply),
    )
    .unwrap();
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::PrepareWithdrawal(
                nonce,
                request.graph,
                manifest,
                statement.clone(),
                reply
            )
        )
        .is_err()
    );
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::Pending(
                vec![(request.graph, manifest)],
                1,
                16 * 1024 * 1024,
                None,
                reply
            )
        )
        .is_err()
    );
    call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| Kind::PrepareWithdrawal(nonce, request.graph, manifest, statement.clone(), reply),
    )
    .unwrap();
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::AcknowledgeWithdrawal(
                nonce,
                request.graph,
                manifest,
                statement.clone(),
                reply
            )
        )
        .is_err()
    );
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::Pending(
                vec![(request.graph, manifest)],
                1,
                16 * 1024 * 1024,
                None,
                reply
            )
        )
        .is_err()
    );
    call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| {
            Kind::AcknowledgeWithdrawal(nonce, request.graph, manifest, statement.clone(), reply)
        },
    )
    .unwrap();
}

#[test]
fn model_free_result_sync_failure_fences_each_mutation_until_exact_retry() {
    use eidetic::TypedPayload;
    use eidetic::records::{
        RecordFilters, RecordOperation, RecordQuery, RecordSettings, RecordSnapshot,
    };
    let root = tempfile::tempdir().unwrap();
    let custody = crate::trail_memory::SourceCustody {
        sealer: Arc::new(Sealer(AtomicBool::new(true))),
        epoch: Arc::new(|| Some(1)),
    };
    let session = uuid::Uuid::new_v4();
    let generation = uuid::Uuid::new_v4();
    let directory = root.path().join(session.to_string()).join("memory");
    let mut store = Some(
        muniment::FjallStore::open_with_limits(
            root.path().join("sealed"),
            crate::research_storage::StorageLimits::default().owner(),
        )
        .unwrap(),
    );
    let snapshot = RecordSnapshot {
        scope_generation: "scope".into(),
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
    let value =
        eidetic_search::records::evaluate(&snapshot, &query, &RecordSettings::default()).unwrap();
    let manifest = eidetic::ManifestId::of_blob(&value.serialize_to_bytes().unwrap());
    let request = record_actor::ResultRequest {
        nonce: uuid::Uuid::new_v4(),
        session,
        graph: uuid::Uuid::new_v4(),
        entity: uuid::Uuid::new_v4(),
        generation,
        manifest,
    };
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::SaveResult(request.clone(), value.clone(), reply)
        )
        .is_err()
    );
    assert_eq!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::SaveResult(request.clone(), value.clone(), reply)
        )
        .unwrap(),
        manifest
    );
    let statement = "result-statement".to_string();
    let assertion = serde_json::json!({"statement_id": statement.clone()});
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::MarkResultAttached(
                request.clone(),
                statement.clone(),
                assertion.clone(),
                reply
            )
        )
        .is_err()
    );
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::Pending(
                vec![(request.graph, manifest)],
                1,
                16 * 1024 * 1024,
                None,
                reply
            )
        )
        .is_err()
    );
    call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| {
            Kind::MarkResultAttached(request.clone(), statement.clone(), assertion.clone(), reply)
        },
    )
    .unwrap();
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            true,
            |reply| Kind::AcknowledgeResult(request.clone(), statement.clone(), reply)
        )
        .is_err()
    );
    assert!(
        call(
            &mut store,
            &custody,
            &directory,
            session,
            generation,
            false,
            |reply| Kind::Pending(
                vec![(request.graph, manifest)],
                1,
                16 * 1024 * 1024,
                None,
                reply
            )
        )
        .is_err()
    );
    call(
        &mut store,
        &custody,
        &directory,
        session,
        generation,
        false,
        |reply| Kind::AcknowledgeResult(request.clone(), statement.clone(), reply),
    )
    .unwrap();
}
