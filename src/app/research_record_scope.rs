// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Recheck previously enumerated scope without rebuilding its projections.
use super::App;
use crate::research_records::{Binding, ScopeToken};

impl App {
    /// A status observation never serializes the held result or source bodies.
    pub(crate) fn structured_scenario_status(&self) -> serde_json::Value {
        let state = &self.research_search.structured;
        let controller = &state.controller;
        let visible = self.omnibar.open
            && self.research_search.view == super::research_search_state::View::Structured;
        let current = controller
            .view_scope
            .as_ref()
            .is_some_and(|scope| self.structured_scope_is_current(scope));
        let rows: Vec<_> = if visible {
            self.omnibar
                .suggestions
                .iter()
                .map(crate::observe::suggestion_line)
                .collect()
        } else {
            Vec::new()
        };
        let limit_refusal = controller.limit_refusal.as_ref().and_then(|(request, error)| {
            if !visible || controller.busy || controller.error.as_deref() != Some(error.as_str())
                || request.selected != request.scope.selected
                || request.settings != request.scope.settings
                || !self.structured_scope_is_current(&request.scope) {
                return None;
            }
            let scope = &request.scope;
            Some(serde_json::json!({
                "stage": "worker-refusal", "request_id": request.id,
                "context_generation": scope.context.generation, "scope_revision": scope.revision,
                "graph_revisions": scope.graphs.iter().map(|(id, revision)|
                    serde_json::json!({"graph": id.as_uuid(), "revision": revision})).collect::<Vec<_>>(),
                "source_selection": if let Some(members) = &request.selected {
                    serde_json::json!({"mode": "chosen", "members": members})
                } else { serde_json::json!({"mode": "all"}) },
                "query": request.query, "settings": request.settings, "error": error,
            }))
        });
        serde_json::json!({
            "schema": "turnstone.structured-status/v1", "session": self.session_id,
            "view": format!("{:?}", state.view), "visible": visible,
            "busy": controller.busy,
            "author_save_ready": self.structured_scenario_author_save_ready(),
            "structured_idle_ready": self.structured_scenario_idle_ready(),
            "error": state.error.as_deref().or(controller.error.as_deref()),
            "held_result": controller.result.is_some(),
            "result_scope_current": current,
            "result_generation": controller.result_generation,
            "visible_rows": rows,
            "limit_refusal": limit_refusal,
        })
    }

    /// Scenario-only read of the same admitted frozen result shown by the UI.
    /// Never runs a query or reads a store. Refuse changed scope and stale data.
    pub(crate) fn structured_scenario_observation(&self) -> Result<serde_json::Value, String> {
        use eidetic::TypedPayload;
        if self.research_search.view != super::research_search_state::View::Structured
            || !self.omnibar.open
            || self.research_search.structured.view != super::research_structured_state::View::Home
        {
            return Err("structured research is not visible".into());
        }
        let controller = &self.research_search.structured.controller;
        if controller.busy || controller.error.is_some() {
            return Err("structured result is busy or unavailable".into());
        }
        let result = controller
            .result
            .as_ref()
            .ok_or("structured result absent")?;
        let generation = controller
            .result_generation
            .ok_or("result generation absent")?;
        let scope = controller
            .view_scope
            .as_ref()
            .ok_or("result scope absent")?;
        if !self.structured_scope_is_current(scope) {
            return Err("structured result scope changed".into());
        }
        if !controller.result_historical {
            let accepted = controller
                .accepted
                .as_ref()
                .ok_or("structured accepted request absent")?;
            if accepted.selected != scope.selected
                || accepted.query != result.query
                || accepted.settings != result.settings
            {
                return Err("structured accepted scope changed".into());
            }
        }
        let bytes = result.serialize_to_bytes().map_err(|e| e.to_string())?;
        let content_manifest = eidetic::ManifestId::of_blob(&bytes);
        // A replay does not carry a fresh accepted request; don't report a
        // possibly stale saved_result as its selected manifest.
        let saved_manifest = if controller.accepted.is_some() && !controller.result_historical {
            controller.saved_result
        } else {
            None
        };
        if saved_manifest.is_some_and(|id| id != content_manifest) {
            return Err("saved result identity mismatch".into());
        }
        Ok(serde_json::json!({
            "schema": "turnstone.structured-observation/v1",
            "session": self.session_id,
            "context_generation": scope.context.generation,
            "graph_revisions": scope.graphs.iter().map(|(id, revision)|
                serde_json::json!({"graph": id.as_uuid(), "revision": revision})).collect::<Vec<_>>(),
            "scope_revision": scope.revision,
            "result_generation": generation,
            "historical": controller.result_historical,
            "busy": false,
            "request_id": controller.accepted.as_ref().map(|r| r.id),
            "content_manifest": content_manifest,
            "saved_manifest": saved_manifest,
            "source_selection": if controller.result_historical {
                serde_json::json!({"mode": "unknown-historical"})
            } else if let Some(members) = &scope.selected {
                serde_json::json!({"mode": "chosen", "members": members})
            } else {
                serde_json::json!({"mode": "all"})
            },
            "result": crate::research_records::observation_result(result),
        }))
    }

    /// Completion barrier after an ordinary structured command; not a receipt.
    pub(crate) fn structured_scenario_idle_ready(&self) -> bool {
        let state = &self.research_search.structured;
        self.omnibar.open
            && self.research_search.view == super::research_search_state::View::Structured
            && state.error.is_none()
            && state.controller.error.is_none()
            && !state.controller.busy
            && state.controller.pending.is_none()
    }

    /// The authoring submit clears view_scope; only the confirmed save restores it.
    /// The preceding Save action must itself have succeeded in the scenario.
    pub(crate) fn structured_scenario_author_save_ready(&self) -> bool {
        let state = &self.research_search.structured;
        let controller = &state.controller;
        self.omnibar.open
            && self.research_search.view == super::research_search_state::View::Structured
            && state.view == super::research_structured_state::View::Author
            && state.error.is_none()
            && controller.error.is_none()
            && !controller.busy
            && controller.pending.is_none()
            && controller.view_scope.as_ref().is_some_and(|scope| self.structured_scope_is_current(scope))
    }

    /// Wait only for an admitted, settled result with durable current identity,
    /// or an explicitly historical replay. Never advances product state.
    pub(crate) fn structured_scenario_result_ready(&self) -> bool {
        let Ok(value) = self.structured_scenario_observation() else {
            return false;
        };
        value["historical"] == true
            || (value["saved_manifest"].is_string()
                && value["saved_manifest"] == value["content_manifest"])
    }

    pub(super) fn structured_scope(&self) -> Result<ScopeToken, String> {
        self.structured_scope_reserved(0, 0)
    }
    pub(super) fn structured_scope_reserved(
        &self,
        work: usize,
        bytes: usize,
    ) -> Result<ScopeToken, String> {
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
        budget.charge(work, 0, bytes)?;
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

    /// Identity-only recovery token: graph enumeration may itself exceed query limits.
    pub(super) fn structured_recovery_scope(&self) -> Result<ScopeToken, String> {
        let (context, revision) = self.research.context_revision()?;
        if context.session != self.session_id {
            return Err("Structured session changed".into());
        }
        let custody_epoch = (self.research.clip_access()?.epoch)()
            .ok_or("Unlock the profile to use structured research")?;
        let mut graphs = Vec::new();
        let mut graph_instances = Vec::new();
        for runtime in self
            .graph_runtimes
            .iter()
            .filter(|runtime| runtime.session == Some(self.session_id))
        {
            runtime
                .persistence
                .placement_for(&self.session_dir())
                .map_err(|e| format!("Graph placement unavailable: {e}"))?;
            graphs.push((runtime.graph, runtime.canvas.graph().revision()));
            graph_instances.push((runtime.graph, runtime.instance));
        }
        graphs.sort_by_key(|(graph, _)| *graph.as_uuid());
        graph_instances.sort_by_key(|(graph, _)| *graph.as_uuid());
        Ok(ScopeToken {
            context,
            revision,
            custody_epoch,
            graphs,
            graph_instances,
            selected: self.research_search.selected.clone(),
            graph_manifests: vec![],
            owner_bindings: vec![],
            settings: self.research_search.preferences.records.clone(),
        })
    }
    pub(super) fn clear_structured_index(&mut self) {
        let state = &mut self.research_search.structured;
        state.index = None;
        state.indexed_generation = None;
        state.index_error = None;
    }
    pub(super) fn clear_structured_projection(&mut self) {
        self.research_search.structured.controller = Default::default();
        self.clear_structured_index();
    }
    pub(super) fn structured_scope_is_current(&self, expected: &ScopeToken) -> bool {
        if expected.context.session != self.session_id
            || self.research.context_revision().ok()
                != Some((expected.context.clone(), expected.revision))
            || self
                .research
                .clip_access()
                .ok()
                .and_then(|access| (access.epoch)())
                != Some(expected.custody_epoch)
            || self.research_search.selected != expected.selected
            || self.research_search.preferences.records != expected.settings
        {
            return false;
        }
        let mut graphs = Vec::new();
        let mut instances = Vec::new();
        for runtime in self
            .graph_runtimes
            .iter()
            .filter(|runtime| runtime.session == Some(self.session_id))
        {
            if runtime
                .persistence
                .placement_for(&self.session_dir())
                .is_err()
            {
                return false;
            }
            graphs.push((runtime.graph, runtime.canvas.graph().revision()));
            instances.push((runtime.graph, runtime.instance));
        }
        graphs.sort_by_key(|(graph, _)| *graph.as_uuid());
        instances.sort_by_key(|(graph, _)| *graph.as_uuid());
        graphs == expected.graphs && instances == expected.graph_instances
    }
}
