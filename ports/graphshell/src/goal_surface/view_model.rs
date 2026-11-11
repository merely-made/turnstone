// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The model's lines in the cited results and the traversal explanation. All
//! of it hangs off optional model data, so an ordinary run renders none.

use super::model::{GoalRunView, ModelVisit, VisitView, verdict_label};
use super::model_view::score;
use super::view::Child;
use cambium::el;
use eidetic_search::goal_model::{ModelAssessRule, SelectionWinner};

pub fn rule_label(rule: ModelAssessRule) -> &'static str {
    match rule {
        ModelAssessRule::Or => "lexical or model",
        ModelAssessRule::ModelReplaces => "model alone",
        ModelAssessRule::RecordOnly => "lexical alone (model recorded)",
    }
}

/// `Lexical: ... · Model similarity ...` for one source.
pub fn visit_line(visit: &VisitView) -> Option<String> {
    let m: &ModelVisit = visit.model.as_ref()?;
    let similarity = match (m.similarity_milli, &m.unavailable) {
        (Some(s), _) => format!("Model similarity {} (threshold {})", score(s), score(m.threshold_milli)),
        (None, why) => format!("Model unavailable for this source ({})", why.as_deref().unwrap_or("no answer")),
    };
    Some(format!(
        "Lexical: {}, coverage {}% · {} · Decided by: {}",
        verdict_label(m.s1_verdict),
        f64::from(m.s1_coverage_permille) / 10.0,
        similarity,
        m.decided_by
    ))
}

pub fn visit_note(visit: &VisitView) -> Option<Child> {
    visit_line(visit).map(|line| Box::new(el("p", line).attr("class", "goal-meta goal-model-line")) as Child)
}

/// The run-level model sentence for the meta paragraph.
pub fn run_summary(run: &GoalRunView) -> Option<String> {
    let m = run.model.as_ref()?;
    let mut text = format!(
        "Model path: rule {}, match threshold {} ({}). The lexical rule alone would have matched {} of the sources read; the combined rule matched {}.",
        rule_label(m.rule),
        score(m.threshold_milli),
        m.threshold_note,
        m.lexical_matched,
        run.matched
    );
    if m.unavailable_steps > 0 {
        text.push_str(&format!(
            " The model was unavailable at {} step(s); those used the lexical answer.",
            m.unavailable_steps
        ));
    }
    Some(text)
}

/// Whether the model, not the lexical rule, took this selection.
pub fn model_chose(step: &super::model::StepView) -> bool {
    step.model.as_ref().is_some_and(|p| p.winner == SelectionWinner::Model && p.unavailable.is_none())
}
