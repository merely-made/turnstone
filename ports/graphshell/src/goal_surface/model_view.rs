// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Plain-language reading of Eidetic's model record: what the model said per
//! source and per selection, and which rule decided. Pure data in, strings out.

use super::model::{ModelPick, ModelVisit};
use eidetic::models::recall::RecallSourceId;
use eidetic_search::goal_model::{ModelAssessRule, ModelAssessment, ModelSelection, ModelStatus, SelectionWinner};
use fleece::retained_traversal::SelectionTrace;

/// A cosine in thousandths as a decimal, `0.312`.
pub fn score(milli: i16) -> String {
    format!("{:.3}", f64::from(milli) / 1000.0)
}

fn reason_of(status: &ModelStatus) -> Option<String> {
    match status {
        ModelStatus::Used => None,
        ModelStatus::Unavailable { reason } => Some(reason.clone()),
    }
}

/// Which side decided a verdict, under the rule in force.
pub fn decided_by(a: &ModelAssessment) -> String {
    let (lexical, model) = (a.s1_match, a.model_match);
    match (model, a.rule) {
        (None, _) => "lexical rule alone (the model could not answer this source)".into(),
        (Some(_), ModelAssessRule::RecordOnly) => "lexical rule alone (the model's score is recorded, not used)".into(),
        (Some(m), ModelAssessRule::ModelReplaces) => {
            format!("model alone: similarity {} the threshold", if m { "reached" } else { "was below" })
        },
        (Some(m), ModelAssessRule::Or) => match (lexical, m) {
            (true, true) => "both: the lexical rule matched and the model reached its threshold".into(),
            (true, false) => "lexical rule (the model was below its threshold)".into(),
            (false, true) => "model (the lexical rule found no match)".into(),
            (false, false) => "neither: the lexical rule did not match and the model was below its threshold".into(),
        },
    }
}

pub fn model_visit(a: &ModelAssessment, threshold_milli: i16) -> ModelVisit {
    ModelVisit {
        s1_verdict: a.s1_verdict,
        s1_coverage_permille: a.s1_coverage_permille,
        similarity_milli: a.similarity_milli,
        threshold_milli,
        decided_by: decided_by(a),
        unavailable: reason_of(&a.status),
    }
}

pub fn model_pick(
    p: &ModelSelection,
    trace: &SelectionTrace,
    name: &impl Fn(&RecallSourceId) -> String,
) -> ModelPick {
    ModelPick {
        winner: p.winner,
        score_milli: p.scores_milli.get(p.chosen).copied().flatten(),
        lexical_pick: name(&p.s1.source),
        model_pick: p.model_pick.and_then(|i| trace.candidates.get(i)).map(name),
        unavailable: reason_of(&p.status),
    }
}

/// Why a candidate was chosen when the model ranked beside the lexical rule.
pub fn model_selection_reason(
    p: &ModelSelection,
    trace: &SelectionTrace,
    name: &impl Fn(&RecallSourceId) -> String,
    lexical: &str,
) -> String {
    let pick = model_pick(p, trace, name);
    let pending = trace.candidates.len();
    match (p.winner, &pick.unavailable) {
        (_, Some(why)) => format!("Model unavailable here ({why}); the lexical rule chose. {lexical}"),
        (SelectionWinner::Model, None) => {
            let score = pick.score_milli.map_or(String::new(), |s| format!(" (similarity {})", score(s)));
            if p.chosen == p.s1.index {
                format!("The model and the lexical rule agree{score}; best of {pending} pending.")
            } else {
                format!(
                    "The model ranked it highest{score}, best of {pending} pending; the lexical rule would have taken {}.",
                    pick.lexical_pick
                )
            }
        },
        (SelectionWinner::S1, None) => match &pick.model_pick {
            Some(other) if p.chosen != p.model_pick.unwrap_or(p.chosen) => {
                format!("The lexical rule picked it. {lexical} The model would have taken {other}.")
            },
            _ => format!("The lexical rule picked it. {lexical}"),
        },
    }
}
