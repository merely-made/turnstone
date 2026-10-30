// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::action::{Action, ResearchStructuredAction};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
use eidetic::records::*;

fn fixture() -> RecordResult {
    let source = RecallSourceId {
        kind: RecallSourceKind::Capture,
        owner: Some("owner".into()),
        source: "source".into(),
        node: None,
    };
    let definition = PropertyDefinitionV1 {
        id: "example.property".into(),
        revision: "r1".into(),
        value_type: ValueType::Text,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec![],
    };
    let record = PropertyRecordV1 {
        id: "filtered-out".into(),
        entity: "entity".into(),
        property: definition.id.clone(),
        schema_revision: definition.revision.clone(),
        definition_manifest: eidetic::ManifestId::of_blob(b"definition"),
        context: "context".into(),
        value: PropertyValue::Text("水".repeat(300)),
        valid_time: None,
        status: None,
        author: "author".into(),
        method: "manual".into(),
        evidence: Evidence::Field {
            source: source.clone(),
            version: "v1".into(),
            record_id: "record".into(),
            field: "field".into(),
        },
        recorded_at: None,
    };
    RecordResult {
        operation_version: "v1".into(),
        snapshot: RecordSnapshot {
            scope_generation: "scope".into(),
            complete: true,
            exclusions: vec![],
            sources: vec![
                SourceRecord {
                    source: source.clone(),
                    version: "v1".into(),
                    producer: "capture".into(),
                    captured_at: Some(4),
                    tags: vec![],
                },
                SourceRecord {
                    source,
                    version: "v2".into(),
                    producer: "capture".into(),
                    captured_at: Some(4),
                    tags: vec![],
                },
            ],
            definitions: vec![definition],
            records: vec![record],
            lifecycle: vec![],
        },
        query: RecordQuery {
            operation: RecordOperation::SourceTimeline,
            filters: RecordFilters::default(),
        },
        settings: RecordSettings::default(),
        rows: vec![],
    }
}
#[test]
fn model_free_input_lists_include_filtered_records_and_definitions() {
    let result = fixture();
    let generation = uuid::Uuid::new_v4();
    let mut rows = vec![];
    input_list(&mut rows, &result, generation, InputKind::Records, 0, 10);
    assert!(rows.iter().any(|row| matches!(
        row,
        Suggestion::Act {
            action: Action::ResearchStructured(ResearchStructuredAction::Frozen {
                target: FrozenTarget::Record(0),
                ..
            }),
            ..
        }
    )));
    rows.clear();
    input_list(
        &mut rows,
        &result,
        generation,
        InputKind::Definitions,
        0,
        10,
    );
    assert!(rows.iter().any(|row| matches!(
        row,
        Suggestion::Act {
            action: Action::ResearchStructured(ResearchStructuredAction::Frozen {
                target: FrozenTarget::Definition(0),
                ..
            }),
            ..
        }
    )));
}
#[test]
fn model_free_source_versions_remain_distinct_and_open_exact_source() {
    let mut result = fixture();
    result.rows.push(RecordRow {
        key: RowValue::Time(Some(4)),
        group: None,
        count: Some(2),
        record_ids: vec![],
        sources: vec![result.snapshot.sources[0].source.clone()],
    });
    let generation = uuid::Uuid::new_v4();
    let index = Index::new(
        generation,
        &result.rows,
        &result.snapshot.sources,
        &result.snapshot.records,
    )
    .unwrap();
    let mut rows = vec![];
    row_source(&mut rows, &result, &index, generation, 0, 0, 0, 10);
    for source in 0..2 {
        assert!(rows.iter().any(|row| matches!(row, Suggestion::Act { action: Action::ResearchStructured(ResearchStructuredAction::Frozen { target: FrozenTarget::Source(i), .. }), .. } if *i == source)));
    }
}

#[test]
fn model_free_frozen_inspector_exposes_checked_source_and_record_routes() {
    let mut controller = Controller::default();
    controller.result = Some(fixture());
    let generation = uuid::Uuid::new_v4();
    controller.result_generation = Some(generation);
    let mut rows = vec![];
    frozen_fields(
        &mut rows,
        &controller,
        Some(generation),
        FrozenTarget::Source(1),
        0,
        30,
    );
    assert!(rows.iter().any(|row| matches!(row, Suggestion::Act { action: Action::ResearchStructured(ResearchStructuredAction::OpenResultSource { generation: g, source: 1 }), .. } if *g == generation)));
    rows.clear();
    frozen_fields(
        &mut rows,
        &controller,
        Some(generation),
        FrozenTarget::Record(0),
        0,
        30,
    );
    assert!(rows.iter().any(|row| matches!(row, Suggestion::Act { action: Action::ResearchStructured(ResearchStructuredAction::OpenFrozenEvidence { generation: g, record: 0 }), .. } if *g == generation)));
}

#[test]
fn model_free_timeline_rows_and_counts_show_each_contributor() {
    let mut result = fixture();
    let first = result.snapshot.sources[0].source.clone();
    let mut second = first.clone();
    second.source = "other".into();
    result.snapshot.sources.push(SourceRecord {
        source: second.clone(),
        version: "v1".into(),
        producer: "capture".into(),
        captured_at: Some(4),
        tags: vec![],
    });
    result.rows = vec![
        RecordRow {
            key: RowValue::Time(Some(4)),
            group: None,
            count: None,
            record_ids: vec![],
            sources: vec![first.clone()],
        },
        RecordRow {
            key: RowValue::Time(Some(4)),
            group: None,
            count: None,
            record_ids: vec![],
            sources: vec![second.clone()],
        },
        RecordRow {
            key: RowValue::Text("capture".into()),
            group: None,
            count: Some(2),
            record_ids: vec![],
            sources: vec![first, second],
        },
    ];
    let generation = uuid::Uuid::new_v4();
    let index = Index::new(
        generation,
        &result.rows,
        &result.snapshot.sources,
        &result.snapshot.records,
    )
    .unwrap();
    let mut rows = vec![];
    super::super::result_rows::result_rows(&mut rows, &result, &index, 0, 20);
    assert!(rows.iter().any(|r| matches!(r, Suggestion::Act { label, .. } if label.contains("1970-01-01T00:00:04Z") && label.contains("source source"))));
    assert!(rows.iter().any(|r| matches!(r, Suggestion::Act { label, .. } if label.contains("1970-01-01T00:00:04Z") && label.contains("source other"))));
    for source in 0..2 {
        assert!(rows.iter().any(|r| matches!(r, Suggestion::Act { action: Action::ResearchStructured(ResearchStructuredAction::RowSource { row: 2, source: s, .. }), .. } if *s == source)));
    }
}

#[test]
fn model_free_result_replacement_rebuilds_cached_index_and_rejects_stale_actions() {
    let root = tempfile::tempdir().unwrap();
    let mut app = super::super::super::App::test_stub_at(root.path().to_path_buf());
    let old = uuid::Uuid::new_v4();
    app.research_search.structured.controller.result = Some(fixture());
    app.research_search.structured.controller.result_generation = Some(old);
    app.research_structured_rows(10);
    assert_eq!(
        app.research_search
            .structured
            .index
            .as_ref()
            .unwrap()
            .generation,
        old
    );
    let new = uuid::Uuid::new_v4();
    app.research_search.structured.controller.result = Some(fixture());
    app.research_search.structured.controller.result_generation = Some(new);
    app.research_structured_rows(10);
    assert_eq!(
        app.research_search
            .structured
            .index
            .as_ref()
            .unwrap()
            .generation,
        new
    );
    app.research_structured_command(ResearchStructuredAction::Frozen {
        generation: Some(old),
        target: FrozenTarget::Record(0),
    });
    assert!(!matches!(
        app.research_search.structured.view,
        super::super::super::research_structured_state::View::Frozen { .. }
    ));
}
