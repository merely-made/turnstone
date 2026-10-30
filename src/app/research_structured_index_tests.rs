// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::{Entry, Index};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
use eidetic::records::{RecordRow, RowValue, SourceRecord};

fn source(name: &str) -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::Capture,
        owner: Some("owner".into()),
        source: name.into(),
        node: None,
    }
}
fn row(sources: Vec<RecallSourceId>, records: Vec<String>) -> RecordRow {
    RecordRow {
        key: RowValue::Time(Some(17)),
        group: None,
        count: None,
        record_ids: records,
        sources,
    }
}
#[test]
fn model_free_index_preserves_same_time_sources_and_count_contributors() {
    let a = source("a");
    let b = source("b");
    let rows = vec![
        row(vec![a.clone()], vec![]),
        row(vec![b.clone()], vec![]),
        row(vec![a.clone(), b.clone()], vec![]),
    ];
    let sources = vec![
        SourceRecord {
            source: a.clone(),
            version: "v1".into(),
            producer: "capture".into(),
            captured_at: Some(17),
            tags: vec![],
        },
        SourceRecord {
            source: b.clone(),
            version: "v2".into(),
            producer: "capture".into(),
            captured_at: Some(17),
            tags: vec![],
        },
    ];
    let index = Index::new(uuid::Uuid::new_v4(), &rows, &sources, &[]).unwrap();
    assert_eq!(index.total(), 7);
    assert_eq!(index.locate(1), Some(Entry::Source { row: 0, source: 0 }));
    assert_eq!(index.locate(3), Some(Entry::Source { row: 1, source: 0 }));
    assert_eq!(index.locate(5), Some(Entry::Source { row: 2, source: 0 }));
    assert_eq!(index.locate(6), Some(Entry::Source { row: 2, source: 1 }));
    assert_eq!(index.source_versions(&a), &[0]);
    assert_eq!(index.source_versions(&b), &[1]);
}
#[test]
fn model_free_large_result_random_navigation_is_logarithmic() {
    let rows = vec![row(vec![], vec!["record".into()]); 100_000];
    let index = Index::new(uuid::Uuid::new_v4(), &rows, &[], &[]).unwrap();
    let mut comparisons = 0;
    assert_eq!(
        index.locate_counted(199_999, || comparisons += 1),
        Some(Entry::Record {
            row: 99_999,
            record: 0
        })
    );
    assert!(comparisons <= 20, "{comparisons} comparisons");
    assert_eq!(index.locate(index.total()), None);
}
