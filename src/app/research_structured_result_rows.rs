// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::super::research_structured_state::{
    FrozenTarget,
    index::{Entry, Index},
};
use super::{
    Command,
    format::{clipped, group_summary, item, page_nav, preview_value, row_value, source_id},
};
use crate::ui::Suggestion;
use eidetic::records::{RecordResult, RowValue};
fn preview(row: &RowValue) -> String {
    match row {
        RowValue::Text(s) => clipped(s, 160),
        RowValue::Value(v) => preview_value(v, 160),
        RowValue::Pair(a, b) => format!("{} ↔ {}", clipped(a, 80), clipped(b, 80)),
        _ => clipped(&row_value(row), 160),
    }
}
pub(super) fn result_rows(
    rows: &mut Vec<Suggestion>,
    result: &RecordResult,
    index: &Index,
    page: usize,
    size: usize,
) {
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size).min(index.total());
    for position in start..end {
        let Some(entry) = index.locate(position) else {
            break;
        };
        let row_index = match entry {
            Entry::Row(i) => i,
            Entry::Record { row, .. } | Entry::Source { row, .. } => row,
        };
        let row = &result.rows[row_index];
        match entry {
            Entry::Row(_) => {
                let mut label = preview(&row.key);
                if let Some(count) = row.count {
                    label.push_str(&format!(" · count {count}"));
                }
                if let Some(source) = row.sources.first() {
                    label.push_str(&format!(" · source {}", source_id(source)));
                    if row.sources.len() > 1 {
                        label.push_str(&format!(" · {} sources", row.sources.len()));
                    }
                }
                if let Some(group) = &row.group {
                    label.push_str(&format!(" · {}", group_summary(group)));
                }
                item(
                    rows,
                    label,
                    if row.group.is_some() {
                        Command::Group {
                            generation: index.generation,
                            index: row_index,
                        }
                    } else if row.sources.len() == 1 {
                        Command::RowSource {
                            generation: index.generation,
                            row: row_index,
                            source: 0,
                        }
                    } else {
                        Command::Home
                    },
                );
            },
            Entry::Record { record, .. } => {
                let id = &row.record_ids[record];
                match index.record_index(id) {
                    Some(i) => item(
                        rows,
                        format!("Inspect record {}", clipped(id, 100)),
                        Command::Frozen {
                            generation: Some(index.generation),
                            target: FrozenTarget::Record(i),
                        },
                    ),
                    None => item(
                        rows,
                        format!("Record {} unavailable or ambiguous", clipped(id, 100)),
                        Command::Home,
                    ),
                }
            },
            Entry::Source { source, .. } => item(
                rows,
                format!(
                    "Inspect contributing source {}",
                    source_id(&row.sources[source])
                ),
                Command::RowSource {
                    generation: index.generation,
                    row: row_index,
                    source,
                },
            ),
        }
    }
    page_nav(rows, page, size, index.total());
}
