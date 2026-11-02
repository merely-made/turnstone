// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Read-only views over the accepted frozen payload; never grants evidence access.
use super::super::research_structured_state::FrozenTarget;
use eidetic::records::{
    Evidence, PropertyDefinitionV1, PropertyRecordV1, PropertyValue, RecordResult,
};
use std::borrow::Cow;
#[cfg(test)]
#[path = "research_structured_snapshot_tests.rs"]
mod tests;
type Field<'a> = (&'static str, Cow<'a, str>);
fn optional(value: Option<&str>) -> Cow<'_, str> {
    Cow::Borrowed(value.unwrap_or("unknown"))
}
fn number(value: impl std::fmt::Display) -> Cow<'static, str> {
    Cow::Owned(value.to_string())
}
fn value_field(value: &PropertyValue, field: usize) -> Option<Field<'_>> {
    Some(match (value, field) {
        (PropertyValue::Text(v), 0) => ("Value", Cow::Borrowed(v)),
        (PropertyValue::Boolean(v), 0) => ("Value", number(v)),
        (PropertyValue::Integer(v), 0) => ("Value", number(v)),
        (PropertyValue::Decimal { coefficient, .. }, 0) => {
            ("Decimal coefficient", number(coefficient))
        },
        (PropertyValue::Decimal { scale, .. }, 1) => ("Decimal scale", number(scale)),
        (PropertyValue::Decimal { unit, .. }, 2) => ("Unit", Cow::Borrowed(unit)),
        (PropertyValue::CivilDate(d), 0) => (
            "Date",
            Cow::Owned(format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)),
        ),
        (PropertyValue::Timestamp(t), 0) => (
            "Timestamp",
            Cow::Owned(format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02} offset {} minutes",
                t.date.year, t.date.month, t.date.day, t.hour, t.minute, t.second, t.offset_minutes
            )),
        ),
        _ => return None,
    })
}
fn source_field(
    source: &eidetic::models::recall::RecallSourceId,
    field: usize,
) -> Option<Field<'_>> {
    Some(match field {
        0 => ("Source kind", Cow::Owned(format!("{:?}", source.kind))),
        1 => ("Source owner", optional(source.owner.as_deref())),
        2 => ("Source ID", Cow::Borrowed(&source.source)),
        3 => ("Source node", optional(source.node.as_deref())),
        _ => return None,
    })
}
pub(super) fn record_field(record: &PropertyRecordV1, field: usize) -> Option<Field<'_>> {
    Some(match field {
        0 => ("Record ID", Cow::Borrowed(&record.id)),
        1 => ("Entity ID", Cow::Borrowed(&record.entity)),
        2 => ("Property ID", Cow::Borrowed(&record.property)),
        3 => ("Schema revision", Cow::Borrowed(&record.schema_revision)),
        4 => ("Definition manifest", number(record.definition_manifest)),
        5 => ("Context", Cow::Borrowed(&record.context)),
        6..=8 => return value_field(&record.value, field - 6),
        9 => ("Status", optional(record.status.as_deref())),
        10 => (
            "Valid from",
            record
                .valid_time
                .as_ref()
                .map(|t| number(t.start))
                .unwrap_or_else(|| optional(None)),
        ),
        11 => (
            "Valid until (exclusive)",
            record
                .valid_time
                .as_ref()
                .map(|t| number(t.end))
                .unwrap_or_else(|| optional(None)),
        ),
        12 => ("Author", Cow::Borrowed(&record.author)),
        13 => ("Method", Cow::Borrowed(&record.method)),
        14 => (
            "Recorded at",
            record
                .recorded_at
                .map(number)
                .unwrap_or_else(|| optional(None)),
        ),
        15..=18 => return source_field(record.evidence.source(), field - 15),
        19 => ("Evidence version", Cow::Borrowed(record.evidence.version())),
        20 => match &record.evidence {
            Evidence::Passage { hash, .. } => ("Evidence hash", number(hash)),
            Evidence::Field { record_id, .. } => ("Evidence record ID", Cow::Borrowed(record_id)),
        },
        21 => match &record.evidence {
            Evidence::Passage { selector, .. } => ("Passage start", number(selector.start)),
            Evidence::Field { field, .. } => ("Evidence field", Cow::Borrowed(field)),
        },
        22 => match &record.evidence {
            Evidence::Passage { selector, .. } => ("Passage end", number(selector.end)),
            Evidence::Field { .. } => return None,
        },
        _ => return None,
    })
}
pub(super) fn definition_field(
    definition: &PropertyDefinitionV1,
    field: usize,
) -> Option<Field<'_>> {
    Some(match field {
        0 => ("Property ID", Cow::Borrowed(&definition.id)),
        1 => ("Schema revision", Cow::Borrowed(&definition.revision)),
        2 => (
            "Value type",
            Cow::Owned(format!("{:?}", definition.value_type)),
        ),
        3 => ("Unit", optional(definition.unit.as_deref())),
        4 => (
            "Cardinality",
            Cow::Owned(format!("{:?}", definition.cardinality)),
        ),
        n => (
            "Allowed status",
            Cow::Borrowed(definition.statuses.get(n - 5)?),
        ),
    })
}
pub(super) fn field_count(
    result: Option<&RecordResult>,
    inspected: Option<&PropertyRecordV1>,
    target: FrozenTarget,
) -> usize {
    match target {
        FrozenTarget::Inspected => inspected.map(|_| 23).unwrap_or(0),
        FrozenTarget::Source(i) => result
            .and_then(|r| r.snapshot.sources.get(i))
            .map(|s| 7usize.saturating_add(s.tags.len().saturating_mul(2)))
            .unwrap_or(0),
        FrozenTarget::Record(i) => result
            .and_then(|r| r.snapshot.records.get(i))
            .map(|_| 23)
            .unwrap_or(0),
        FrozenTarget::Definition(i) => result
            .and_then(|r| r.snapshot.definitions.get(i))
            .map(|d| 5usize.saturating_add(d.statuses.len()))
            .unwrap_or(0),
        FrozenTarget::Lifecycle(i) => result
            .and_then(|r| r.snapshot.lifecycle.get(i))
            .map(|_| 3)
            .unwrap_or(0),
        FrozenTarget::Exclusion(i) => result
            .and_then(|r| r.snapshot.exclusions.get(i))
            .map(|_| 1)
            .unwrap_or(0),
        FrozenTarget::Query => result.map(|_| 23).unwrap_or(0),
    }
}
pub(super) fn field<'a>(
    result: Option<&'a RecordResult>,
    inspected: Option<&'a PropertyRecordV1>,
    target: FrozenTarget,
    index: usize,
) -> Option<Field<'a>> {
    if target == FrozenTarget::Inspected {
        return record_field(inspected?, index);
    }
    let result = result?;
    match target {
        FrozenTarget::Source(i) => {
            let s = result.snapshot.sources.get(i)?;
            match index {
                0..=3 => source_field(&s.source, index),
                4 => Some(("Version", Cow::Borrowed(&s.version))),
                5 => Some(("Producer", Cow::Borrowed(&s.producer))),
                6 => Some((
                    "Captured at",
                    s.captured_at.map(number).unwrap_or_else(|| optional(None)),
                )),
                n => {
                    let t = s.tags.get((n - 7) / 2)?;
                    if (n - 7) % 2 == 0 {
                        Some(("Tag owner", Cow::Borrowed(&t.owner)))
                    } else {
                        Some(("Tag concept", Cow::Borrowed(&t.concept)))
                    }
                },
            }
        },
        FrozenTarget::Record(i) => record_field(result.snapshot.records.get(i)?, index),
        FrozenTarget::Definition(i) => definition_field(result.snapshot.definitions.get(i)?, index),
        FrozenTarget::Lifecycle(i) => {
            let l = result.snapshot.lifecycle.get(i)?;
            match index {
                0 => Some(("Record ID", Cow::Borrowed(&l.record_id))),
                1 => Some(("Lifecycle reference", Cow::Borrowed(&l.lifecycle_ref))),
                2 => Some(("Lifecycle state", Cow::Owned(format!("{:?}", l.state)))),
                _ => None,
            }
        },
        FrozenTarget::Exclusion(i) => {
            if index == 0 {
                Some((
                    "Exclusion",
                    Cow::Borrowed(result.snapshot.exclusions.get(i)?.as_str()),
                ))
            } else {
                None
            }
        },
        FrozenTarget::Query => query_field(result, index),
        FrozenTarget::Inspected => None,
    }
}
fn query_field(result: &RecordResult, index: usize) -> Option<Field<'_>> {
    let f = &result.query.filters;
    let s = &result.settings;
    Some(match index {
        0 => (
            "Operation",
            Cow::Owned(format!("{:?}", result.query.operation)),
        ),
        1 => (
            "Operation version",
            Cow::Borrowed(&result.operation_version),
        ),
        2 => (
            "Scope generation",
            Cow::Borrowed(&result.snapshot.scope_generation),
        ),
        3 => ("Entity filter", optional(f.entity.as_deref())),
        4 => ("Property filter", optional(f.property.as_deref())),
        5 => (
            "Schema revision filter",
            optional(f.schema_revision.as_deref()),
        ),
        6 => ("Status filter", optional(f.status.as_deref())),
        7 => ("Unit filter", optional(f.unit.as_deref())),
        8..=11 => {
            return f
                .source
                .as_ref()
                .and_then(|source| source_field(source, index - 8))
                .or(Some(("Source filter", optional(None))));
        },
        12 => (
            "Tag owner filter",
            optional(f.tag.as_ref().map(|t| t.owner.as_str())),
        ),
        13 => (
            "Tag concept filter",
            optional(f.tag.as_ref().map(|t| t.concept.as_str())),
        ),
        14 => (
            "Valid from filter",
            f.valid_time
                .as_ref()
                .map(|v| number(v.start))
                .unwrap_or_else(|| optional(None)),
        ),
        15 => (
            "Valid until filter",
            f.valid_time
                .as_ref()
                .map(|v| number(v.end))
                .unwrap_or_else(|| optional(None)),
        ),
        16 => ("Source limit", number(s.max_sources)),
        17 => ("Record limit", number(s.max_records)),
        18 => ("Work limit", number(s.max_work)),
        19 => ("Validation byte limit", number(s.max_validation_bytes)),
        20 => ("Row limit", number(s.max_rows)),
        21 => ("Input byte limit", number(s.max_input_bytes)),
        22 => ("Output byte limit", number(s.max_output_bytes)),
        _ => return None,
    })
}
/// A bounded Unicode chunk and direct next/previous byte offsets.
pub(super) fn part(text: &str, start: usize) -> Option<(&str, Option<usize>, Option<usize>)> {
    if start > text.len() || !text.is_char_boundary(start) {
        return None;
    }
    let end = text[start..]
        .char_indices()
        .nth(200)
        .map(|(i, _)| start + i)
        .unwrap_or(text.len());
    let previous = (start > 0).then(|| {
        text[..start]
            .char_indices()
            .rev()
            .nth(199)
            .map(|(i, _)| i)
            .unwrap_or(0)
    });
    Some((
        &text[start..end],
        (end < text.len()).then_some(end),
        previous,
    ))
}
