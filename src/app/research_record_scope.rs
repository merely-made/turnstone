// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Recheck previously enumerated scope without rebuilding its projections.
use super::App;
use crate::research_records::ScopeToken;

impl App {
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
