// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use eidetic::models::recall::{PassageSelector, RecallSourceId, RecallSourceKind};
use eidetic::records::{
    Evidence, PropertyRecordV1, PropertyValue, RecordFilters, RecordOperation, RecordQuery,
    RecordSnapshot,
};

#[test]
fn model_free_limit_refusal_binds_the_attempt_and_current_scope() {
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
    let graph = app.graph_runtimes.active_graph();
    app.graph_runtimes
        .get_mut(graph)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            let node = mere::kernel::graph::apply::add_node(
                graph,
                Some(uuid::Uuid::new_v4()),
                "https://fixture.test/a".into(),
                Default::default(),
            );
            let entity = graph.shown_resource_id(node).unwrap();
            crate::research_record_graph::attach(
                graph,
                entity,
                eidetic::ManifestId::of_blob(b"one"),
            )
            .unwrap();
            crate::research_record_graph::attach(
                graph,
                entity,
                eidetic::ManifestId::of_blob(b"two"),
            )
            .unwrap();
            true
        });
    app.research_search.preferences.records.max_records = 1;
    app.omnibar.open = true;
    app.research_search.view = super::super::research_search_state::View::Structured;
    let query = RecordQuery {
        operation: RecordOperation::PropertyLookup,
        filters: RecordFilters::default(),
    };
    let settings = app.research_search.preferences.records.clone();
    let effects = app.structured_query(query.clone(), settings.clone());
    let request = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ResearchRecords(Job::Query(request)) => Some(request),
            _ => None,
        })
        .expect("query dispatched");
    let expected_id = request.id;
    let mut stale = request.clone();
    stale.id = uuid::Uuid::new_v4();
    app.structured_completed(JobResult::Query {
        request: stale,
        result: Err("Structured record limit exceeded".into()),
        manifest: Ok(None),
    });
    assert!(app.structured_scenario_status()["limit_refusal"].is_null());
    app.structured_completed(JobResult::Query {
        request,
        result: Err("Structured record limit exceeded".into()),
        manifest: Ok(None),
    });
    let status = app.structured_scenario_status();
    assert_eq!(status["error"], "Structured record limit exceeded");
    let refusal = &status["limit_refusal"];
    assert_eq!(refusal["stage"], "worker-refusal");
    assert_eq!(refusal["request_id"], serde_json::json!(expected_id));
    assert_eq!(refusal["query"], serde_json::to_value(&query).unwrap());
    assert_eq!(
        refusal["settings"],
        serde_json::to_value(&settings).unwrap()
    );
    assert_eq!(refusal["source_selection"]["mode"], "all");
    assert_eq!(
        refusal["graph_revisions"][0]["graph"],
        serde_json::json!(graph.as_uuid())
    );
    assert_eq!(refusal["error"], "Structured record limit exceeded");
    app.research_search.selected = Some(vec![]);
    app.invalidate_structured();
    app.research_search.selected = None;
    assert!(
        app.structured_scenario_status()["limit_refusal"].is_null(),
        "refusal resurrected after selection roundtrip"
    );
    let effects = app.structured_query(query.clone(), settings.clone());
    let request = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ResearchRecords(Job::Query(request)) => Some(request),
            _ => None,
        })
        .unwrap();
    app.structured_completed(JobResult::Query {
        request,
        result: Err("Structured record limit exceeded".into()),
        manifest: Ok(None),
    });
    app.graph_runtimes.visit("https://fixture.test/other");
    assert!(app.structured_scenario_status()["limit_refusal"].is_null());
    let effects = app.structured_query(query.clone(), settings.clone());
    let request = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ResearchRecords(Job::Query(request)) => Some(request),
            _ => None,
        })
        .unwrap();
    app.structured_completed(JobResult::Query {
        request,
        result: Err("Structured record limit exceeded".into()),
        manifest: Ok(None),
    });
    assert!(app.structured_scenario_status()["limit_refusal"].is_object());
    app.research_structured_command(super::super::research_structured_state::Command::Home);
    assert!(app.structured_scenario_status()["limit_refusal"].is_null());
    let effects = app.structured_query(query, settings);
    let request = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ResearchRecords(Job::Query(request)) => Some(request),
            _ => None,
        })
        .unwrap();
    app.structured_completed(JobResult::Query {
        request,
        result: Err("Structured record limit exceeded".into()),
        manifest: Ok(None),
    });
    app.research.set_available(false);
    app.invalidate_structured();
    assert!(app.structured_scenario_status()["limit_refusal"].is_null());
}

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

#[test]
fn model_free_status_never_serializes_a_held_result_during_refusal() {
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
        scope_generation: "private-result-canary".into(),
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
    let controller = &mut app.research_search.structured.controller;
    controller.result = Some(value);
    controller.view_scope = Some(scope);
    controller.error = Some("source unavailable".into());
    let status = app.structured_scenario_status();
    assert_eq!(status["held_result"], true);
    assert_eq!(status["error"], "source unavailable");
    assert!(!status.to_string().contains("private-result-canary"));
}
