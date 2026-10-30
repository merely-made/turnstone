// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::research_search_values::apply_value;
use super::{
    App,
    research_search_state::{Command, Preferences, State, View},
};
use crate::action::{Action, Effect};
use crate::research_search::Request;
use crate::ui::OmnibarMode;
use eidetic_search::query::{ProducerKind, QueryMode};

impl App {
    pub(crate) fn start_research_search(&mut self) -> Vec<Effect> {
        self.research_search = State::default();
        match Preferences::load(&self.data_root) {
            Ok(preferences) => self.research_search.preferences = preferences,
            Err(error) => {
                self.research_search.settings_error = Some(error.clone());
                self.research_search.error = Some(error);
            },
        }
        self.sync_research_context();
        self.research_search.scope_revision = self
            .research
            .context_revision()
            .ok()
            .map(|(_, revision)| revision);
        self.issue_research_search()
    }
    pub(crate) fn issue_research_search(&mut self) -> Vec<Effect> {
        self.research_search.query_revision = self.research_search.query_revision.saturating_add(1);
        self.research_search.compare_left = None;
        self.research_search.compare_right = None;
        self.research_search.compare_request = None;
        self.research_search.compare_accepted = None;
        self.research_search.comparison = None;
        self.research_search.result = None;
        self.research_search.accepted = None;
        self.research_search.request = None;
        if self.research_search.settings_error.is_some() {
            return vec![Effect::Redraw];
        }
        self.research_search.error = None;
        let (context, revision) = match self.research.context_revision() {
            Ok(value) => value,
            Err(error) => {
                self.research_search.error = Some(error);
                return vec![Effect::Redraw];
            },
        };
        let epoch = match self
            .research
            .clip_access()
            .and_then(|access| (access.epoch)().ok_or_else(|| crate::identity::PENDING.into()))
        {
            Ok(epoch) => epoch,
            Err(error) => {
                self.research_search.error = Some(error);
                return vec![Effect::Redraw];
            },
        };
        let text = self.omnibar.text.clone();
        let mut query = self.research_search.query.clone();
        query.mode = if self.research_search.mode == 4 || text.is_empty() {
            QueryMode::Browse
        } else {
            match self.research_search.mode {
                1 => QueryMode::Phrase(text),
                2 => QueryMode::Address(text),
                3 => QueryMode::Prefix(text),
                _ => QueryMode::Terms(text),
            }
        };
        query.aliases = self.research_search.preferences.aliases.clone();
        let mut settings = self.research_search.preferences.settings.clone();
        settings.offset = self.research_search.page.saturating_mul(settings.limit);
        let request = Request {
            id: uuid::Uuid::new_v4(),
            context,
            revision,
            epoch,
            query_revision: self.research_search.query_revision,
            selected: self.research_search.selected.clone(),
            query,
            settings,
        };
        self.research_search.request = Some(request.clone());
        vec![Effect::ResearchSearch(request), Effect::Redraw]
    }
    /// Forget all private text and projections before custody or session changes.
    pub(crate) fn clear_private_research(&mut self) {
        if matches!(self.omnibar.mode, OmnibarMode::SavedSources) {
            self.omnibar = crate::ui::OmnibarState::default();
            self.shell.close_omnibar();
            if self.focus == crate::surface::FocusTarget::Chrome {
                self.focus = crate::surface::FocusTarget::Graph(self.default_graph_pane());
            }
        }
        self.research_search = State::default();
    }
    pub(crate) fn invalidate_research_search(&mut self) -> Vec<Effect> {
        let current = self.research.context_revision().ok();
        let epoch = self
            .research
            .clip_access()
            .ok()
            .and_then(|access| (access.epoch)());
        let scope_changed = self
            .research_search
            .scope_revision
            .is_some_and(|revision| current.as_ref().map(|(_, now)| *now) != Some(revision));
        let stale = scope_changed
            || self
                .research_search
                .request
                .as_ref()
                .or(self.research_search.accepted.as_ref())
                .is_some_and(|request| {
                    current.as_ref() != Some(&(request.context.clone(), request.revision))
                        || epoch != Some(request.epoch)
                });
        let stale = stale
            || self
                .research_search
                .compare_request
                .as_ref()
                .or(self.research_search.compare_accepted.as_ref())
                .is_some_and(|request| {
                    current.as_ref() != Some(&(request.context.clone(), request.revision))
                        || epoch != Some(request.epoch)
                });
        if stale {
            self.clear_private_research();
            return vec![Effect::CancelResearchSearch];
        }
        vec![]
    }
    pub(crate) fn research_search_finished(
        &mut self,
        request: Request,
        result: Result<eidetic_search::query::SearchResult, String>,
    ) -> Vec<Effect> {
        let current = self.research_search.request.as_ref() == Some(&request)
            && self.research.context_revision().ok()
                == Some((request.context.clone(), request.revision))
            && self
                .research
                .clip_access()
                .ok()
                .and_then(|access| (access.epoch)())
                == Some(request.epoch);
        if !current {
            return vec![];
        }
        self.research_search.request = None;
        self.research_search.accepted = Some(request);
        match result {
            Ok(result) => {
                self.research_search.error = None;
                self.research_search.result = Some(result);
            },
            Err(error) => self.research_search.error = Some(error),
        }
        if self.omnibar.open && matches!(self.omnibar.mode, OmnibarMode::SavedSources) {
            self.recompute_omnibar_suggestions();
        }
        vec![Effect::Redraw]
    }
    pub(crate) fn research_search_command(&mut self, command: Command) -> Vec<Effect> {
        let mut refresh = false;
        let state = &mut self.research_search;
        state.first_row = 0;
        match command {
            Command::Results => state.view = View::Results,
            Command::Compare => state.view = View::Compare,
            Command::ChooseLeft => state.view = View::ChooseLeft,
            Command::ChooseRight => state.view = View::ChooseRight,
            Command::ChooseCompare { left, target } => {
                let valid = state.accepted.as_ref().is_some_and(|r| {
                    self.research.context_revision().ok() == Some((r.context.clone(), r.revision))
                        && self.research.clip_access().ok().and_then(|a| (a.epoch)())
                            == Some(r.epoch)
                }) && state.result.as_ref().is_some_and(|result| {
                    result.inputs.iter().any(|input| {
                        crate::research::target(&input.source, &input.version)
                            .ok()
                            .as_ref()
                            == Some(&target)
                    })
                });
                if !valid {
                    return self.saved_source_refusal(
                        "The source selection is stale or unavailable.".into(),
                    );
                }
                if left {
                    state.compare_left = Some(target);
                } else {
                    state.compare_right = Some(target);
                }
                state.compare_request = None;
                state.compare_accepted = None;
                state.comparison = None;
                state.compare_page = 0;
                state.view = View::Compare;
                state.error = None;
                self.recompute_omnibar_suggestions();
                return vec![Effect::CancelResearchSearch, Effect::Redraw];
            },
            Command::RunCompare => {
                let effects = self.issue_research_compare();
                self.recompute_omnibar_suggestions();
                return effects;
            },
            Command::ComparePage(delta) => {
                state.compare_page = state.compare_page.saturating_add_signed(delta)
            },
            Command::OpenCompare {
                left,
                span,
                request,
            } => return self.open_comparison(left, span, request),
            Command::Options => state.view = View::Options,
            Command::Modes => state.view = View::Modes,
            Command::Types => state.view = View::Types,
            Command::Sources => state.view = View::Sources,
            Command::Dates => state.view = View::Dates,
            Command::Tags => state.view = View::Tags,
            Command::Settings => state.view = View::Settings,
            Command::Coverage => state.view = View::Coverage,
            Command::Mode(mode) if mode < 5 => {
                state.mode = mode;
                state.page = 0;
                state.view = View::Results;
                refresh = true;
            },
            Command::AllKinds => {
                state.query.kinds.clear();
                state.page = 0;
                refresh = true;
            },
            Command::Kind(kind) => {
                if let Some(i) = state.query.kinds.iter().position(|v| v == &kind) {
                    state.query.kinds.remove(i);
                } else {
                    state.query.kinds.push(kind);
                }
                state.page = 0;
                refresh = true;
            },
            Command::ScopeAll => {
                state.selected = None;
                state.page = 0;
                refresh = true;
            },
            Command::ScopeChosen => {
                state.selected = Some(vec![]);
                state.page = 0;
                refresh = true;
            },
            Command::Member(member) => {
                let selected = state.selected.get_or_insert_with(Vec::new);
                if let Some(i) = selected.iter().position(|v| v == &member) {
                    selected.remove(i);
                } else {
                    selected.push(member);
                }
                state.page = 0;
                refresh = true;
            },
            Command::Tag(tag) => {
                if let Some(i) = state.query.tags.iter().position(|v| v == &tag) {
                    state.query.tags.remove(i);
                } else {
                    state.query.tags.push(tag);
                }
                state.page = 0;
                refresh = true;
            },
            Command::Input(field) => {
                state.original_text = self.omnibar.text.clone();
                self.omnibar.text.clear();
                self.omnibar.cursor = 0;
                state.view = View::Input(field);
            },
            Command::Submit(value) => {
                let View::Input(field) = state.view.clone() else {
                    return vec![];
                };
                let original = std::mem::take(&mut state.original_text);
                self.omnibar.text = original;
                self.omnibar.cursor = self.omnibar.text.len();
                match apply_value(state, &self.data_root, &field, &value) {
                    Ok(()) => {
                        state.error = None;
                        state.page = 0;
                        state.view = if field.starts_with("date_") {
                            View::Dates
                        } else {
                            View::Settings
                        };
                        refresh = true;
                    },
                    Err(error) => {
                        state.error = Some(error);
                        state.view = if field.starts_with("date_") {
                            View::Dates
                        } else {
                            View::Settings
                        };
                    },
                }
            },
            Command::Page(delta) => {
                state.page = state.page.saturating_add_signed(delta);
                refresh = true;
            },
            Command::Open { target, request } => {
                let valid = state.accepted.as_ref().is_some_and(|accepted| {
                    accepted.id == request
                        && self.research.context_revision().ok()
                            == Some((accepted.context.clone(), accepted.revision))
                        && self
                            .research
                            .clip_access()
                            .ok()
                            .and_then(|access| (access.epoch)())
                            == Some(accepted.epoch)
                }) && state.result.as_ref().is_some_and(|result| {
                    result.hits.iter().any(|hit| {
                        let Ok(mut actual) = crate::research::target(&hit.source, &hit.version)
                        else {
                            return false;
                        };
                        if let Some(passage) = &hit.passage {
                            actual.set_selector(crate::saved_sources::Selector {
                                start: passage.selector.start,
                                end: passage.selector.end,
                            });
                        }
                        actual == target
                    })
                });
                if !valid {
                    return self
                        .saved_source_refusal("The search result is stale or unavailable.".into());
                }
                self.omnibar.open = false;
                self.shell.close_omnibar();
                return match target {
                    crate::research::Target::Capture(target) => self.open_saved_source(target),
                    crate::research::Target::Clip(target) => self.open_research_clip(target),
                };
            },
            _ => {},
        }
        let mut effects = if refresh {
            self.research_search.error = None;
            self.issue_research_search()
        } else {
            vec![Effect::Redraw]
        };
        self.recompute_omnibar_suggestions();
        effects.push(Effect::Redraw);
        effects
    }
}
