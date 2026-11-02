// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::Command;
use crate::{action::Action, ui::Suggestion};
use eidetic::records::{Comparison, PropertyValue, RecordOperation, RowValue};

pub(super) fn item(rows: &mut Vec<Suggestion>, label: impl Into<String>, command: Command) {
    rows.push(Suggestion::Act {
        label: label.into(),
        action: Action::ResearchStructured(command),
    });
}
pub(super) fn field(rows: &mut Vec<Suggestion>, label: &str, value: &str, key: &str) {
    item(
        rows,
        format!(
            "{label}: {}",
            if value.is_empty() {
                "unknown / unset".into()
            } else {
                clipped(value, 160)
            }
        ),
        Command::Input(key.into()),
    );
}
pub(super) fn operation(op: &RecordOperation) -> &'static str {
    match op {
        RecordOperation::SourceLookup => "Source lookup",
        RecordOperation::CountByProducer => "Count by source kind",
        RecordOperation::CountBySource => "Count by source",
        RecordOperation::SourceTimeline => "Source timeline",
        RecordOperation::RecordTimeline => "Record timeline",
        RecordOperation::TagMembership => "Tag membership",
        RecordOperation::PropertyLookup => "Property lookup",
        RecordOperation::GroupValues => "Group equal values",
        RecordOperation::JoinEntity => "Join by entity ID",
        RecordOperation::JoinSource => "Join by source ID",
        RecordOperation::Compare => "Compare properties",
        RecordOperation::Sum => "Sum compatible values",
    }
}
pub(super) fn value(v: &PropertyValue) -> String {
    match v {
        PropertyValue::Text(v) => v.clone(),
        PropertyValue::Boolean(v) => v.to_string(),
        PropertyValue::Integer(v) => v.to_string(),
        PropertyValue::Decimal {
            coefficient,
            scale,
            unit,
        } => format!("{coefficient} × 10^-{scale} {unit}"),
        PropertyValue::CivilDate(d) => format!("{:04}-{:02}-{:02}", d.year, d.month, d.day),
        PropertyValue::Timestamp(t) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} (offset {} min)",
            t.date.year, t.date.month, t.date.day, t.hour, t.minute, t.second, t.offset_minutes
        ),
    }
}
pub(super) fn row_value(v: &RowValue) -> String {
    match v {
        RowValue::Source(s) => format!("Source {}", source_id(s)),
        RowValue::Text(s) => s.clone(),
        RowValue::Time(t) => t
            .map(|t| {
                jiff::Timestamp::from_second(t)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|_| format!("Out-of-range timestamp ({t})"))
            })
            .unwrap_or_else(|| "Time unknown".into()),
        RowValue::Value(v) => value(v),
        RowValue::Count(n) => n.to_string(),
        RowValue::Comparison(c) => match c {
            Comparison::Equal => "Equal",
            Comparison::Disagreement => "Disagreement",
            Comparison::Distinct => "Distinct",
            Comparison::NonComparable => "Not comparable",
        }
        .into(),
        RowValue::Pair(a, b) => format!("{a} ↔ {b}"),
    }
}
pub(super) fn clipped(s: &str, limit: usize) -> String {
    clip_chars(s.chars(), limit)
}
pub(crate) fn clip_chars(mut chars: impl Iterator<Item = char>, limit: usize) -> String {
    let mut out: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}
pub(super) fn source_id(source: &eidetic::models::recall::RecallSourceId) -> String {
    format!(
        "{:?} · owner {} · source {} · node {}",
        source.kind,
        clipped(source.owner.as_deref().unwrap_or("unknown"), 100),
        clipped(&source.source, 100),
        clipped(source.node.as_deref().unwrap_or("unknown"), 100)
    )
}
pub(super) fn page_nav(rows: &mut Vec<Suggestion>, page: usize, size: usize, total: usize) {
    if page > 0 {
        item(rows, "Previous page", Command::Page(-1));
    }
    if page.saturating_mul(size).saturating_add(size) < total {
        item(rows, "Next page", Command::Page(1));
    }
}

#[cfg(test)]
pub(crate) fn group_fields(
    group: &eidetic::records::PropertyGroupKey,
) -> Vec<(&'static str, String)> {
    vec![
        ("Entity", group.entity.clone()),
        ("Property", group.property.clone()),
        ("Schema revision", group.schema_revision.clone()),
        ("Context", group.context.clone()),
        (
            "Status",
            group.status.clone().unwrap_or_else(|| "unknown".into()),
        ),
        (
            "Valid time",
            group
                .valid_time
                .as_ref()
                .map(|t| format!("{} to {} UTC seconds (end exclusive)", t.start, t.end))
                .unwrap_or_else(|| "unknown".into()),
        ),
        ("Value", value(&group.value)),
    ]
}
pub(super) fn group_summary(group: &eidetic::records::PropertyGroupKey) -> String {
    format!(
        "entity {} · property {} · revision {} · context {} · status {} · time {}",
        clipped(&group.entity, 40),
        clipped(&group.property, 40),
        clipped(&group.schema_revision, 20),
        clipped(&group.context, 40),
        group
            .status
            .as_ref()
            .map(|s| clipped(s, 30))
            .unwrap_or_else(|| "unknown".into()),
        group
            .valid_time
            .as_ref()
            .map(|t| format!("{}..{}", t.start, t.end))
            .unwrap_or_else(|| "unknown".into())
    )
}
fn group_field(
    group: &eidetic::records::PropertyGroupKey,
    field: usize,
) -> Option<(&'static str, std::borrow::Cow<'_, str>)> {
    use std::borrow::Cow;
    Some(match field {
        0 => ("Entity", Cow::Borrowed(&group.entity)),
        1 => ("Property", Cow::Borrowed(&group.property)),
        2 => ("Schema revision", Cow::Borrowed(&group.schema_revision)),
        3 => ("Context", Cow::Borrowed(&group.context)),
        4 => (
            "Status",
            Cow::Borrowed(group.status.as_deref().unwrap_or("unknown")),
        ),
        5 => (
            "Valid time",
            Cow::Owned(
                group
                    .valid_time
                    .as_ref()
                    .map(|t| format!("{} to {} UTC seconds (end exclusive)", t.start, t.end))
                    .unwrap_or_else(|| "unknown".into()),
            ),
        ),
        6 => (
            "Value",
            match &group.value {
                PropertyValue::Text(s) => Cow::Borrowed(s),
                PropertyValue::Decimal {
                    coefficient, scale, ..
                } => Cow::Owned(format!("{coefficient} × 10^-{scale}")),
                v => Cow::Owned(value(v)),
            },
        ),
        7 => match &group.value {
            PropertyValue::Decimal { unit, .. } => ("Unit", Cow::Borrowed(unit)),
            _ => return None,
        },
        _ => return None,
    })
}
pub(super) fn group_rows(
    rows: &mut Vec<Suggestion>,
    group: &eidetic::records::PropertyGroupKey,
    generation: uuid::Uuid,
    index: usize,
) {
    for field in 0..8 {
        if let Some((name, text)) = group_field(group, field) {
            item(
                rows,
                format!("{name}: {}", clipped(&text, 160)),
                Command::GroupPart {
                    generation,
                    index,
                    field,
                    start: 0,
                },
            );
        }
    }
}
pub(super) fn group_part_rows(
    rows: &mut Vec<Suggestion>,
    group: &eidetic::records::PropertyGroupKey,
    generation: uuid::Uuid,
    index: usize,
    field: usize,
    start: usize,
) {
    let Some((name, text)) = group_field(group, field) else {
        return;
    };
    if start > text.len() || !text.is_char_boundary(start) {
        return;
    }
    let rest = &text[start..];
    let end = rest
        .char_indices()
        .nth(200)
        .map(|(i, _)| start + i)
        .unwrap_or(text.len());
    item(
        rows,
        format!("{name}: {}", &text[start..end]),
        Command::GroupPart {
            generation,
            index,
            field,
            start,
        },
    );
    if start > 0 {
        let previous = text[..start]
            .char_indices()
            .rev()
            .nth(199)
            .map(|(i, _)| i)
            .unwrap_or(0);
        item(
            rows,
            "Previous part",
            Command::GroupPart {
                generation,
                index,
                field,
                start: previous,
            },
        );
    }
    if end < text.len() {
        item(
            rows,
            "Next part",
            Command::GroupPart {
                generation,
                index,
                field,
                start: end,
            },
        );
    }
}

pub(crate) fn recovery_rows<'a>(
    rows: &mut Vec<Suggestion>,
    choices: impl Iterator<Item = (uuid::Uuid, &'a str, &'a str, bool)>,
    page: usize,
    size: usize,
    total: usize,
) {
    item(rows, "Refresh pending saves", Command::RefreshRecovery);
    for (nonce, kind, status, orphan) in choices.skip(page.saturating_mul(size)).take(size) {
        item(
            rows,
            format!(
                "Retry save · {} · status {} · operation ID {}",
                clipped(kind, 60),
                clipped(status, 100),
                nonce
            ),
            Command::RetryRecovery(nonce),
        );
        if orphan {
            item(
                rows,
                format!("Set aside unsaved save · operation ID {nonce}"),
                Command::AbandonRecovery(nonce),
            );
        }
    }
    page_nav(rows, page, size, total);
    item(rows, "Back", Command::Home);
}

pub(super) fn settings_rows(
    rows: &mut Vec<Suggestion>,
    s: &eidetic::records::RecordSettings,
    storage: &crate::research_storage::StorageLimits,
    active: Option<&crate::research_storage::StorageLimits>,
) {
    for (label, key, n) in [
        ("Sources", "record_max_sources", s.max_sources),
        ("Records", "record_max_records", s.max_records),
        ("Work", "record_max_work", s.max_work),
        (
            "Schema validation bytes",
            "record_max_validation_bytes",
            s.max_validation_bytes,
        ),
        ("Rows", "record_max_rows", s.max_rows),
        ("Input bytes", "record_max_input_bytes", s.max_input_bytes),
        (
            "Output bytes",
            "record_max_output_bytes",
            s.max_output_bytes,
        ),
    ] {
        field(rows, label, &n.to_string(), key);
    }
    let active_fields = active.map(|limits| limits.fields()).unwrap_or_default();
    for (label, key, value) in storage.fields() {
        let active_value = active_fields
            .iter()
            .find(|(_, active_key, _)| active_key == &key)
            .map(|(_, _, value)| value.as_str())
            .unwrap_or("unavailable");
        field(
            rows,
            &format!("{label} (active {active_value})"),
            &value,
            &key,
        );
    }
    item(
        rows,
        "Storage settings take effect after restarting Turnstone",
        Command::Settings,
    );
    item(rows, "Back", Command::Home);
}

pub(super) fn preview_value(v: &PropertyValue, limit: usize) -> String {
    match v {
        PropertyValue::Text(s) => clipped(s, limit),
        PropertyValue::Decimal {
            coefficient,
            scale,
            unit,
        } => format!("{coefficient} × 10^-{scale} {}", clipped(unit, limit)),
        _ => value(v),
    }
}

pub(super) fn home_scope(
    rows: &mut Vec<Suggestion>,
    selected: &Option<Vec<crate::research_search::Member>>,
) {
    let label = match selected {
        None => "Scope: All admitted sources".into(),
        Some(sources) if sources.is_empty() => "Scope: No sources chosen".into(),
        Some(sources) => format!("Scope: {} chosen", sources.len()),
    };
    item(rows, label, Command::Scope);
}

pub(super) fn result_operation(op: &RecordOperation, historical: bool) -> String {
    if historical {
        format!("{} · frozen historical input", operation(op))
    } else {
        operation(op).into()
    }
}
