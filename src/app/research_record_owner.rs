// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Host admission and current-graph checks for structured research.
use super::App;
use crate::action::Effect;
use crate::research_records::{
    AuthorRequest, Binding, Candidate, EntityChoice, Job, JobResult, ManualProperty, PendingChoice,
    QueryRequest, SavedResultChoice, ScopeToken,
};
use eidetic::records::{PropertyRecordV1, RecordQuery, RecordResult, RecordSettings};
#[cfg(test)]
#[path = "research_record_integration_tests.rs"]
mod integration_tests;
#[cfg(test)]
#[path = "research_record_owner_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct Controller {
    pub result: Option<RecordResult>,
    pub result_generation: Option<uuid::Uuid>,
    pub result_historical: bool,
    pub(super) record_index: std::collections::HashMap<String, (usize, Binding)>,
    pub accepted: Option<QueryRequest>,
    pub inspected: Option<(Binding, PropertyRecordV1)>,
    pub inspected_withdrawn: Option<bool>,
    pub saved_result: Option<eidetic::ManifestId>,
    pub error: Option<String>,
    pub busy: bool,
    pub(super) pending: Option<Job>,
    pub(super) view_scope: Option<ScopeToken>,
    pub recovery: Vec<PendingChoice>,
    pub(super) recovery_items: Vec<crate::trail_memory::record_actor::RecoveryItem>,
    pub(super) recovery_scope: Option<ScopeToken>,
    pub(super) saved_choices: Vec<SavedResultChoice>,
    pub(super) discovery_scope: Option<ScopeToken>,
    pub(super) history_choices: Vec<Binding>,
    pub(super) history_scope: Option<ScopeToken>,
}
impl Controller {
    pub fn binding_for_record(&self, record_id: &str) -> Result<Binding, String> {
        self.record_index
            .get(record_id)
            .map(|(_, binding)| binding.clone())
            .ok_or_else(|| "Property record unavailable".into())
    }
    pub fn frozen_record(&self, record_id: &str) -> Option<&PropertyRecordV1> {
        let (index, _) = self.record_index.get(record_id)?;
        self.result.as_ref()?.snapshot.records.get(*index)
    }
}
impl App {
    pub(super) fn structured_scope(&self) -> Result<ScopeToken, String> {
        let (context, revision) = self.research.context_revision()?;
        if context.session != self.session_id {
            return Err("Structured session changed".into());
        }
        let custody_epoch = (self.research.clip_access()?.epoch)()
            .ok_or("Unlock the profile to use structured research")?;
        let mut graphs = Vec::new();
        let mut graph_instances = Vec::new();
        let mut graph_manifests = Vec::new();
        let mut owner_bindings = Vec::new();
        let mut budget =
            crate::research_record_graph::Budget::new(&self.research_search.preferences.records);
        for runtime in self
            .graph_runtimes
            .iter()
            .filter(|runtime| runtime.session == Some(self.session_id))
        {
            runtime
                .persistence
                .placement_for(&self.session_dir())
                .map_err(|e| format!("Graph placement unavailable: {e}"))?;
            for (_, node) in runtime.canvas.graph().nodes() {
                budget.charge(1, 0, node.url().len())?;
                crate::retained_clip::request(node.url())
                    .map_err(|e| format!("Retained source is malformed: {e}"))?;
            }
            graphs.push((runtime.graph, runtime.canvas.graph().revision()));
            graph_instances.push((runtime.graph, runtime.instance));
            for binding in crate::research_record_graph::bindings_limited(
                runtime.canvas.graph(),
                true,
                &mut budget,
            )? {
                graph_manifests.push((runtime.graph, binding.manifest));
                owner_bindings.push(crate::research_records::OwnerBinding {
                    binding: Binding {
                        graph: runtime.graph,
                        entity: binding.entity,
                        statement_id: binding.statement_id,
                        manifest: binding.manifest,
                    },
                    author: binding.author,
                    withdrawn: binding.withdrawn,
                });
            }
            for binding in crate::research_record_graph::result_bindings_limited(
                runtime.canvas.graph(),
                &mut budget,
            )? {
                graph_manifests.push((runtime.graph, binding.manifest));
            }
        }
        graphs.sort_by_key(|(graph, _)| *graph.as_uuid());
        graph_instances.sort_by_key(|(graph, _)| *graph.as_uuid());
        graph_manifests.sort_by_key(|(graph, manifest)| (*graph.as_uuid(), manifest.to_string()));
        owner_bindings.sort_by_key(|value| {
            (
                *value.binding.graph.as_uuid(),
                value.binding.statement_id.clone(),
            )
        });
        Ok(ScopeToken {
            context,
            revision,
            custody_epoch,
            graphs,
            graph_instances,
            selected: self.research_search.selected.clone(),
            graph_manifests,
            owner_bindings,
            settings: self.research_search.preferences.records.clone(),
        })
    }
    pub(super) fn structured_candidates(&self, historical: bool) -> Result<Vec<Candidate>, String> {
        let mut values = Vec::new();
        let mut budget =
            crate::research_record_graph::Budget::new(&self.research_search.preferences.records);
        for runtime in self
            .graph_runtimes
            .iter()
            .filter(|runtime| runtime.session == Some(self.session_id))
        {
            runtime
                .persistence
                .placement_for(&self.session_dir())
                .map_err(|e| e.to_string())?;
            for item in crate::research_record_graph::bindings_limited(
                runtime.canvas.graph(),
                historical,
                &mut budget,
            )? {
                if historical != item.withdrawn {
                    continue;
                }
                values.push(Candidate {
                    binding: Binding {
                        graph: runtime.graph,
                        entity: item.entity,
                        statement_id: item.statement_id,
                        manifest: item.manifest,
                    },
                    author: item.author,
                });
            }
        }
        Ok(values)
    }
    pub(super) fn structured_has_binding(
        &self,
        binding: &Binding,
        historical: bool,
    ) -> Result<bool, String> {
        Ok(self
            .structured_candidates(historical)?
            .iter()
            .any(|item| item.binding == *binding))
    }
    pub(super) fn structured_submit(&mut self, job: Job) -> Vec<Effect> {
        if matches!(
            &job,
            Job::Deposit(_)
                | Job::PrepareWithdrawal { .. }
                | Job::Recovery { .. }
                | Job::Reconcile { .. }
                | Job::Abandon { .. }
        ) {
            self.clear_structured_index();
        }
        let controller = &mut self.research_search.structured.controller;
        if matches!(
            &job,
            Job::Deposit(_)
                | Job::PrepareWithdrawal { .. }
                | Job::Recovery { .. }
                | Job::Reconcile { .. }
                | Job::Abandon { .. }
        ) {
            controller.result = None;
            controller.result_generation = None;
            controller.result_historical = false;
            controller.record_index.clear();
            controller.accepted = None;
            controller.inspected = None;
            controller.inspected_withdrawn = None;
            controller.view_scope = None;
        }
        controller.error = None;
        controller.busy = true;
        controller.pending = Some(job.clone());
        vec![Effect::ResearchRecords(job), Effect::Redraw]
    }
    pub(super) fn structured_error(&mut self, error: impl Into<String>) -> Vec<Effect> {
        let controller = &mut self.research_search.structured.controller;
        controller.busy = false;
        controller.pending = None;
        controller.error = Some(error.into());
        vec![Effect::Redraw]
    }
    pub(crate) fn structured_query(
        &mut self,
        query: RecordQuery,
        settings: RecordSettings,
    ) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(v) => v,
            Err(e) => return self.structured_error(e),
        };
        let candidates = match self.structured_candidates(false) {
            Ok(v) => v,
            Err(e) => return self.structured_error(e),
        };
        let result_anchor = self
            .structured_entities()
            .ok()
            .and_then(|entities| entities.first().map(|entity| (entity.graph, entity.entity)));
        let request = QueryRequest {
            id: uuid::Uuid::new_v4(),
            scope,
            selected: self.research_search.selected.clone(),
            query,
            settings,
            candidates,
            result_anchor,
        };
        self.structured_submit(Job::Query(request))
    }
    pub(crate) fn structured_author(&mut self, property: ManualProperty) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(v) => v,
            Err(e) => return self.structured_error(e),
        };
        let Some(runtime) = self.graph_runtimes.get(property.graph) else {
            return self.structured_error("Property graph unavailable");
        };
        if runtime.session != Some(self.session_id)
            || runtime.canvas.graph().resource(property.entity).is_none()
            || !runtime.persistence.owns_directory(&self.session_dir())
        {
            return self.structured_error("Property graph or entity is unavailable");
        }
        let author = match self.structured_author_identity() {
            Ok(author) => author.asserter_iri(),
            Err(error) => return self.structured_error(error),
        };
        let recorded_at = match std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_secs()).ok())
        {
            Some(v) => v,
            None => return self.structured_error("Record time unavailable"),
        };
        let request = AuthorRequest {
            id: uuid::Uuid::new_v4(),
            scope,
            selected: self.research_search.selected.clone(),
            property,
            author,
            recorded_at,
        };
        self.structured_submit(Job::Deposit(request))
    }
    pub(crate) fn structured_inspect(&mut self, binding: Binding) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(v) => v,
            Err(e) => return self.structured_error(e),
        };
        if !self
            .structured_has_binding(&binding, false)
            .unwrap_or(false)
            && !self.structured_has_binding(&binding, true).unwrap_or(false)
        {
            return self.structured_error("Property assertion unavailable");
        }
        self.structured_submit(Job::Inspect {
            id: uuid::Uuid::new_v4(),
            scope,
            binding,
        })
    }
    pub(crate) fn structured_replay(&mut self, result: eidetic::ManifestId) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(v) => v,
            Err(e) => return self.structured_error(e),
        };
        match self.structured_saved_results() {
            Ok(values)
                if values
                    .iter()
                    .any(|choice| choice.binding.manifest == result) => {},
            _ => return self.structured_error("Saved result is not admitted"),
        }
        self.structured_submit(Job::Replay {
            id: uuid::Uuid::new_v4(),
            scope,
            result,
        })
    }
    pub(crate) fn structured_entities(&self) -> Result<Vec<EntityChoice>, String> {
        self.structured_scope()?;
        let mut choices = Vec::new();
        let mut budget =
            crate::research_record_graph::Budget::new(&self.research_search.preferences.records);
        for runtime in self
            .graph_runtimes
            .iter()
            .filter(|runtime| runtime.session == Some(self.session_id))
        {
            for resource in runtime.canvas.graph().resource_nodes() {
                budget.charge(1, 1, resource.canonical_iri().len())?;
                choices.push(EntityChoice {
                    graph: runtime.graph,
                    entity: resource.id(),
                    label: resource.canonical_iri().into(),
                });
            }
        }
        choices.sort_by_key(|choice| (*choice.graph.as_uuid(), choice.entity));
        Ok(choices)
    }
    pub(crate) fn structured_history(&self) -> Result<Vec<Binding>, String> {
        let scope = self.structured_scope()?;
        let controller = &self.research_search.structured.controller;
        if controller.history_scope.as_ref() != Some(&scope) {
            return Err("Refresh withdrawn records".into());
        }
        Ok(controller.history_choices.clone())
    }
    pub(crate) fn structured_refresh_history(&mut self) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let bindings: Vec<_> = match self.structured_candidates(true) {
            Ok(values) => values.into_iter().map(|item| item.binding).collect(),
            Err(error) => return self.structured_error(error),
        };
        if bindings.len() > scope.settings.max_records {
            return self.structured_error("Withdrawn record limit exceeded");
        }
        self.structured_submit(Job::Discover {
            id: uuid::Uuid::new_v4(),
            scope,
            bindings,
            history: true,
        })
    }
    pub(crate) fn structured_saved_results(&self) -> Result<Vec<SavedResultChoice>, String> {
        let scope = self.structured_scope()?;
        let controller = &self.research_search.structured.controller;
        if controller.discovery_scope.as_ref() != Some(&scope) {
            return Err("Refresh saved results".into());
        }
        Ok(controller.saved_choices.clone())
    }
    pub(crate) fn structured_refresh_saved(&mut self) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let values = (|| {
            let mut values = Vec::new();
            let mut budget = crate::research_record_graph::Budget::new(&scope.settings);
            for runtime in self
                .graph_runtimes
                .iter()
                .filter(|runtime| runtime.session == Some(self.session_id))
            {
                for row in crate::research_record_graph::result_bindings_limited(
                    runtime.canvas.graph(),
                    &mut budget,
                )? {
                    values.push(Binding {
                        graph: runtime.graph,
                        entity: row.entity,
                        statement_id: row.statement_id,
                        manifest: row.manifest,
                    });
                }
            }
            Ok::<_, String>(values)
        })();
        let values = match values {
            Ok(values) => values,
            Err(error) => return self.structured_error(error),
        };
        if values.len() > scope.settings.max_records {
            return self.structured_error("Saved result limit exceeded");
        }
        self.structured_submit(Job::Discover {
            id: uuid::Uuid::new_v4(),
            scope,
            bindings: values,
            history: false,
        })
    }
    pub(crate) fn structured_refresh_recovery(&mut self) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        self.structured_submit(Job::Recovery {
            id: uuid::Uuid::new_v4(),
            scope,
        })
    }
    pub(crate) fn structured_retry_recovery(&mut self, nonce: uuid::Uuid) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let controller = &self.research_search.structured.controller;
        if controller.recovery_scope.as_ref() != Some(&scope) {
            return self.structured_error("Refresh pending saves first");
        }
        let Some(item) = controller
            .recovery_items
            .iter()
            .find(|item| item.nonce() == nonce)
            .cloned()
        else {
            return self.structured_error("Pending save unavailable");
        };
        if controller
            .recovery
            .iter()
            .any(|choice| choice.nonce == nonce && choice.orphan)
        {
            return self
                .structured_error("This unbound deposit has no assertion to retry; set it aside");
        }
        self.structured_submit(Job::Reconcile {
            id: uuid::Uuid::new_v4(),
            scope,
            item,
        })
    }
    pub(crate) fn structured_abandon_recovery(&mut self, nonce: uuid::Uuid) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let controller = &self.research_search.structured.controller;
        if controller.recovery_scope.as_ref() != Some(&scope)
            || !controller
                .recovery
                .iter()
                .any(|choice| choice.nonce == nonce && choice.orphan)
        {
            return self.structured_error("Only an unbound pending save can be set aside");
        }
        self.structured_submit(Job::Abandon {
            id: uuid::Uuid::new_v4(),
            scope,
            nonce,
        })
    }
    pub(crate) fn invalidate_structured(&mut self) -> Vec<Effect> {
        let controller = &self.research_search.structured.controller;
        if controller.pending.is_none()
            && controller.view_scope.is_none()
            && controller.recovery_scope.is_none()
            && controller.discovery_scope.is_none()
            && controller.history_scope.is_none()
        {
            return vec![];
        }
        let state = &self.research_search.structured.controller;
        let expected = state.pending.as_ref().map(|job| match job {
            Job::Query(r) => &r.scope,
            Job::Deposit(r) => &r.scope,
            Job::Inspect { scope, .. }
            | Job::Replay { scope, .. }
            | Job::ConfirmProperty { scope, .. }
            | Job::ConfirmResult { scope, .. }
            | Job::MarkProperty { scope, .. }
            | Job::MarkResult { scope, .. }
            | Job::PrepareWithdrawal { scope, .. }
            | Job::ConfirmWithdrawal { scope, .. }
            | Job::Recovery { scope, .. }
            | Job::Reconcile { scope, .. }
            | Job::RecoveryMark { scope, .. }
            | Job::RecoveryAck { scope, .. }
            | Job::Abandon { scope, .. }
            | Job::Discover { scope, .. }
            | Job::OpenSource { scope, .. }
            | Job::OpenEvidence { scope, .. } => scope,
        });
        let stale = expected.is_some_and(|expected| !self.structured_scope_is_current(expected))
            || state
                .view_scope
                .as_ref()
                .is_some_and(|expected| !self.structured_scope_is_current(expected))
            || state
                .recovery_scope
                .as_ref()
                .is_some_and(|expected| !self.structured_scope_is_current(expected))
            || state
                .discovery_scope
                .as_ref()
                .is_some_and(|expected| !self.structured_scope_is_current(expected))
            || state
                .history_scope
                .as_ref()
                .is_some_and(|expected| !self.structured_scope_is_current(expected));
        if stale {
            self.clear_structured_projection();
            return vec![Effect::CancelResearchSearch];
        }
        vec![]
    }
    pub(super) fn structured_after_commit(
        &self,
        before: &ScopeToken,
        graph: crate::panes::GraphId,
    ) -> Result<ScopeToken, String> {
        let after = self.structured_scope()?;
        if after.context != before.context
            || after.revision != before.revision
            || after.custody_epoch != before.custody_epoch
            || after.selected != before.selected
            || after.settings != before.settings
            || after.graph_instances != before.graph_instances
            || after.graphs.len() != before.graphs.len()
            || before
                .graphs
                .iter()
                .any(|(id, rev)| *id != graph && !after.graphs.contains(&(*id, *rev)))
            || before
                .graph_manifests
                .iter()
                .filter(|(id, _)| *id != graph)
                .ne(after.graph_manifests.iter().filter(|(id, _)| *id != graph))
            || before
                .owner_bindings
                .iter()
                .filter(|value| value.binding.graph != graph)
                .ne(after
                    .owner_bindings
                    .iter()
                    .filter(|value| value.binding.graph != graph))
        {
            return Err("Structured research changed during graph save".into());
        }
        Ok(after)
    }
    pub(crate) fn structured_withdraw(&mut self, binding: Binding) -> Vec<Effect> {
        let scope = match self.structured_scope() {
            Ok(value) => value,
            Err(error) => return self.structured_error(error),
        };
        let controller = &self.research_search.structured.controller;
        let Some((held, record)) = controller.inspected.as_ref() else {
            return self.structured_error("Inspect the property before withdrawal");
        };
        if held != &binding || controller.view_scope.as_ref() != Some(&scope) {
            return self.structured_error("Property inspection is stale");
        }
        let author = match self.structured_author_identity() {
            Ok(value) => value.asserter_iri(),
            Err(error) => return self.structured_error(error),
        };
        if record.author != author
            || !self
                .structured_has_binding(&binding, false)
                .unwrap_or(false)
        {
            return self.structured_error("Only your current assertion can be withdrawn");
        }
        let nonce = match uuid::Uuid::parse_str(&record.id) {
            Ok(value) => value,
            Err(_) => return self.structured_error("Property nonce is invalid"),
        };
        self.structured_submit(Job::PrepareWithdrawal {
            id: uuid::Uuid::new_v4(),
            scope,
            binding,
            nonce,
        })
    }
}
