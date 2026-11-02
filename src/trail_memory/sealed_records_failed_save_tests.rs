// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

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
    let mut store = open_bounded(&sealed);
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
            budget: crate::research_storage::StorageLimits::default().budget(),
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
    let mut store = open_bounded(&sealed);
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
