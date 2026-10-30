// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use eidetic::models::recall::{PassageSelector, RecallSourceId, RecallSourceKind};
use eidetic::records::{
    Evidence, PropertyRecordV1, PropertyValue, RecordFilters, RecordOperation, RecordQuery,
    RecordSnapshot,
};

#[test]
fn model_free_inspection_and_replay_clear_when_custody_or_runtime_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let scope = app.structured_scope().unwrap();
    let graph = scope.graphs[0].0;
    let binding = Binding {
        graph,
        entity: uuid::Uuid::new_v4(),
        statement_id: "statement".into(),
        manifest: eidetic::ManifestId::of_blob(b"record"),
    };
    let record = PropertyRecordV1 {
        id: uuid::Uuid::new_v4().to_string(),
        entity: binding.entity.to_string(),
        property: "test.property".into(),
        schema_revision: "1".into(),
        definition_manifest: eidetic::ManifestId::of_blob(b"definition"),
        context: "test".into(),
        value: PropertyValue::Integer(1),
        valid_time: None,
        status: None,
        author: "author".into(),
        method: "manual".into(),
        evidence: Evidence::Passage {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: None,
                source: "source".into(),
                node: None,
            },
            version: "v".into(),
            hash: eidetic::Hash::of(b"x"),
            selector: PassageSelector { start: 0, end: 1 },
        },
        recorded_at: None,
    };
    app.research_search.structured.controller.inspected = Some((binding, record));
    app.research_search.structured.controller.view_scope = Some(scope.clone());
    app.research.set_available(false);
    app.invalidate_structured();
    assert!(
        app.research_search
            .structured
            .controller
            .inspected
            .is_none()
    );
    app.research.set_available(true);
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let scope = app.structured_scope().unwrap();
    let snapshot = RecordSnapshot {
        scope_generation: "test".into(),
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
    app.research_search.structured.controller.result = Some(result);
    app.research_search.structured.controller.view_scope = Some(scope);
    app.graph_runtimes.get_mut(graph).unwrap().instance = uuid::Uuid::new_v4();
    app.invalidate_structured();
    assert!(app.research_search.structured.controller.result.is_none());
}

#[test]
fn model_free_late_inspection_and_replay_results_never_install_after_graph_or_session_change() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let scope = app.structured_scope().unwrap();
    let graph_id = scope.graphs[0].0;
    let binding = Binding {
        graph: graph_id,
        entity: uuid::Uuid::new_v4(),
        statement_id: "stale".into(),
        manifest: eidetic::ManifestId::of_blob(b"stale"),
    };
    let id = uuid::Uuid::new_v4();
    app.research_search.structured.controller.pending = Some(Job::Inspect {
        id,
        scope: scope.clone(),
        binding: binding.clone(),
    });
    app.graph_runtimes
        .get_mut(graph_id)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            mere::kernel::graph::apply::add_node(
                graph,
                Some(uuid::Uuid::new_v4()),
                "https://fixture.test/change".into(),
                Default::default(),
            );
            true
        });
    app.structured_completed(JobResult::Inspect {
        id,
        scope,
        binding,
        result: Err("late".into()),
    });
    assert!(
        app.research_search
            .structured
            .controller
            .inspected
            .is_none()
    );
    app.sync_research_context();
    let scope = app.structured_scope().unwrap();
    let id = uuid::Uuid::new_v4();
    let manifest = eidetic::ManifestId::of_blob(b"result");
    app.research_search.structured.controller.pending = Some(Job::Replay {
        id,
        scope: scope.clone(),
        result: manifest,
    });
    app.session_id = crate::panes::SessionId::new();
    app.structured_completed(JobResult::Replay {
        id,
        scope,
        result: Err("late".into()),
    });
    assert!(app.research_search.structured.controller.result.is_none());
}

#[test]
fn model_free_failed_graph_save_and_ack_never_install_a_successful_result() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let original = app.structured_scope().unwrap();
    let graph_id = original.graphs[0].0;
    let mut entity = None;
    app.graph_runtimes
        .get_mut(graph_id)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            let node = mere::kernel::graph::apply::add_node(
                graph,
                Some(uuid::Uuid::new_v4()),
                "https://fixture.test/entity".into(),
                Default::default(),
            );
            entity = graph.shown_resource_id(node);
            true
        });
    let entity = entity.unwrap();
    let before = app.structured_scope().unwrap();
    let snapshot = RecordSnapshot {
        scope_generation: "fixture".into(),
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
    let settings = RecordSettings::default();
    let value = eidetic_search::records::evaluate(&snapshot, &query, &settings).unwrap();
    let request = QueryRequest {
        id: uuid::Uuid::new_v4(),
        scope: before,
        selected: None,
        query,
        settings,
        candidates: vec![],
        result_anchor: Some((graph_id, entity)),
    };
    let manifest = eidetic::ManifestId::of_blob(b"result");
    let author = app.structured_author_identity().unwrap().asserter_iri();
    let (binding, assertion) = app
        .structured_attach_binding(graph_id, entity, manifest, true, &author)
        .unwrap();
    let scope = app.structured_scope().unwrap();
    app.research_search.structured.controller.pending = Some(Job::MarkResult {
        request: request.clone(),
        scope: scope.clone(),
        binding: binding.clone(),
        assertion: assertion.clone(),
        value: value.clone(),
    });
    app.structured_completed(JobResult::MarkResult {
        request: request.clone(),
        scope: scope.clone(),
        binding: binding.clone(),
        assertion: assertion.clone(),
        value: value.clone(),
        result: Err("injected sync failure".into()),
    });
    assert!(app.research_search.structured.controller.result.is_none());
    assert!(
        !app.session_dir()
            .join(pandect::session_graph_store::GRAPH_FILE)
            .exists()
    );
    std::fs::create_dir(
        app.session_dir()
            .join(pandect::session_graph_store::GRAPH_FILE),
    )
    .unwrap();
    app.research_search.structured.controller.pending = Some(Job::MarkResult {
        request: request.clone(),
        scope: scope.clone(),
        binding: binding.clone(),
        assertion: assertion.clone(),
        value: value.clone(),
    });
    app.structured_completed(JobResult::MarkResult {
        request: request.clone(),
        scope: scope.clone(),
        binding: binding.clone(),
        assertion,
        value: value.clone(),
        result: Ok(()),
    });
    assert!(app.research_search.structured.controller.result.is_none());
    assert!(
        app.research_search
            .structured
            .controller
            .error
            .as_deref()
            .is_some_and(|error| error.contains("save failed"))
    );
    app.research_search.structured.controller.pending = Some(Job::ConfirmResult {
        request: request.clone(),
        scope: scope.clone(),
        binding: binding.clone(),
        value: value.clone(),
    });
    app.structured_completed(JobResult::ConfirmResult {
        request,
        scope,
        binding,
        value,
        result: Err("ack refused".into()),
    });
    assert!(app.research_search.structured.controller.result.is_none());
    assert!(
        app.research_search
            .structured
            .controller
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ack refused"))
    );
}

#[test]
fn model_free_result_install_generations_refuse_old_source_open_requests() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let scope = app.structured_scope().unwrap();
    let snapshot = RecordSnapshot {
        scope_generation: "fixture".into(),
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
    app.research_search
        .structured
        .controller
        .install_result(result.clone(), scope.clone())
        .unwrap();
    let old = app
        .research_search
        .structured
        .controller
        .result_generation
        .unwrap();
    app.research_search
        .structured
        .controller
        .install_result(result, scope)
        .unwrap();
    assert_ne!(
        app.research_search.structured.controller.result_generation,
        Some(old)
    );
    let source = RecallSourceId {
        kind: RecallSourceKind::Capture,
        owner: None,
        source: "unavailable".into(),
        node: None,
    };
    let effects = app.structured_open_result_source(old, source, "v1".into());
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::ResearchRecords(_)))
    );
    assert!(
        app.research_search
            .structured
            .controller
            .error
            .as_deref()
            .is_some_and(|error| error.contains("stale"))
    );
}
