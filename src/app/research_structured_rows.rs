// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::{
    App,
    research_structured_state::{Command, View},
};
use eidetic::records::{Cardinality, RecordOperation, ValueType};

#[path = "research_structured_format.rs"]
pub(super) mod format;
#[path = "research_structured_result_rows.rs"]
mod result_rows;
#[path = "research_structured_snapshot.rs"]
mod snapshot;
#[path = "research_structured_snapshot_rows.rs"]
mod snapshot_rows;
use format::{clipped, field, group_rows, item, operation, page_nav, source_id};
impl App {
    pub(crate) fn research_structured_rows(&mut self, row_limit: usize) {
        {
            let state = &mut self.research_search.structured;
            let generation = state.controller.result_generation;
            if state.indexed_generation != generation {
                state.index = None;
                state.index_error = None;
                state.indexed_generation = generation;
                if let (Some(generation), Some(result)) =
                    (generation, state.controller.result.as_ref())
                {
                    match super::research_structured_state::index::Index::new(
                        generation,
                        &result.rows,
                        &result.snapshot.sources,
                        &result.snapshot.records,
                    ) {
                        Ok(index) => state.index = Some(index),
                        Err(error) => state.index_error = Some(error),
                    }
                }
                state.page = 0;
                if matches!(
                    state.view,
                    View::Frozen { .. }
                        | View::FrozenPart { .. }
                        | View::RowSource { .. }
                        | View::Group { .. }
                        | View::GroupPart { .. }
                        | View::InputList(_)
                ) {
                    state.view = View::Home;
                }
            }
        }
        let state = &self.research_search.structured;
        let mut rows = vec![];
        let size = self
            .research_search
            .preferences
            .settings
            .limit
            .clamp(1, 100);
        let start = state.page.saturating_mul(size);
        if let Some(error) = state.error.as_ref().or(state.controller.error.as_ref()) {
            item(
                &mut rows,
                format!("Unavailable: {}", clipped(error, 160)),
                Command::Home,
            );
        }
        if let Some(error) = &state.index_error {
            item(
                &mut rows,
                format!("Result navigation unavailable: {}", clipped(error, 160)),
                Command::Home,
            );
        }
        if state.controller.busy {
            item(&mut rows, "Working…", Command::Home);
        }
        match &state.view {
            View::Home => {
                item(
                    &mut rows,
                    format!("Operation: {}", operation(&state.operation)),
                    Command::Operations,
                );
                format::home_scope(&mut rows, &self.research_search.selected);
                item(&mut rows, "Filters…", Command::Filters);
                item(&mut rows, "Run in this scope", Command::Run);
                item(&mut rows, "Enter a sourced property…", Command::Author);
                item(&mut rows, "Saved results and replay…", Command::Saved);
                item(&mut rows, "Withdrawn record history…", Command::History);
                item(&mut rows, "Pending saves…", Command::Recovery);
                item(&mut rows, "Operation limits…", Command::Settings);
                if let Some(result) = &state.controller.result {
                    item(
                        &mut rows,
                        format!(
                            "{} · {} · {} inputs · {} records",
                            format::result_operation(
                                &result.query.operation,
                                state.controller.result_historical
                            ),
                            if result.snapshot.complete {
                                "complete"
                            } else {
                                "incomplete"
                            },
                            result.snapshot.sources.len(),
                            result.snapshot.records.len()
                        ),
                        Command::Home,
                    );
                    for exclusion in result.snapshot.exclusions.iter().take(10) {
                        item(
                            &mut rows,
                            format!("Excluded: {}", clipped(exclusion, 160)),
                            Command::Home,
                        );
                    }
                    if result.snapshot.complete && result.rows.is_empty() {
                        item(
                            &mut rows,
                            "No matching records in this scope",
                            Command::Home,
                        );
                    }
                    item(
                        &mut rows,
                        "Inspect exact query inputs and lifecycle…",
                        Command::Inputs,
                    );
                    let page_size = self
                        .research_search
                        .preferences
                        .settings
                        .limit
                        .clamp(1, 100);
                    if let Some(index) = &state.index {
                        result_rows::result_rows(&mut rows, result, index, state.page, page_size);
                    }
                }
                if let Some((_, record)) = &state.controller.inspected {
                    item(
                        &mut rows,
                        format!("Inspect record {}", clipped(&record.id, 100)),
                        Command::Frozen {
                            generation: None,
                            target: super::research_structured_state::FrozenTarget::Inspected,
                        },
                    );
                }

                item(&mut rows, "Back to search", Command::Back);
            },
            View::Inputs => snapshot_rows::input_menu(&mut rows),
            View::InputList(kind) => {
                if let (Some(result), Some(index)) = (&state.controller.result, &state.index) {
                    snapshot_rows::input_list(
                        &mut rows,
                        result,
                        index.generation,
                        *kind,
                        state.page,
                        size,
                    );
                }
            },
            View::RowSource {
                generation,
                row,
                source,
            } => {
                if let (Some(result), Some(index)) = (&state.controller.result, &state.index) {
                    snapshot_rows::row_source(
                        &mut rows,
                        result,
                        index,
                        *generation,
                        *row,
                        *source,
                        state.page,
                        size,
                    );
                }
            },
            View::Frozen { generation, target } => snapshot_rows::frozen_fields(
                &mut rows,
                &state.controller,
                *generation,
                *target,
                state.page,
                size,
            ),
            View::FrozenPart {
                generation,
                target,
                field,
                start,
            } => snapshot_rows::frozen_part(
                &mut rows,
                &state.controller,
                *generation,
                *target,
                *field,
                *start,
            ),
            View::Group { generation, index } => {
                if let Some(group) = state
                    .controller
                    .result
                    .as_ref()
                    .and_then(|r| r.rows.get(*index))
                    .and_then(|row| row.group.as_ref())
                {
                    group_rows(&mut rows, group, *generation, *index);
                }
                item(&mut rows, "Back to results", Command::Home);
            },
            View::GroupPart {
                generation,
                index,
                field,
                start,
            } => {
                if let Some(group) = state
                    .controller
                    .result
                    .as_ref()
                    .and_then(|r| r.rows.get(*index))
                    .and_then(|r| r.group.as_ref())
                {
                    format::group_part_rows(&mut rows, group, *generation, *index, *field, *start);
                }
                item(
                    &mut rows,
                    "Back to group",
                    Command::Group {
                        generation: *generation,
                        index: *index,
                    },
                );
            },
            View::Recovery => format::recovery_rows(
                &mut rows,
                state
                    .controller
                    .recovery
                    .iter()
                    .map(|p| (p.nonce, p.kind.as_str(), p.status.as_str(), p.orphan)),
                state.page,
                size,
                state.controller.recovery.len(),
            ),
            View::Operations => {
                for op in [
                    RecordOperation::SourceLookup,
                    RecordOperation::CountByProducer,
                    RecordOperation::CountBySource,
                    RecordOperation::SourceTimeline,
                    RecordOperation::RecordTimeline,
                    RecordOperation::TagMembership,
                    RecordOperation::PropertyLookup,
                    RecordOperation::GroupValues,
                    RecordOperation::JoinEntity,
                    RecordOperation::JoinSource,
                    RecordOperation::Compare,
                    RecordOperation::Sum,
                ] {
                    item(&mut rows, operation(&op), Command::Operation(op));
                }
                item(&mut rows, "Back", Command::Home);
            },
            View::Filters => {
                let f = &state.filters;
                item(
                    &mut rows,
                    format!(
                        "Source: {}",
                        f.source
                            .as_ref()
                            .map(source_id)
                            .unwrap_or_else(|| "any".into())
                    ),
                    Command::FilterSources,
                );
                field(
                    &mut rows,
                    "Entity ID",
                    f.entity.as_deref().unwrap_or(""),
                    "filter_entity",
                );
                field(
                    &mut rows,
                    "Property ID",
                    f.property.as_deref().unwrap_or(""),
                    "filter_property",
                );
                field(
                    &mut rows,
                    "Schema revision",
                    f.schema_revision.as_deref().unwrap_or(""),
                    "filter_revision",
                );
                field(
                    &mut rows,
                    "Status",
                    f.status.as_deref().unwrap_or(""),
                    "filter_status",
                );
                field(
                    &mut rows,
                    "Unit",
                    f.unit.as_deref().unwrap_or(""),
                    "filter_unit",
                );
                field(
                    &mut rows,
                    "Tag owner|concept",
                    &f.tag
                        .as_ref()
                        .map(|t| format!("{}|{}", clipped(&t.owner, 80), clipped(&t.concept, 80)))
                        .unwrap_or_default(),
                    "filter_tag",
                );
                field(&mut rows, "Valid from", &state.filter_start, "filter_start");
                field(
                    &mut rows,
                    "Valid until (exclusive)",
                    &state.filter_end,
                    "filter_end",
                );
                item(&mut rows, "Back", Command::Home);
            },
            View::FilterSources => {
                item(&mut rows, "Any source", Command::FilterSource(None));
                if let Some(result) = &self.research_search.result {
                    for source in result.inputs.iter().skip(start).take(size) {
                        item(
                            &mut rows,
                            format!(
                                "{} · {}",
                                clipped(source.title.as_deref().unwrap_or("Untitled"), 100),
                                source_id(&source.source)
                            ),
                            Command::FilterSource(Some(source.source.clone())),
                        );
                    }
                    page_nav(&mut rows, state.page, size, result.inputs.len());
                }
                item(&mut rows, "Back", Command::Filters);
            },
            View::Author => {
                let d = &state.draft;
                item(
                    &mut rows,
                    format!(
                        "Existing entity: {}",
                        d.entity
                            .map(|(_, id)| id.to_string())
                            .unwrap_or_else(|| "choose".into())
                    ),
                    Command::Entities,
                );
                field(
                    &mut rows,
                    "Namespaced property ID",
                    &d.definition,
                    "definition",
                );
                field(&mut rows, "Schema revision", &d.revision, "revision");
                item(&mut rows, "Value type:", Command::Author);
                for (kind, label) in [
                    (ValueType::Text, "Text"),
                    (ValueType::Boolean, "Boolean"),
                    (ValueType::Integer, "Integer"),
                    (ValueType::Decimal, "Decimal"),
                    (ValueType::CivilDate, "Date"),
                    (ValueType::Timestamp, "Timestamp"),
                ] {
                    item(
                        &mut rows,
                        format!(
                            "{} {label}",
                            if d.value_type.as_ref() == Some(&kind) {
                                "✓"
                            } else {
                                "○"
                            }
                        ),
                        Command::Type(kind),
                    );
                }
                for (v, label) in [
                    (Cardinality::Single, "Single value"),
                    (Cardinality::Multiple, "Multiple values"),
                ] {
                    item(
                        &mut rows,
                        format!(
                            "{} {label}",
                            if d.cardinality.as_ref() == Some(&v) {
                                "✓"
                            } else {
                                "○"
                            }
                        ),
                        Command::Cardinality(v),
                    );
                }
                field(&mut rows, "Unit", &d.unit, "unit");
                field(
                    &mut rows,
                    "Allowed statuses (comma separated)",
                    &d.statuses,
                    "statuses",
                );
                field(&mut rows, "Value", &d.value, "value");
                field(&mut rows, "Context", &d.context, "context");
                field(
                    &mut rows,
                    "Status (empty means unknown)",
                    &d.status,
                    "status",
                );
                field(&mut rows, "Valid from", &d.valid_start, "valid_start");
                field(
                    &mut rows,
                    "Valid until (exclusive)",
                    &d.valid_end,
                    "valid_end",
                );
                item(
                    &mut rows,
                    format!(
                        "Exact evidence: {}",
                        d.evidence
                            .as_ref()
                            .map(|t| match t {
                                crate::research::Target::Capture(t) =>
                                    clipped(&t.key.annotation_manifest, 100),
                                crate::research::Target::Clip(t) =>
                                    clipped(&t.clip.manifest.to_string(), 100),
                            })
                            .unwrap_or_else(|| "choose a search passage".into())
                    ),
                    Command::Evidence,
                );
                item(
                    &mut rows,
                    "Author: current profile identity · method: manual",
                    Command::Author,
                );
                item(&mut rows, "Save sourced property", Command::SaveProperty);
                item(&mut rows, "Back", Command::Home);
            },
            View::Entities => {
                match self.structured_entities() {
                    Ok(choices) => {
                        let total = choices.len();
                        for choice in choices.into_iter().skip(start).take(size) {
                            item(
                                &mut rows,
                                format!("{} · {}", clipped(&choice.label, 100), choice.entity),
                                Command::Entity(choice.graph, choice.entity),
                            );
                        }
                        page_nav(&mut rows, state.page, size, total);
                    },
                    Err(error) => item(
                        &mut rows,
                        format!("Entities unavailable: {}", clipped(&error, 160)),
                        Command::Author,
                    ),
                }
                item(&mut rows, "Back", Command::Author);
            },
            View::Evidence => {
                if let Some(result) = &self.research_search.result {
                    let total = result.hits.len();
                    for hit in result.hits.iter().skip(start).take(size) {
                        if let (Ok(mut target), Some(passage)) = (
                            crate::research::target(&hit.source, &hit.version),
                            &hit.passage,
                        ) {
                            target.set_selector(crate::saved_sources::Selector {
                                start: passage.selector.start,
                                end: passage.selector.end,
                            });
                            item(
                                &mut rows,
                                format!(
                                    "{} · {} · version {} · {}..{} · {}",
                                    clipped(hit.title.as_deref().unwrap_or("Untitled"), 100),
                                    source_id(&hit.source),
                                    clipped(&hit.version, 100),
                                    passage.selector.start,
                                    passage.selector.end,
                                    clipped(
                                        &passage.text,
                                        self.research_search
                                            .preferences
                                            .settings
                                            .snippet_context
                                            .clamp(1, 200)
                                    )
                                ),
                                Command::EvidenceTarget(target),
                            );
                        }
                    }
                    page_nav(&mut rows, state.page, size, total);
                }
                item(
                    &mut rows,
                    "Search for a passage first; each choice preserves its exact version and scalar range",
                    Command::Author,
                );
                item(&mut rows, "Back", Command::Author);
            },
            View::Settings => format::settings_rows(
                &mut rows,
                &self.research_search.preferences.records,
                &self.research_search.preferences.storage,
                self.research.storage_limits().ok().as_ref(),
            ),
            View::Saved => {
                match self.structured_saved_results() {
                    Ok(choices) => {
                        let total = choices.len();
                        for choice in choices.into_iter().skip(start).take(size) {
                            item(
                                &mut rows,
                                format!("Replay saved result {}", choice.binding.manifest),
                                Command::Replay(choice.binding.manifest),
                            );
                        }
                        page_nav(&mut rows, state.page, size, total);
                    },
                    Err(error) => item(
                        &mut rows,
                        format!("Saved results unavailable: {}", clipped(&error, 160)),
                        Command::Home,
                    ),
                }
                item(&mut rows, "Back", Command::Home);
            },
            View::History => {
                match self.structured_history() {
                    Ok(choices) => {
                        let total = choices.len();
                        for binding in choices.into_iter().skip(start).take(size) {
                            item(
                                &mut rows,
                                format!(
                                    "Inspect withdrawn assertion {}",
                                    clipped(&binding.statement_id, 100)
                                ),
                                Command::Inspect(binding),
                            );
                        }
                        page_nav(&mut rows, state.page, size, total);
                    },
                    Err(error) => item(
                        &mut rows,
                        format!("History unavailable: {}", clipped(&error, 160)),
                        Command::Home,
                    ),
                }
                item(&mut rows, "Back", Command::Home);
            },
            View::Input(field) => {
                let prompt = if field.contains("start") || field.contains("end") {
                    "Enter YYYY-MM-DDTHH:MM:SS with Z or ±HH:MM; leave both bounds empty for unknown"
                } else if field == "value" {
                    "Enter the typed value; timestamps require an explicit offset"
                } else {
                    "Enter a value; empty clears"
                };
                item(
                    &mut rows,
                    prompt,
                    Command::Submit(self.omnibar.text.clone()),
                );
            },
        }
        let start = self
            .research_search
            .first_row
            .min(rows.len().saturating_sub(1));
        self.omnibar.suggestions = rows
            .iter()
            .skip(start)
            .take(row_limit.max(1))
            .cloned()
            .collect();
        self.omnibar.selected = 0;
        self.research_search.rows = rows;
    }
}
