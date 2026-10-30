// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::{
    App,
    research_structured_state::{Command, View, optional},
};
use crate::action::Effect;
use eidetic::records::TagIdentity;

impl App {
    pub(crate) fn research_structured_command(&mut self, command: Command) -> Vec<Effect> {
        let requested_generation = match &command {
            Command::Group { generation, .. }
            | Command::GroupPart { generation, .. }
            | Command::RowSource { generation, .. }
            | Command::OpenResultSource { generation, .. }
            | Command::OpenFrozenEvidence { generation, .. } => Some(*generation),
            Command::Frozen { generation, .. } | Command::FrozenPart { generation, .. } => {
                *generation
            },
            _ => None,
        };
        if matches!(&command,
            Command::Frozen { generation: None, target, .. } | Command::FrozenPart { generation: None, target, .. }
            if *target != super::research_structured_state::FrozenTarget::Inspected)
        {
            return self.saved_source_refusal("The frozen result identity is missing.".into());
        }
        if requested_generation.is_some()
            && requested_generation != self.research_search.structured.controller.result_generation
        {
            return self.saved_source_refusal(
                "The structured result has changed; inspect the current result.".into(),
            );
        }
        let state = &mut self.research_search.structured;
        self.research_search.first_row = 0;
        match command {
            Command::Home => state.view = View::Home,
            Command::Inputs => {
                state.page = 0;
                state.view = View::Inputs;
            },
            Command::InputList(kind) => {
                state.page = 0;
                state.view = View::InputList(kind);
            },
            Command::RowSource {
                generation,
                row,
                source,
            } => {
                state.page = 0;
                state.view = View::RowSource {
                    generation,
                    row,
                    source,
                };
            },
            Command::Frozen { generation, target } => {
                state.page = 0;
                state.view = View::Frozen { generation, target };
            },
            Command::FrozenPart {
                generation,
                target,
                field,
                start,
            } => {
                state.view = View::FrozenPart {
                    generation,
                    target,
                    field,
                    start,
                };
            },
            Command::OpenResultSource { generation, source } => {
                let Some((source, version)) = state
                    .controller
                    .result
                    .as_ref()
                    .and_then(|r| r.snapshot.sources.get(source))
                    .map(|s| (s.source.clone(), s.version.clone()))
                else {
                    return self.saved_source_refusal("Frozen source unavailable".into());
                };
                return self.structured_open_result_source(generation, source, version);
            },
            Command::OpenFrozenEvidence { generation, record } => {
                return self.structured_open_frozen_record_evidence(generation, record);
            },
            Command::Group { generation, index } => {
                state.page = 0;
                state.view = View::Group { generation, index };
            },
            Command::GroupPart {
                generation,
                index,
                field,
                start,
            } => {
                state.view = View::GroupPart {
                    generation,
                    index,
                    field,
                    start,
                };
            },
            Command::Operations => state.view = View::Operations,
            Command::Scope => {
                self.research_search.view = super::research_search_state::View::Sources
            },
            Command::Filters => state.view = View::Filters,
            Command::FilterSources => {
                state.page = 0;
                state.view = View::FilterSources;
            },
            Command::Author => state.view = View::Author,
            Command::Entities => {
                state.page = 0;
                state.view = View::Entities;
            },
            Command::Evidence => {
                state.page = 0;
                state.view = View::Evidence;
            },
            Command::Settings => state.view = View::Settings,
            Command::Saved => {
                state.page = 0;
                state.view = View::Saved;
                return self.structured_refresh_saved();
            },
            Command::History => {
                state.page = 0;
                state.view = View::History;
                return self.structured_refresh_history();
            },
            Command::Recovery => {
                state.page = 0;
                state.view = View::Recovery;
            },
            Command::RefreshRecovery => return self.structured_refresh_recovery(),
            Command::RetryRecovery(nonce) => return self.structured_retry_recovery(nonce),
            Command::AbandonRecovery(nonce) => return self.structured_abandon_recovery(nonce),
            Command::Operation(op) => {
                state.operation = op;
                state.page = 0;
                state.view = View::Home;
            },
            Command::Type(kind) => {
                state.draft.value_type = Some(kind);
                state.view = View::Author;
            },
            Command::Cardinality(value) => {
                state.draft.cardinality = Some(value);
                state.view = View::Author;
            },
            Command::Entity(graph, entity) => {
                state.draft.entity = Some((graph, entity));
                state.view = View::Author;
            },
            Command::EvidenceTarget(target) => {
                state.draft.evidence = Some(target);
                state.view = View::Author;
            },
            Command::FilterSource(source) => {
                state.filters.source = source;
                state.view = View::Filters;
            },
            Command::Input(field) => {
                state.input_return = state.view.clone();
                self.research_search.original_text = self.omnibar.text.clone();
                self.omnibar.text.clear();
                self.omnibar.cursor = 0;
                state.view = View::Input(field);
            },
            Command::Submit(value) => {
                let View::Input(field) = state.view.clone() else {
                    return vec![];
                };
                let original = std::mem::take(&mut self.research_search.original_text);
                self.omnibar.text = original;
                self.omnibar.cursor = self.omnibar.text.len();
                let result = if field.starts_with("record_max_") {
                    super::research_search_values::apply_value(
                        &mut self.research_search,
                        &self.data_root,
                        &field,
                        &value,
                    )
                } else {
                    apply_input(&mut self.research_search.structured, &field, &value)
                };
                let state = &mut self.research_search.structured;
                state.error = result.err();
                state.view = state.input_return.clone();
            },
            Command::Run => {
                state.page = 0;
                match state.query() {
                    Ok(query) => {
                        let settings = self.research_search.preferences.records.clone();
                        return self.structured_query(query, settings);
                    },
                    Err(error) => state.error = Some(error),
                }
            },
            Command::SaveProperty => match state.draft.property() {
                Ok(property) => return self.structured_author(property),
                Err(error) => state.error = Some(error),
            },
            Command::Inspect(binding) => {
                state.view = View::Home;
                return self.structured_inspect(binding);
            },
            Command::OpenEvidence(binding) => return self.structured_open_evidence(binding),
            Command::Withdraw(binding) => return self.structured_withdraw(binding),
            Command::Replay(id) => {
                state.view = View::Home;
                state.page = 0;
                return self.structured_replay(id);
            },
            Command::Page(delta) => state.page = state.page.saturating_add_signed(delta),
            Command::Back => {
                self.research_search.view = super::research_search_state::View::Results
            },
        }
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }
}
fn apply_input(
    state: &mut super::research_structured_state::State,
    field: &str,
    value: &str,
) -> Result<(), String> {
    let draft = &mut state.draft;
    let value = if field == "value" {
        value.to_owned()
    } else {
        value.trim().to_owned()
    };
    match field {
        "definition" => draft.definition = value,
        "revision" => draft.revision = value,
        "unit" => draft.unit = value,
        "statuses" => draft.statuses = value,
        "value" => draft.value = value,
        "context" => draft.context = value,
        "status" => draft.status = value,
        "valid_start" => draft.valid_start = value,
        "valid_end" => draft.valid_end = value,
        "filter_entity" => {
            if !value.is_empty() {
                uuid::Uuid::parse_str(&value).map_err(|_| "Enter an existing entity UUID")?;
            }
            state.filters.entity = optional(&value);
        },
        "filter_property" => state.filters.property = optional(&value),
        "filter_revision" => state.filters.schema_revision = optional(&value),
        "filter_status" => state.filters.status = optional(&value),
        "filter_unit" => state.filters.unit = optional(&value),
        "filter_start" => state.filter_start = value,
        "filter_end" => state.filter_end = value,
        "filter_tag" => {
            state.filters.tag = if value.is_empty() {
                None
            } else {
                let (owner, concept) = value.split_once('|').ok_or("Enter tag owner|concept")?;
                if owner.trim().is_empty() || concept.trim().is_empty() {
                    return Err("Enter tag owner|concept".into());
                }
                Some(TagIdentity {
                    owner: owner.trim().into(),
                    concept: concept.trim().into(),
                })
            }
        },
        _ => return Err("Unknown field".into()),
    }
    Ok(())
}
