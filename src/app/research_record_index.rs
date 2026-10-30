// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Checked once-per-result identity index for bounded presentation.
use super::research_record_owner::Controller;
use crate::research_records::{Binding, ScopeToken};
use eidetic::records::{LifecycleState, RecordResult};
use std::collections::HashMap;

impl Controller {
    pub(super) fn install_result(
        &mut self,
        result: RecordResult,
        scope: ScopeToken,
    ) -> Result<(), String> {
        self.result = None;
        self.result_generation = None;
        self.record_index.clear();
        let mut observations = HashMap::new();
        let mut owners = HashMap::new();
        for owner in &scope.owner_bindings {
            let key = (
                *owner.binding.graph.as_uuid(),
                owner.binding.statement_id.as_str(),
            );
            if owners.insert(key, owner).is_some() {
                return Err("Ambiguous frozen graph owner".into());
            }
        }
        for observation in &result.snapshot.lifecycle {
            if observation.state != LifecycleState::Active
                || observations
                    .insert(observation.record_id.as_str(), observation)
                    .is_some()
            {
                return Err("Ambiguous frozen record lifecycle".into());
            }
        }
        let mut index = HashMap::new();
        for (position, record) in result.snapshot.records.iter().enumerate() {
            let observation = observations
                .remove(record.id.as_str())
                .ok_or("Frozen record lifecycle unavailable")?;
            let (session, graph, statement_id, manifest) =
                crate::research_records_worker::parse_lifecycle(&observation.lifecycle_ref)?;
            if session != *scope.context.session.as_uuid() {
                return Err("Frozen record belongs to another session".into());
            }
            let entity = record
                .entity
                .parse()
                .map_err(|_| "Invalid frozen record entity")?;
            let binding = Binding {
                graph: crate::panes::GraphId(graph),
                entity,
                statement_id,
                manifest,
            };
            if !owners
                .get(&(graph, binding.statement_id.as_str()))
                .is_some_and(|owner| owner.binding == binding && owner.author == record.author)
            {
                return Err("Frozen record owner is unavailable".into());
            }
            if index
                .insert(record.id.clone(), (position, binding))
                .is_some()
            {
                return Err("Ambiguous frozen record identity".into());
            }
        }
        if !observations.is_empty() {
            return Err("Frozen lifecycle has no record".into());
        }
        self.result = Some(result);
        self.record_index = index;
        self.result_generation = Some(uuid::Uuid::new_v4());
        self.view_scope = Some(scope);
        Ok(())
    }
}
