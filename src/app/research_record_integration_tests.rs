// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::research::{Bridge, ClipAccess};
use crate::research_search::Request as SearchRequest;
use crate::trail_memory::{self, TrailCommand};
use eidetic::records::{
    Cardinality, PropertyDefinitionV1, PropertyValue, RecordFilters, RecordOperation, ValueType,
};
use std::sync::{Arc, mpsc};
use std::time::Duration;

fn finish(
    app: &mut App,
    bridge: &Bridge,
    actor: &armillary::ActorHandle<TrailCommand>,
    mut effects: Vec<Effect>,
) {
    for _ in 0..8 {
        let Some(job) = effects.into_iter().find_map(|effect| {
            if let Effect::ResearchRecords(job) = effect {
                Some(job)
            } else {
                None
            }
        }) else {
            return;
        };
        let response = crate::research_records_worker::execute(bridge, actor, job);
        effects = app.structured_completed(response);
    }
    panic!("structured operation did not settle");
}

#[test]
fn model_free_app_authors_saves_reopens_and_queries_a_sourced_property() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    app.research_search.preferences.records.max_records = 1;
    let graph_id = app.graph_runtimes.active_graph();
    let node = app.graph_runtimes.visit("https://fixture.test/ferry");
    let graph = app.graph_runtimes.graph();
    let entity = graph.shown_resource_id(node).unwrap();
    let node_id = graph.get_node(node).unwrap().id;
    let directory = app.session_dir();
    let memory = directory.join("memory");
    std::fs::create_dir_all(&memory).unwrap();
    let html = "<html><main>Ferry departs 09:30.</main></html>";
    let mut store = muniment::FjallStore::open(&memory).unwrap();
    let capture = trail_memory::capture_source_document_in(
        &mut store,
        &crate::content::FetchedDocument {
            bytes: html.as_bytes().to_vec(),
            body: html.into(),
            effective_url: Some("https://fixture.test/ferry".into()),
            content_type: Some("text/html".into()),
            acquired_at_ms: 1000,
        },
    )
    .unwrap();
    drop(store);
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph_id).unwrap().canvas,
        |facets| {
            crate::source_capture::append_reference(
                facets,
                node_id,
                capture.annotation_manifest.clone(),
            )
        },
    )
    .unwrap()
    .unwrap();
    let graph_file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let runtime = app.graph_runtimes.get(graph_id).unwrap();
    runtime
        .persistence
        .save_graph_with(
            &directory,
            runtime.canvas.graph(),
            || pandect::save_node_facets(&directory, runtime.canvas.graph().facets()),
            |placement| {
                crate::session_graph_file::save_profiled(
                    &graph_file,
                    runtime.canvas.graph(),
                    placement,
                )
            },
        )
        .unwrap();
    let custody = trail_memory::custody_test_fixture::custody();
    let (actor, _) = trail_memory::spawn_trail_for_session(
        Arc::new(|| {}),
        Some(memory),
        Default::default(),
        app.source_capture_generation,
    );
    assert!(actor.command(TrailCommand::SetCustody(Some(custody.clone()))));
    let (ready, received) = mpsc::sync_channel(1);
    assert!(actor.command(TrailCommand::CustodyReady(ready)));
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    app.research.set_available(true);
    app.research.access(Some(ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let bridge = app.research.clone();
    let (context, revision) = bridge.context_revision().unwrap();
    let lexical = SearchRequest {
        id: uuid::Uuid::new_v4(),
        context,
        revision,
        epoch: 1,
        query_revision: 1,
        selected: None,
        query: Default::default(),
        settings: Default::default(),
    };
    let sources = crate::research_search::snapshot(&bridge, &actor, &lexical).unwrap();
    let source = &sources.sources[0].retained;
    let mut target = crate::research::target(&source.source, &source.version).unwrap();
    target.set_selector(crate::saved_sources::Selector { start: 0, end: 5 });
    let property = ManualProperty {
        graph: graph_id,
        entity,
        definition: PropertyDefinitionV1 {
            id: "fixture.departure".into(),
            revision: "1".into(),
            value_type: ValueType::Integer,
            unit: None,
            cardinality: Cardinality::Single,
            statuses: vec!["approved".into()],
        },
        value: PropertyValue::Integer(930),
        context: "route".into(),
        valid_time: None,
        status: Some("approved".into()),
        evidence: target,
    };
    let effects = app.structured_author(property);
    finish(&mut app, &bridge, &actor, effects);
    assert!(app.research_search.structured.controller.error.is_none());
    assert!(!app.research_search.structured.controller.busy);
    let saved_graph = crate::session_graph_file::load(&graph_file)
        .unwrap()
        .unwrap();
    let property_bindings = crate::research_record_graph::bindings(&saved_graph, false).unwrap();
    assert_eq!(property_bindings.len(), 1);
    let query = RecordQuery {
        operation: RecordOperation::PropertyLookup,
        filters: RecordFilters::default(),
    };
    let mut settings = RecordSettings::default();
    settings.max_records = 1;
    let effects = app.structured_query(query.clone(), settings.clone());
    finish(&mut app, &bridge, &actor, effects);
    let controller = &app.research_search.structured.controller;
    assert!(controller.error.is_none(), "{:?}", controller.error);
    assert_eq!(controller.result.as_ref().unwrap().rows.len(), 1);
    assert!(controller.saved_result.is_some());
    let saved_graph = crate::session_graph_file::load(&graph_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::research_record_graph::bindings(&saved_graph, false).unwrap(),
        property_bindings
    );
    assert_eq!(
        crate::research_record_graph::result_bindings(&saved_graph)
            .unwrap()
            .len(),
        1
    );
    let effects = app.structured_refresh_saved();
    finish(&mut app, &bridge, &actor, effects);
    let saved = app.structured_saved_results().unwrap();
    assert_eq!(saved.len(), 1);
    let effects = app.structured_replay(saved[0].binding.manifest);
    finish(&mut app, &bridge, &actor, effects);
    assert_eq!(
        app.research_search
            .structured
            .controller
            .result
            .as_ref()
            .unwrap()
            .rows
            .len(),
        1
    );
    let record_id = app
        .research_search
        .structured
        .controller
        .result
        .as_ref()
        .unwrap()
        .rows[0]
        .record_ids[0]
        .clone();
    let binding = app
        .research_search
        .structured
        .controller
        .binding_for_record(&record_id)
        .unwrap();
    let generation = app
        .research_search
        .structured
        .controller
        .result_generation
        .unwrap();
    let index = app.research_search.structured.controller.record_index[&record_id].0;
    let stale = app.structured_inspect_frozen_record(uuid::Uuid::new_v4(), index);
    assert!(
        !stale
            .iter()
            .any(|effect| matches!(effect, Effect::ResearchRecords(_)))
    );
    let effects = app.structured_inspect_frozen_record(generation, index);
    finish(&mut app, &bridge, &actor, effects);
    assert!(
        app.research_search
            .structured
            .controller
            .inspected
            .is_some()
    );
    assert_eq!(
        app.research_search
            .structured
            .controller
            .inspected_withdrawn,
        Some(false)
    );
    let effects = app.structured_withdraw(binding.clone());
    finish(&mut app, &bridge, &actor, effects);
    assert!(app.research_search.structured.controller.error.is_none());
    let saved_graph = crate::session_graph_file::load(&graph_file)
        .unwrap()
        .unwrap();
    assert!(
        saved_graph
            .find_assertion(&binding.statement_id)
            .unwrap()
            .1
            .withdrawn
            .is_some()
    );
    let effects = app.structured_refresh_history();
    finish(&mut app, &bridge, &actor, effects);
    assert_eq!(app.structured_history().unwrap(), vec![binding.clone()]);
    let effects = app.structured_refresh_saved();
    finish(&mut app, &bridge, &actor, effects);
    let effects = app.structured_replay(saved[0].binding.manifest);
    finish(&mut app, &bridge, &actor, effects);
    let controller = &app.research_search.structured.controller;
    assert!(controller.result_historical);
    assert_eq!(controller.result.as_ref().unwrap().rows.len(), 1);
    let generation = controller.result_generation.unwrap();
    let index = controller.record_index[&record_id].0;
    let effects = app.structured_inspect_frozen_record(generation, index);
    finish(&mut app, &bridge, &actor, effects);
    assert_eq!(
        app.research_search
            .structured
            .controller
            .inspected_withdrawn,
        Some(true)
    );
    let effects = app.structured_query(query, settings);
    finish(&mut app, &bridge, &actor, effects);
    let controller = &app.research_search.structured.controller;
    assert!(!controller.result_historical);
    assert!(controller.result.as_ref().unwrap().rows.is_empty());
}
