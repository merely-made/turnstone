// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::App;

impl App {
    /// Read a current admitted search summary without copying source bodies.
    pub(crate) fn scenario_research_search_summary(&self) -> Option<serde_json::Value> {
        let state = &self.research_search;
        if !self.omnibar.open
            || !matches!(self.omnibar.mode, crate::ui::OmnibarMode::SavedSources)
            || state.view != super::research_search_state::View::Results
            || state.error.is_some()
            || state.request.is_some()
        {
            return None;
        }
        let request = state.accepted.as_ref()?;
        let result = state.result.as_ref()?;
        if self.research.context_revision().ok()
            != Some((request.context.clone(), request.revision))
            || self.research.clip_access().ok().and_then(|a| (a.epoch)()) != Some(request.epoch)
            || state.query_revision != request.query_revision
            || state.selected != request.selected
            || result.query != request.query
            || result.settings != request.settings
        {
            return None;
        }
        Some(serde_json::json!({
            "schema": "turnstone.research-search-summary/v1",
            "session": self.session_id,
            "request": request.id,
            "generation": result.generation,
            "selected": state.selected,
            "scope": result.scope,
            "coverage": result.coverage,
            "total": result.total,
            "inputs": result.inputs,
            "hits": result.hits.iter().map(|hit| serde_json::json!({
                "source": hit.source, "version": hit.version, "producer": hit.producer,
                "selector": hit.passage.as_ref().map(|passage| serde_json::json!({
                    "start": passage.selector.start, "end": passage.selector.end
                }))
            })).collect::<Vec<_>>(),
        }))
    }
}
