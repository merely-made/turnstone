// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::App;
use crate::action::Effect;
use crate::research_compare::{Request, ResultView};

impl App {
    pub(crate) fn issue_research_compare(&mut self) -> Vec<Effect> {
        let state = &mut self.research_search;
        state.compare_page = 0;
        state.comparison = None;
        state.compare_accepted = None;
        state.compare_request = None;
        let (Some(left), Some(right)) = (state.compare_left.clone(), state.compare_right.clone())
        else {
            state.error = Some("Choose both sides.".into());
            return vec![Effect::CancelResearchSearch, Effect::Redraw];
        };
        let (context, revision) = match self.research.context_revision() {
            Ok(value) => value,
            Err(error) => {
                state.error = Some(error);
                return vec![Effect::CancelResearchSearch, Effect::Redraw];
            },
        };
        let Some(epoch) = self
            .research
            .clip_access()
            .ok()
            .and_then(|access| (access.epoch)())
        else {
            state.error = Some("Profile custody is unavailable.".into());
            return vec![Effect::CancelResearchSearch, Effect::Redraw];
        };
        if !crate::research::admitted(&context, &left)
            || !crate::research::admitted(&context, &right)
        {
            let side = if !crate::research::admitted(&context, &left) {
                "Left"
            } else {
                "Right"
            };
            state.error = Some(format!("{side} withdrawn from scope."));
            return vec![Effect::CancelResearchSearch, Effect::Redraw];
        }
        let request = Request {
            id: uuid::Uuid::new_v4(),
            context,
            revision,
            epoch,
            left,
            right,
            settings: state.preferences.comparison.clone(),
        };
        state.error = None;
        state.compare_request = Some(request.clone());
        vec![Effect::ResearchCompare(request), Effect::Redraw]
    }
    pub(crate) fn research_compare_finished(
        &mut self,
        request: Request,
        result: Result<ResultView, String>,
    ) -> Vec<Effect> {
        let state = &mut self.research_search;
        if state.compare_request.as_ref() != Some(&request)
            || state.compare_left.as_ref() != Some(&request.left)
            || state.compare_right.as_ref() != Some(&request.right)
            || self.research.context_revision().ok()
                != Some((request.context.clone(), request.revision))
            || self.research.clip_access().ok().and_then(|a| (a.epoch)()) != Some(request.epoch)
        {
            return vec![];
        }
        state.compare_request = None;
        match result {
            Ok(result) => {
                state.error = None;
                state.compare_accepted = Some(request);
                state.comparison = Some(result);
            },
            Err(error) => {
                state.error = Some(error);
                state.compare_accepted = None;
                state.comparison = None;
            },
        }
        if self.omnibar.open {
            self.recompute_omnibar_suggestions();
        }
        vec![Effect::Redraw]
    }
    pub(crate) fn open_comparison(
        &mut self,
        left: bool,
        span: Option<usize>,
        id: uuid::Uuid,
    ) -> Vec<Effect> {
        let state = &self.research_search;
        let valid = state.compare_accepted.as_ref().filter(|request| {
            request.id == id
                && state.compare_left.as_ref() == Some(&request.left)
                && state.compare_right.as_ref() == Some(&request.right)
                && self.research.context_revision().ok()
                    == Some((request.context.clone(), request.revision))
                && self.research.clip_access().ok().and_then(|a| (a.epoch)()) == Some(request.epoch)
                && crate::research::admitted(&request.context, &request.left)
                && crate::research::admitted(&request.context, &request.right)
        });
        let Some((request, result)) = valid.zip(state.comparison.as_ref()) else {
            return self.saved_source_refusal("The comparison is stale or unavailable.".into());
        };
        let mut target = if left {
            request.left.clone()
        } else {
            request.right.clone()
        };
        if let Some(index) = span {
            let Some(span) = result.comparison.spans.get(index) else {
                return self.saved_source_refusal("The comparison passage is unavailable.".into());
            };
            let selector = if left { &span.old } else { &span.new };
            let Some(selector) = selector else {
                return self.saved_source_refusal("The comparison side is unavailable.".into());
            };
            if selector.start >= selector.end {
                return self.saved_source_refusal("The comparison passage is empty.".into());
            }
            target.set_selector(crate::saved_sources::Selector {
                start: selector.start,
                end: selector.end,
            });
        }
        self.omnibar.open = false;
        self.shell.close_omnibar();
        match target {
            crate::research::Target::Capture(target) => self.open_saved_source(target),
            crate::research::Target::Clip(target) => self.open_research_clip(target),
        }
    }
}
