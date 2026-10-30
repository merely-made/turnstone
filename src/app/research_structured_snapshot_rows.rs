// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::super::{
    research_record_owner::Controller,
    research_structured_state::{FrozenTarget, InputKind, index::Index},
};
use super::{
    Command,
    format::{clipped, item, page_nav, source_id},
    snapshot,
};
use crate::ui::Suggestion;
use eidetic::records::RecordResult;
#[cfg(test)]
#[path = "research_structured_snapshot_rows_tests.rs"]
mod tests;

pub(super) fn input_menu(rows: &mut Vec<Suggestion>) {
    for (label, kind) in [
        ("Sources", InputKind::Sources),
        ("Records", InputKind::Records),
        ("Definitions", InputKind::Definitions),
        ("Lifecycle observations", InputKind::Lifecycle),
        ("Exclusions", InputKind::Exclusions),
        ("Query and settings", InputKind::Query),
    ] {
        item(rows, label, Command::InputList(kind));
    }
    item(rows, "Back to results", Command::Home);
}
pub(super) fn input_list(
    rows: &mut Vec<Suggestion>,
    result: &RecordResult,
    generation: uuid::Uuid,
    kind: InputKind,
    page: usize,
    size: usize,
) {
    if kind == InputKind::Query {
        item(
            rows,
            "Inspect exact query and settings",
            Command::Frozen {
                generation: Some(generation),
                target: FrozenTarget::Query,
            },
        );
        item(rows, "Back", Command::Inputs);
        return;
    }
    let total = match kind {
        InputKind::Sources => result.snapshot.sources.len(),
        InputKind::Records => result.snapshot.records.len(),
        InputKind::Definitions => result.snapshot.definitions.len(),
        InputKind::Lifecycle => result.snapshot.lifecycle.len(),
        InputKind::Exclusions => result.snapshot.exclusions.len(),
        InputKind::Query => 0,
    };
    let start = page.saturating_mul(size);
    for index in start..start.saturating_add(size).min(total) {
        let (label, target) = match kind {
            InputKind::Sources => {
                let s = &result.snapshot.sources[index];
                (
                    format!(
                        "Source {} · version {}",
                        source_id(&s.source),
                        clipped(&s.version, 80)
                    ),
                    FrozenTarget::Source(index),
                )
            },
            InputKind::Records => {
                let r = &result.snapshot.records[index];
                (
                    format!(
                        "Record {} · entity {} · property {}",
                        clipped(&r.id, 80),
                        clipped(&r.entity, 80),
                        clipped(&r.property, 80)
                    ),
                    FrozenTarget::Record(index),
                )
            },
            InputKind::Definitions => {
                let d = &result.snapshot.definitions[index];
                (
                    format!(
                        "Definition {} · revision {}",
                        clipped(&d.id, 100),
                        clipped(&d.revision, 60)
                    ),
                    FrozenTarget::Definition(index),
                )
            },
            InputKind::Lifecycle => {
                let l = &result.snapshot.lifecycle[index];
                (
                    format!("Lifecycle {} · {:?}", clipped(&l.record_id, 100), l.state),
                    FrozenTarget::Lifecycle(index),
                )
            },
            InputKind::Exclusions => (
                format!(
                    "Exclusion {}",
                    clipped(&result.snapshot.exclusions[index], 160)
                ),
                FrozenTarget::Exclusion(index),
            ),
            InputKind::Query => unreachable!(),
        };
        item(
            rows,
            label,
            Command::Frozen {
                generation: Some(generation),
                target,
            },
        );
    }
    page_nav(rows, page, size, total);
    item(rows, "Back", Command::Inputs);
}
pub(super) fn row_source(
    rows: &mut Vec<Suggestion>,
    result: &RecordResult,
    index: &Index,
    generation: uuid::Uuid,
    row: usize,
    source: usize,
    page: usize,
    size: usize,
) {
    if index.generation != generation {
        return;
    }
    let Some(id) = result.rows.get(row).and_then(|r| r.sources.get(source)) else {
        return;
    };
    item(
        rows,
        format!("Contributing source {}", source_id(id)),
        Command::Home,
    );
    let versions = index.source_versions(id);
    if versions.is_empty() {
        item(
            rows,
            "Exact source version unavailable in frozen inputs",
            Command::Home,
        );
    }
    for &source_index in versions.iter().skip(page.saturating_mul(size)).take(size) {
        let s = &result.snapshot.sources[source_index];
        item(
            rows,
            format!("Inspect version {}", clipped(&s.version, 120)),
            Command::Frozen {
                generation: Some(generation),
                target: FrozenTarget::Source(source_index),
            },
        );
    }
    page_nav(rows, page, size, versions.len());
    item(rows, "Back to results", Command::Home);
}
fn record(controller: &Controller) -> Option<&eidetic::records::PropertyRecordV1> {
    controller.inspected.as_ref().map(|(_, r)| r)
}
pub(super) fn frozen_fields(
    rows: &mut Vec<Suggestion>,
    controller: &Controller,
    generation: Option<uuid::Uuid>,
    target: FrozenTarget,
    page: usize,
    size: usize,
) {
    let total = snapshot::field_count(controller.result.as_ref(), record(controller), target);
    let start = page.saturating_mul(size);
    for field in start..start.saturating_add(size).min(total) {
        if let Some((label, text)) = snapshot::field(
            controller.result.as_ref(),
            record(controller),
            target,
            field,
        ) {
            item(
                rows,
                format!("{label}: {}", clipped(&text, 160)),
                Command::FrozenPart {
                    generation,
                    target,
                    field,
                    start: 0,
                },
            );
        }
    }
    page_nav(rows, page, size, total);
    match (generation, target) {
        (Some(generation), FrozenTarget::Source(source)) => item(
            rows,
            "Open exact retained source",
            Command::OpenResultSource { generation, source },
        ),
        (Some(generation), FrozenTarget::Record(record)) => item(
            rows,
            "Open exact record evidence",
            Command::OpenFrozenEvidence { generation, record },
        ),
        (None, FrozenTarget::Inspected) => {
            if let Some((binding, _)) = &controller.inspected {
                item(
                    rows,
                    "Open exact record evidence",
                    Command::OpenEvidence(binding.clone()),
                );
                item(
                    rows,
                    "Withdraw this assertion",
                    Command::Withdraw(binding.clone()),
                );
            }
        },
        _ => {},
    }
    item(
        rows,
        "Back",
        if target == FrozenTarget::Inspected {
            Command::Home
        } else {
            Command::Inputs
        },
    );
}
pub(super) fn frozen_part(
    rows: &mut Vec<Suggestion>,
    controller: &Controller,
    generation: Option<uuid::Uuid>,
    target: FrozenTarget,
    field: usize,
    start: usize,
) {
    if let Some((label, text)) = snapshot::field(
        controller.result.as_ref(),
        record(controller),
        target,
        field,
    ) {
        if let Some((piece, next, previous)) = snapshot::part(&text, start) {
            item(
                rows,
                format!("{label}: {piece}"),
                Command::FrozenPart {
                    generation,
                    target,
                    field,
                    start,
                },
            );
            if let Some(start) = previous {
                item(
                    rows,
                    "Previous part",
                    Command::FrozenPart {
                        generation,
                        target,
                        field,
                        start,
                    },
                );
            }
            if let Some(start) = next {
                item(
                    rows,
                    "Next part",
                    Command::FrozenPart {
                        generation,
                        target,
                        field,
                        start,
                    },
                );
            }
        }
    }
    item(
        rows,
        "Back to fields",
        Command::Frozen { generation, target },
    );
}
