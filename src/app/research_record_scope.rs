// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Recheck previously enumerated scope without rebuilding its projections.
use super::App;
use crate::research_records::{Binding, ScopeToken};

impl App {
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
