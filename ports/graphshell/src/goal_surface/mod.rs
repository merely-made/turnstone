// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A goal surface for Graphshell's native host: a person types a goal, runs
//! it over what Graphshell already retains, and reads cited results with the
//! traversal's reasons.
//!
//! The ordinary path is Eidetic's model-free lexical `GoalAssessor` under
//! Fleece's `run_retained_traversal`; no model, network or clock is involved.
//! A run reads a [`RetainedPages`] snapshot and writes nothing: the result is
//! view state (`GoalSurface::run`), and neither the graph nor any store
//! changes. It shows cited results and explanations only; proposals belong to
//! a later slice and are not offered here.

mod facts;
mod model;
mod model_view;
#[cfg(feature = "goal-surface-model")]
pub mod model_path;
#[cfg(feature = "goal-surface-model")]
mod model_controls;
mod view_model;
#[cfg(all(test, feature = "goal-surface-model"))]
mod model_tests;
mod provider;
pub mod door;
pub mod suggest;
pub(crate) mod view;
pub mod fixture;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod door_tests;
#[cfg(test)]
mod suggest_tests;

pub use model::{
    Budgets, GoalRunView, ModelPick, ModelRun, ModelVisit, Outcome, Passage, Segment, Snippet, StepView, VisitView, failure_reason, selection_reason,
    passage, snippet, stop_reason, verdict_label,
};
pub use fleece::retained_traversal::RetainedTraversalReceipt;
pub use facts::{ConceptFact, GraphFacts};
pub use provider::{LoadBounds, Origin, PagesProvider, RetainedPage, RetainedPages, record_text, resource_of, source_id};
pub use model_view::score;
pub use view::{CSS, view};

use cambium::TextInput;
use eidetic_search::goal::GoalSettings;
use fleece::retained_traversal::{RetainedLimits, RetainedSeed, run_retained_traversal};
use fleece::traversal::TraversalPolicy;
use std::rc::Rc;

/// Where a run begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Every retained page is a seed.
    All,
    /// One retained page, by position in [`RetainedPages::sources`].
    Source(usize),
}

/// The surface's state. Everything here is view state; dropping it changes
/// nothing durable.
pub struct GoalSurface {
    pages: Rc<RetainedPages>,
    pub goal: TextInput,
    pub max_visits: TextInput,
    pub max_depth: TextInput,
    pub start: Start,
    pub run: Option<GoalRunView>,
    /// The serializable account of the last run, for a host that records evidence.
    pub receipt: Option<RetainedTraversalReceipt>,
    pub notice: Option<String>,
    /// Replaces the opening sentence when the host knows better what it reads.
    pub intro: Option<String>,
    /// Whether the surface shows its own title; a host that titles it hides it.
    pub heading: bool,
    /// The opt-in model path: toggle, worker and what it reported.
    #[cfg(feature = "goal-surface-model")]
    pub model: model_path::ModelPanel,
}

impl GoalSurface {
    pub fn new(pages: Rc<RetainedPages>) -> Self {
        let defaults = TraversalPolicy::default();
        Self {
            pages,
            goal: TextInput::new(""),
            max_visits: TextInput::new(defaults.max_nodes.to_string()),
            max_depth: TextInput::new(defaults.max_depth.to_string()),
            start: Start::All,
            run: None,
            receipt: None,
            notice: None,
            intro: None,
            heading: true,
            #[cfg(feature = "goal-surface-model")]
            model: model_path::ModelPanel::default(),
        }
    }

    /// Swap in a fresh snapshot after a refresh, keeping what was typed and
    /// the last result (which holds its own copies of what it cited).
    pub fn replace_pages(&mut self, pages: Rc<RetainedPages>) {
        if let Start::Source(i) = self.start {
            if i >= pages.sources().len() {
                self.start = Start::All;
            }
        }
        self.pages = pages;
        self.notice = None;
    }

    pub fn pages(&self) -> &RetainedPages {
        &self.pages
    }

    fn number<T: std::str::FromStr>(input: &TextInput, what: &str) -> Result<T, String> {
        input
            .text()
            .trim()
            .parse()
            .map_err(|_| format!("{what} must be a whole number."))
    }

    /// The limits as typed. Request and byte budgets keep Fleece's defaults.
    pub fn limits(&self) -> Result<(TraversalPolicy, RetainedLimits, Budgets), String> {
        let max_nodes: usize = Self::number(&self.max_visits, "Visits")?;
        let max_depth: u32 = Self::number(&self.max_depth, "Depth")?;
        if max_nodes == 0 {
            return Err("Visits must be at least 1.".into());
        }
        let policy = TraversalPolicy {
            max_nodes,
            max_depth,
            ..TraversalPolicy::default()
        };
        let limits = RetainedLimits::default();
        let budgets = Budgets {
            max_nodes,
            max_depth,
            max_requests: limits.max_requests,
            max_bytes: limits.max_bytes,
        };
        Ok((policy, limits, budgets))
    }

    /// Run the typed goal over the retained snapshot. A refusal (empty goal,
    /// bad limit) leaves the previous result and sets a notice.
    pub fn run(&mut self) {
        self.notice = None;
        let (policy, limits, budgets) = match self.limits() {
            Ok(all) => all,
            Err(refusal) => return self.notice = Some(refusal),
        };
        let sources = self.pages.sources();
        let chosen: Vec<_> = match self.start {
            Start::All => sources.iter().map(|(s, _)| s.clone()).collect(),
            Start::Source(i) => sources.get(i).map(|(s, _)| s.clone()).into_iter().collect(),
        };
        let seeds: Vec<RetainedSeed> = chosen
            .into_iter()
            .map(|source| RetainedSeed { source, version: None })
            .collect();
        if seeds.is_empty() {
            return self.notice = Some("Nothing is retained to start from.".into());
        }
        let goal = self.goal.text().to_owned();
        #[cfg(feature = "goal-surface-model")]
        if self.model.enabled {
            // Refuse an unsearchable goal now, as the ordinary path does.
            if let Err(refusal) = eidetic_search::goal::GoalAssessor::new(&goal, GoalSettings::default()) {
                return self.notice = Some(format!("The goal cannot be searched: {refusal}"));
            }
            if self.model.running {
                return self.notice = Some("A model run is still going; wait for it.".into());
            }
            return self.run_with_model(model_path::Request { seeds, goal, policy, limits, budgets });
        }
        self.run_lexical(seeds, &goal, policy, limits, budgets);
    }

    /// The ordinary path: Eidetic's lexical assessor under Fleece's traversal.
    fn run_lexical(&mut self, seeds: Vec<RetainedSeed>, goal: &str, policy: TraversalPolicy, limits: RetainedLimits, budgets: Budgets) {
        let mut provider = self.pages.provider();
        match run_retained_traversal(&mut provider, seeds, goal, GoalSettings::default(), policy, limits, || false) {
            Ok(report) => {
                self.run = Some(GoalRunView::build(&report, &self.pages, budgets));
                self.receipt = Some(report.receipt());
            },
            Err(refusal) => self.notice = Some(format!("The goal cannot be searched: {refusal}")),
        }
    }
}
