// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Reopen exact structured-query evidence after current admission checks.
use super::App;
use crate::action::Effect;
use crate::research_records::{Binding, Job};
use eidetic::models::recall::RecallSourceId;

impl App {
    pub(crate) fn structured_open_evidence(&mut self, binding: Binding) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let controller = &self.research_search.structured.controller;
        if controller.view_scope.as_ref() != Some(&scope) {
            return self.structured_error("Property inspection is stale");
        }
        let Some((held, record)) = controller.inspected.as_ref() else {
            return self.structured_error("Inspect the property first");
        };
        if held != &binding {
            return self.structured_error("Property inspection changed");
        }
        let record = record.clone();
        self.structured_submit(Job::OpenEvidence {
            id: uuid::Uuid::new_v4(),
            scope,
            generation: None,
            binding,
            record,
        })
    }
    pub(crate) fn structured_open_frozen_record_evidence(
        &mut self,
        generation: uuid::Uuid,
        index: usize,
    ) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let controller = &self.research_search.structured.controller;
        if controller.view_scope.as_ref() != Some(&scope)
            || controller.result_generation != Some(generation)
        {
            return self.structured_error("Frozen result is stale");
        }
        let Some(record) = controller
            .result
            .as_ref()
            .and_then(|result| result.snapshot.records.get(index))
            .cloned()
        else {
            return self.structured_error("Frozen record unavailable");
        };
        let binding = match controller.binding_for_record(&record.id) {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        self.structured_submit(Job::OpenEvidence {
            id: uuid::Uuid::new_v4(),
            scope,
            generation: Some(generation),
            binding,
            record,
        })
    }
    pub(crate) fn structured_open_result_source(
        &mut self,
        generation: uuid::Uuid,
        source: RecallSourceId,
        version: String,
    ) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let controller = &self.research_search.structured.controller;
        if controller.view_scope.as_ref() != Some(&scope)
            || controller.result_generation != Some(generation)
        {
            return self.structured_error("Frozen result is stale");
        }
        if !controller.result.as_ref().is_some_and(|result| {
            result
                .snapshot
                .sources
                .iter()
                .any(|held| held.source == source && held.version == version)
        }) {
            return self.structured_error("Frozen source unavailable");
        }
        self.structured_submit(Job::OpenSource {
            id: uuid::Uuid::new_v4(),
            scope,
            generation,
            source,
            version,
        })
    }
    pub(super) fn structured_open_verified(
        &mut self,
        target: crate::research::Target,
    ) -> Vec<Effect> {
        self.omnibar.open = false;
        self.shell.close_omnibar();
        match target {
            crate::research::Target::Capture(target) => self.open_saved_source(target),
            crate::research::Target::Clip(target) => self.open_research_clip(target),
        }
    }
}
