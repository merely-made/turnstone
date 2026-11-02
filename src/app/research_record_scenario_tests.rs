// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use eidetic::models::recall::{PassageSelector, RecallSourceId, RecallSourceKind};
use eidetic::records::{
    Evidence, PropertyRecordV1, PropertyValue, RecordFilters, RecordOperation, RecordQuery,
    RecordSnapshot,
};

#[test]
fn model_free_scenario_observation_is_scope_checked_and_read_only() {
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
    let graph = scope.graphs[0].0;
    let snapshot = RecordSnapshot {
        scope_generation: "safe".into(),
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
    let mut result =
        eidetic_search::records::evaluate(&snapshot, &query, &RecordSettings::default()).unwrap();
    result.snapshot.complete = false;
    result.snapshot.exclusions.push("Denied".into());
    result
        .snapshot
        .exclusions
        .push("private-exclusion-canary".into());
    result.snapshot.records.push(PropertyRecordV1 {
        id: "record".into(),
        entity: "entity".into(),
        property: "property".into(),
        schema_revision: "1".into(),
        definition_manifest: eidetic::ManifestId::of_blob(b"definition"),
        context: "context".into(),
        value: PropertyValue::Integer(7),
        valid_time: None,
        status: None,
        author: "private-author-canary".into(),
        method: "private-method-canary".into(),
        evidence: Evidence::Passage {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: None,
                source: "source".into(),
                node: None,
            },
            version: "version".into(),
            hash: eidetic::Hash::of(b"x"),
            selector: PassageSelector { start: 0, end: 1 },
        },
        recorded_at: None,
    });
    result
        .snapshot
        .lifecycle
        .push(eidetic::records::LifecycleObservation {
            record_id: "record".into(),
            lifecycle_ref: "private-lifecycle-canary".into(),
            state: eidetic::records::LifecycleState::Active,
        });
    let mut withdrawn = result.snapshot.records[0].clone();
    withdrawn.id = "withdrawn-record".into();
    result.snapshot.records.push(withdrawn);
    result
        .snapshot
        .lifecycle
        .push(eidetic::records::LifecycleObservation {
            record_id: "withdrawn-record".into(),
            lifecycle_ref: "second-private-lifecycle-canary".into(),
            state: eidetic::records::LifecycleState::Withdrawn,
        });
    app.research_search.view = super::super::research_search_state::View::Structured;
    app.omnibar.open = true;
    app.research_search.structured.controller.result = Some(result.clone());
    app.research_search.structured.controller.result_generation = Some(uuid::Uuid::new_v4());
    app.research_search.structured.controller.view_scope = Some(scope.clone());
    app.research_search.structured.controller.accepted =
        Some(crate::research_records::QueryRequest {
            id: uuid::Uuid::new_v4(),
            scope: scope.clone(),
            selected: scope.selected.clone(),
            query: result.query.clone(),
            settings: result.settings.clone(),
            candidates: vec![],
            result_anchor: None,
        });
    let first = app.structured_scenario_observation().unwrap();
    let second = app.structured_scenario_observation().unwrap();
    assert_eq!(first, second);
    assert_eq!(first["result"]["snapshot"]["records"][0]["id"], "record");
    assert_eq!(first["result"]["snapshot"]["exclusion_count"], 2);
    assert_eq!(
        first["result"]["snapshot"]["exclusions"],
        serde_json::json!(["denied", "unclassified"])
    );
    assert_eq!(
        first["result"]["snapshot"]["lifecycle"],
        serde_json::json!([{"record_id":"record", "state":"Active"},
            {"record_id":"withdrawn-record", "state":"Withdrawn"}])
    );
    assert_eq!(first["result"]["snapshot"]["complete"], false);
    assert_eq!(first["source_selection"], serde_json::json!({"mode":"all"}));
    for canary in [
        "private-author-canary",
        "private-method-canary",
        "private-exclusion-canary",
        "private-lifecycle-canary",
    ] {
        assert!(!first.to_string().contains(canary));
    }
    assert!(first.get("owner_bindings").is_none());
    assert!(!app.structured_scenario_result_ready());
    use eidetic::TypedPayload;
    let manifest = eidetic::ManifestId::of_blob(&result.serialize_to_bytes().unwrap());
    let current_scope = app
        .research_search
        .structured
        .controller
        .view_scope
        .clone()
        .unwrap();
    app.research_search.structured.controller.accepted =
        Some(crate::research_records::QueryRequest {
            id: uuid::Uuid::new_v4(),
            scope: current_scope.clone(),
            selected: current_scope.selected.clone(),
            query: result.query.clone(),
            settings: result.settings.clone(),
            candidates: vec![],
            result_anchor: None,
        });
    app.research_search.structured.controller.saved_result = Some(manifest);
    assert!(app.structured_scenario_result_ready());
    app.research_search.structured.controller.saved_result =
        Some(eidetic::ManifestId::of_blob(b"wrong"));
    assert!(!app.structured_scenario_result_ready());
    app.research_search.structured.controller.accepted = None;
    app.research_search.structured.controller.saved_result = None;
    app.research_search.structured.controller.result_historical = true;
    assert!(app.structured_scenario_result_ready());
    let historical = app.structured_scenario_observation().unwrap();
    assert_eq!(historical["result"], first["result"]);
    assert_eq!(
        historical["source_selection"],
        serde_json::json!({"mode":"unknown-historical"})
    );
    assert!(!historical.to_string().contains("private-author-canary"));
    app.research_search.structured.controller.result_historical = false;
    app.research_search.structured.controller.accepted =
        Some(crate::research_records::QueryRequest {
            id: uuid::Uuid::new_v4(),
            scope: scope.clone(),
            selected: scope.selected.clone(),
            query: result.query.clone(),
            settings: result.settings.clone(),
            candidates: vec![],
            result_anchor: None,
        });
    app.research_search.selected = Some(vec![]);
    assert!(app.structured_scenario_observation().is_err());
    app.research_search
        .structured
        .controller
        .view_scope
        .as_mut()
        .unwrap()
        .selected = Some(vec![]);
    assert!(app.structured_scenario_observation().is_err());
    app.research_search
        .structured
        .controller
        .accepted
        .as_mut()
        .unwrap()
        .selected = Some(vec![]);
    assert_eq!(
        app.structured_scenario_observation().unwrap()["source_selection"],
        serde_json::json!({"mode":"chosen", "members":[]})
    );
    let member = crate::research_search::Member::Capture(crate::saved_sources::Key {
        session: app.session_id,
        graph,
        node: uuid::Uuid::new_v4(),
        annotation_manifest: "sealed-v1:test".into(),
    });
    app.research_search.selected = Some(vec![member.clone()]);
    assert!(app.structured_scenario_observation().is_err());
    app.research_search
        .structured
        .controller
        .view_scope
        .as_mut()
        .unwrap()
        .selected = Some(vec![member.clone()]);
    assert!(app.structured_scenario_observation().is_err());
    app.research_search
        .structured
        .controller
        .accepted
        .as_mut()
        .unwrap()
        .selected = Some(vec![member.clone()]);
    assert_eq!(
        app.structured_scenario_observation().unwrap()["source_selection"],
        serde_json::json!({"mode":"chosen", "members":[member]})
    );
    app.research_search.structured.controller.busy = true;
    assert!(app.structured_scenario_observation().is_err());
    app.research_search.structured.controller.busy = false;
    app.research_search.structured.controller.error = Some("refused".into());
    assert!(app.structured_scenario_observation().is_err());
    app.research_search.structured.controller.error = None;
    app.graph_runtimes.get_mut(graph).unwrap().instance = uuid::Uuid::new_v4();
    assert!(app.structured_scenario_observation().is_err());
}

#[test]
fn model_free_scope_change_cancels_stale_work_before_dispatching_fresh_search() {
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
    for (command, selected) in [
        (crate::action::ResearchSearchAction::ScopeChosen, Some(vec![])),
        (crate::action::ResearchSearchAction::ScopeAll, None),
    ] {
        app.research_search.structured.controller.view_scope = Some(app.structured_scope().unwrap());
        let effects = app.research_search_command(command);
        let cancel = effects.iter().position(|effect| matches!(effect, Effect::CancelResearchSearch)).expect("stale structured work canceled");
        let search = effects.iter().position(|effect| matches!(effect, Effect::ResearchSearch(_))).expect("fresh search dispatched");
        assert!(cancel < search, "cancel must precede the replacement search");
        assert_eq!(app.research_search.selected, selected);
        assert!(app.research_search.structured.controller.view_scope.is_none());
        assert!(matches!(&effects[search], Effect::ResearchSearch(request) if request.selected == selected));
    }
}
