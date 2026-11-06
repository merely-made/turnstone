// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The view model of one goal run: plain data the surface renders and tests
//! assert on. Built from a traversal report and the retained snapshot it ran
//! over; it holds citations and explanations, never a graph change.

use super::provider::RetainedPages;
use eidetic::models::recall::{PassageSelector, RecallSourceId};
use eidetic_search::goal::{GoalAssessment, GoalVerdict, SelectionChoice, SelectionRule};
use super::model_view::{model_pick, model_selection_reason, model_visit};
use eidetic_search::goal_model::{ModelAssessRule, ModelStatus, SelectionWinner};
use fleece::retained_traversal::{
    RetainedFailure, RetainedStop, RetainedTraversalReport, recall_source,
};

/// Characters of retained text shown either side of a cited passage.
pub const CONTEXT_CHARS: usize = 48;
/// Cited passages rendered per source; the rest are counted.
pub const MAX_SNIPPETS: usize = 4;

/// A run of a passage's text; `hit` marks a goal-term match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}

/// One cited passage: the retained text of an Eidetic passage selector, split
/// into plain and matched runs so every goal term inside it is highlighted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    pub segments: Vec<Segment>,
    /// The passage, in Unicode scalar offsets of the retained text.
    pub start: usize,
    pub end: usize,
}

impl Passage {
    /// The passage text with its highlights unmarked.
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }

    /// The highlighted runs, in order.
    pub fn hits(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().filter(|s| s.hit).map(|s| s.text.as_str())
    }
}

/// A cited passage: the retained text split around one selector's match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub before: String,
    pub hit: String,
    pub after: String,
    /// The selector, in Unicode scalar offsets of the retained text.
    pub start: usize,
    pub end: usize,
}

/// What the model said about one source beside the lexical answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelVisit {
    pub s1_verdict: GoalVerdict,
    pub s1_coverage_permille: u16,
    /// Best chunk's cosine to the goal, in thousandths; `None` when the model
    /// could not answer this step.
    pub similarity_milli: Option<i16>,
    pub threshold_milli: i16,
    /// Which rule decided the verdict, in a plain sentence.
    pub decided_by: String,
    /// Why the model was unavailable for this step, when it was.
    pub unavailable: Option<String>,
}

/// How a selection was made when a model ranked beside the lexical rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPick {
    pub winner: SelectionWinner,
    /// The model's cosine for the candidate taken, in thousandths.
    pub score_milli: Option<i16>,
    /// What the lexical rule would have taken, and what the model would have.
    pub lexical_pick: String,
    pub model_pick: Option<String>,
    pub unavailable: Option<String>,
}

/// The model's part of a run, shown with the run's own limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRun {
    pub rule: ModelAssessRule,
    pub threshold_milli: i16,
    /// Where the threshold came from, or why it is only a placeholder.
    pub threshold_note: String,
    /// Sources the lexical rule alone would have matched.
    pub lexical_matched: usize,
    /// Steps where the model could not answer (selections and assessments).
    pub unavailable_steps: usize,
}

/// A source that was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisitView {
    pub model: Option<ModelVisit>,
    pub requested_version: Option<String>,
    pub retained_version: String,
    pub text_hash: String,
    pub bytes: usize,
    pub verdict: GoalVerdict,
    pub coverage_permille: u16,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub passages: Vec<Passage>,
    pub more_passages: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Visited(VisitView),
    /// The lookup found nothing it could read, with the plain reason.
    Failed(String),
    /// The traversal ended before this selection's lookup was recorded.
    Unrecorded,
}

/// One selection and what came of it, in traversal order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepView {
    pub number: usize,
    pub address: String,
    pub title: Option<String>,
    pub source: RecallSourceId,
    /// Why this candidate was chosen over the other pending ones.
    pub reason: String,
    pub pending: usize,
    pub model: Option<ModelPick>,
    pub depth: Option<u32>,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    pub max_nodes: usize,
    pub max_depth: u32,
    pub max_requests: usize,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalRunView {
    pub goal: String,
    pub terms: Vec<String>,
    pub operation: String,
    pub tokenizer: String,
    pub min_coverage_permille: u16,
    pub satisfy_after: usize,
    pub stop: RetainedStop,
    pub stop_reason: String,
    pub matched: usize,
    pub requests: usize,
    pub bytes: usize,
    pub budgets: Budgets,
    pub steps: Vec<StepView>,
    /// Candidates the policy left out, by cause: (depth, fan-out, pending, repeat).
    pub omissions: [usize; 4],
    /// Present when the model drove the run.
    pub model: Option<ModelRun>,
}

pub fn stop_reason(stop: RetainedStop) -> &'static str {
    match stop {
        RetainedStop::Exhausted => "Every reachable retained source was read.",
        RetainedStop::NodeBudget => "Stopped at the visit budget; more sources were reachable.",
        RetainedStop::RequestBudget => "Stopped at the lookup budget; more sources were reachable.",
        RetainedStop::ByteBudget => "Stopped at the byte budget; the next text would not fit.",
        RetainedStop::Cancelled => "Cancelled before the walk finished.",
        RetainedStop::GoalSatisfied => "Enough sources matched the goal; the walk ended early.",
        RetainedStop::SelectionStopped => "The selection rule ended the walk.",
        RetainedStop::InvalidSelection => "The selection rule named a candidate that was not pending.",
    }
}

pub fn failure_reason(failure: &RetainedFailure) -> String {
    match failure {
        RetainedFailure::Missing => "Nothing is retained for this source; it was not fetched.".into(),
        RetainedFailure::VersionMismatch { requested, retained } => format!(
            "A different version is retained (asked {}, held {}); not substituted.",
            short(requested),
            short(retained)
        ),
        RetainedFailure::SourceMismatch => "The retained text belongs to a different source.".into(),
        RetainedFailure::Oversized => "The retained text exceeds the goal's text limit.".into(),
        RetainedFailure::ByteBudget { needed, remaining } => {
            format!("Needs {needed} bytes but {remaining} remained in the byte budget.")
        },
    }
}

pub fn verdict_label(verdict: GoalVerdict) -> &'static str {
    match verdict {
        GoalVerdict::Continue => "not a match",
        GoalVerdict::Match => "match",
        GoalVerdict::Satisfied => "match, goal satisfied",
    }
}

pub fn selection_reason(choice: &SelectionChoice) -> String {
    match choice.rule {
        SelectionRule::GoalTermOverlap => format!(
            "Its label shares goal terms ({}); best of {} pending.",
            choice.matched_terms.join(", "),
            choice.pending
        ),
        SelectionRule::OldestFallback => format!(
            "No pending label shared a goal term; took the oldest of {} pending.",
            choice.pending
        ),
    }
}

/// First twelve characters of a hex version or hash.
pub fn short(hex: &str) -> String {
    hex.chars().take(12).collect()
}

/// Byte offsets for a half-open range of Unicode scalar positions; `None` when
/// the range is reversed or reaches past the text.
fn byte_range(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let range = PassageSelector { start, end }.to_byte_range(text).ok()?;
    Some((range.start, range.end))
}

/// Split `text` around the scalar range, with up to `context` scalars each side.
pub fn snippet(text: &str, start: usize, end: usize, context: usize) -> Option<Snippet> {
    let (from, to) = byte_range(text, start, end)?;
    let (lead, _) = byte_range(text, start.saturating_sub(context), start)?;
    let total = text.chars().count();
    let (_, tail) = byte_range(text, end, (end + context).min(total))?;
    let tidy = |s: &str| s.replace(['\n', '\r'], " ");
    Some(Snippet {
        before: tidy(&text[lead..from]),
        hit: tidy(&text[from..to]),
        after: tidy(&text[to..tail]),
        start,
        end,
    })
}

/// One passage of `text`, with each of `matches` that lies inside it marked.
/// `None` when the passage selector does not fit the text.
pub fn passage(text: &str, selector: &PassageSelector, matches: &[PassageSelector]) -> Option<Passage> {
    let (from, to) = byte_range(text, selector.start, selector.end)?;
    let mut spans: Vec<(usize, usize)> = matches
        .iter()
        .filter(|m| m.start >= selector.start && m.end <= selector.end)
        .filter_map(|m| byte_range(text, m.start, m.end))
        .filter(|(a, b)| a < b)
        .collect();
    spans.sort_unstable();
    let tidy = |s: &str| s.replace(['\n', '\r'], " ");
    let (mut segments, mut at) = (Vec::new(), from);
    for (a, b) in spans {
        if a < at {
            continue;
        }
        if a > at {
            segments.push(Segment { text: tidy(&text[at..a]), hit: false });
        }
        segments.push(Segment { text: tidy(&text[a..b]), hit: true });
        at = b;
    }
    if at < to {
        segments.push(Segment { text: tidy(&text[at..to]), hit: false });
    }
    Some(Passage { segments, start: selector.start, end: selector.end })
}

fn visit_view(
    pages: &RetainedPages,
    source: &RecallSourceId,
    requested: Option<String>,
    version: &str,
    assessment: Option<&GoalAssessment>,
    verdict: GoalVerdict,
    model: Option<ModelVisit>,
    bytes: usize,
) -> VisitView {
    let text = pages.text_of(source, version).unwrap_or_default();
    let matches = assessment.map(|a| a.matches.as_slice()).unwrap_or_default();
    // One cited snippet per passage (merged hits plus context), with every
    // goal-term match inside it highlighted. Records without passages fall
    // back to a padded window around each match.
    let windows: Vec<PassageSelector> = match assessment {
        Some(a) if !a.passages.is_empty() => a.passages.clone(),
        _ => {
            let total = text.chars().count();
            matches
                .iter()
                .map(|m| PassageSelector {
                    start: m.start.saturating_sub(CONTEXT_CHARS),
                    end: (m.end + CONTEXT_CHARS).min(total),
                })
                .collect()
        },
    };
    let passages: Vec<Passage> = windows
        .iter()
        .filter_map(|w| passage(text, w, matches))
        .take(MAX_SNIPPETS)
        .collect();
    let more = windows.len().saturating_sub(passages.len());
    let truncated = assessment.is_some_and(|a| {
        if a.passages.is_empty() { a.matches_truncated } else { a.passages_truncated }
    });
    VisitView {
        requested_version: requested,
        retained_version: version.to_owned(),
        text_hash: assessment
            .map(|a| a.text_hash.to_hex())
            .unwrap_or_default(),
        bytes,
        verdict,
        model,
        coverage_permille: assessment.map_or(0, |a| a.coverage_permille),
        matched_terms: assessment.map(|a| a.matched_terms.clone()).unwrap_or_default(),
        missing_terms: assessment.map(|a| a.missing_terms.clone()).unwrap_or_default(),
        passages,
        more_passages: more + usize::from(truncated),
    }
}

impl GoalRunView {
    /// Interleave selections with what each lookup produced. The report keeps
    /// visits and failures apart; the goal record's selections are in lookup
    /// order, so each selection takes the next unused outcome for its source.
    pub fn build(
        report: &RetainedTraversalReport,
        pages: &RetainedPages,
        budgets: Budgets,
    ) -> Self {
        let source_of = |r: &fleece::traversal::ContentRef| recall_source(r);
        let visits = &report.traversal.visits;
        let failures = &report.traversal.failures;
        let (mut next_visit, mut next_failure) = (vec![false; visits.len()], vec![false; failures.len()]);
        let model = report.model.as_ref();
        let threshold = model.map_or(0, |m| m.settings.match_similarity_milli);
        let name = |source: &RecallSourceId| {
            pages.describe(source).map_or_else(|| source.source.clone(), |(a, t)| t.unwrap_or(a))
        };
        let mut steps = Vec::new();
        for (index, trace) in report.selections.iter().enumerate() {
            let choice = &report.goal.selections[index];
            let source = trace.candidates[trace.chosen].clone();
            let visit = visits.iter().enumerate().find(|(i, v)| {
                !next_visit[*i] && source_of(&v.candidate.reference).as_ref() == Some(&source)
            });
            let failure = failures.iter().enumerate().find(|(i, f)| {
                !next_failure[*i] && source_of(&f.candidate.reference).as_ref() == Some(&source)
            });
            let (address, title) = pages.describe(&source).unwrap_or_else(|| {
                (format!("unknown source {}", source.source), None)
            });
            let (depth, outcome) = match (visit, failure) {
                (Some((i, v)), _) => {
                    next_visit[i] = true;
                    let content = &v.observation.content;
                    let decided = match v.goal {
                        fleece::traversal::GoalDecision::Continue => GoalVerdict::Continue,
                        fleece::traversal::GoalDecision::Match => GoalVerdict::Match,
                        fleece::traversal::GoalDecision::Satisfied => GoalVerdict::Satisfied,
                    };
                    let seen = model.and_then(|m| m.assessments.get(i)).map(|a| model_visit(a, threshold));
                    (
                        Some(v.candidate.depth),
                        Outcome::Visited(visit_view(
                            pages,
                            &source,
                            v.candidate.reference.version.clone(),
                            &content.version,
                            report.goal.assessments.get(i),
                            decided,
                            seen,
                            content.text.len() + content.title.as_ref().map_or(0, String::len),
                        )),
                    )
                },
                (None, Some((i, f))) => {
                    next_failure[i] = true;
                    (Some(f.candidate.depth), Outcome::Failed(failure_reason(&f.error)))
                },
                (None, None) => (None, Outcome::Unrecorded),
            };
            let picked = model.and_then(|m| m.selections.get(index));
            steps.push(StepView {
                number: index + 1,
                address,
                title,
                source,
                reason: picked.map_or_else(
                    || selection_reason(choice),
                    |p| model_selection_reason(p, trace, &name, &selection_reason(&p.s1)),
                ),
                pending: choice.pending,
                model: picked.map(|p| model_pick(p, trace, &name)),
                depth,
                outcome,
            });
        }
        let o = report.traversal.omissions;
        let unavailable_steps = model.map_or(0, |m| {
            m.selections.iter().filter(|s| matches!(s.status, ModelStatus::Unavailable { .. })).count()
                + m.assessments.iter().filter(|a| matches!(a.status, ModelStatus::Unavailable { .. })).count()
        });
        Self {
            goal: report.goal.goal.clone(),
            terms: report.goal.terms.clone(),
            operation: report.goal.operation_version.clone(),
            tokenizer: report.goal.tokenizer.clone(),
            min_coverage_permille: report.goal.settings.min_coverage_permille,
            satisfy_after: report.goal.settings.satisfy_after,
            stop: report.stop,
            stop_reason: stop_reason(report.stop).into(),
            matched: model.map_or(report.goal.matched, |m| m.matched),
            requests: report.requests,
            bytes: report.bytes,
            budgets,
            steps,
            omissions: [o.depth, o.fanout, o.pending, o.duplicates],
            model: model.map(|m| ModelRun {
                rule: m.settings.assess_rule,
                threshold_milli: threshold,
                threshold_note: String::new(),
                lexical_matched: m.assessments.iter().filter(|a| a.s1_match).count(),
                unavailable_steps,
            }),
        }
    }

    /// Steps that matched the goal, in traversal order: the answer.
    pub fn matches(&self) -> impl Iterator<Item = &StepView> {
        self.steps.iter().filter(|s| {
            matches!(&s.outcome, Outcome::Visited(v) if v.verdict != GoalVerdict::Continue)
        })
    }

    /// One plain sentence on what was left out, or `None` when nothing was.
    pub fn omission_summary(&self) -> Option<String> {
        let [depth, fanout, pending, repeats] = self.omissions;
        let parts: Vec<String> = [
            (depth, "past the depth limit"),
            (fanout, "beyond a source's link limit"),
            (pending, "beyond the pending limit"),
            (repeats, "already queued"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, why)| format!("{n} {why}"))
        .collect();
        (!parts.is_empty()).then(|| format!("Left out: {}.", parts.join(", ")))
    }
}
