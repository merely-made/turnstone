// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::{definition_field, part, record_field};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
use eidetic::records::{
    Cardinality, Evidence, PropertyDefinitionV1, PropertyRecordV1, PropertyValue, ValueType,
};

#[test]
fn model_free_frozen_record_and_definition_fields_are_exact_beyond_preview() {
    let long = format!("{}終", "水".repeat(250));
    let def = PropertyDefinitionV1 {
        id: long.clone(),
        revision: "r1".into(),
        value_type: ValueType::Text,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec!["approved".into()],
    };
    let source = RecallSourceId {
        kind: RecallSourceKind::Capture,
        owner: Some("owner".into()),
        source: "source".into(),
        node: None,
    };
    let record = PropertyRecordV1 {
        id: "filtered-out".into(),
        entity: "entity".into(),
        property: def.id.clone(),
        schema_revision: def.revision.clone(),
        definition_manifest: eidetic::ManifestId::of_blob(b"definition"),
        context: long.clone(),
        value: PropertyValue::Text(long.clone()),
        valid_time: None,
        status: None,
        author: "author".into(),
        method: "manual".into(),
        evidence: Evidence::Field {
            source,
            version: "v1".into(),
            record_id: "producer-record".into(),
            field: "field".into(),
        },
        recorded_at: None,
    };
    assert_eq!(record_field(&record, 5).unwrap().1, long);
    assert_eq!(record_field(&record, 6).unwrap().1, long);
    assert_eq!(definition_field(&def, 0).unwrap().1, long);
    let (first, next, _) = part(&long, 0).unwrap();
    let (second, end, previous) = part(&long, next.unwrap()).unwrap();
    assert_eq!(format!("{first}{second}"), long);
    assert!(end.is_none());
    assert_eq!(previous, Some(0));
}
