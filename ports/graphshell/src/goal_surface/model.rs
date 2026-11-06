// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The view model of one goal run: plain data the surface renders and tests
//! assert on. Built from a traversal report and the retained snapshot it ran
//! over; it holds citations and explanations, never a graph change.

use super::provider::RetainedPages;
use eidetic::models::recall::RecallSourceId;
use eidetic_search::goal::{GoalAssessment, GoalVerdict, SelectionChoice, SelectionRule};
use fleece::retained_traversal::{
    RetainedFailure, RetainedStop, RetainedTraversalReport, recall_source,
};

/// Characters of retained text shown either side of a cited passage.
pub const CONTEXT_CHARS: usize = 48;
/// Cited passages rendered per source; the rest are counted.
pub const MAX_SNIPPETS: usize = 4;

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

/// A source that was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisitView {
    pub requested_version: Option<String>,
    pub retained_version: String,
    pub text_hash: String,
    pub bytes: usize,
    pub verdict: GoalVerdict,
    pub coverage_permille: u16,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub snippets: Vec<Snippet>,
    pub more_snippets: usize,
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

/// Byte offsets for a half-open range of Unicode scalar positions, clamped to
/// the text. Eidetic's `PassageSelector::to_byte_range` does the same once the
/// pinned Eidetic carries it.
fn byte_range(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start > end {
        return None;
    }
    let mut bytes = text.char_indices().map(|(b, _)| b).chain([text.len()]);
    let start_byte = bytes.clone().nth(start)?;
    let end_byte = bytes.nth(end)?;
    Some((start_byte, end_byte))
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

fn visit_view(
    pages: &RetainedPages,
    source: &RecallSourceId,
    requested: Option<String>,
    version: &str,
    assessment: Option<&GoalAssessment>,
    bytes: usize,
) -> VisitView {
    let text = pages.text_of(source, version).unwrap_or_default();
    let selectors = assessment.map(|a| a.matches.as_slice()).unwrap_or_default();
    // One cited passage per distinct matched word, first occurrence first, so
    // a long source shows each goal term once rather than the same word four times.
    let mut seen = std::collections::BTreeSet::new();
    let snippets: Vec<Snippet> = selectors
        .iter()
        .filter_map(|s| snippet(text, s.start, s.end, CONTEXT_CHARS))
        .filter(|s| seen.insert(s.hit.to_lowercase()))
        .take(MAX_SNIPPETS)
        .collect();
    let more = selectors.len().saturating_sub(snippets.len());
    VisitView {
        requested_version: requested,
        retained_version: version.to_owned(),
        text_hash: assessment
            .map(|a| a.text_hash.to_hex())
            .unwrap_or_default(),
        bytes,
        verdict: assessment.map_or(GoalVerdict::Continue, |a| a.verdict),
        coverage_permille: assessment.map_or(0, |a| a.coverage_permille),
        matched_terms: assessment.map(|a| a.matched_terms.clone()).unwrap_or_default(),
        missing_terms: assessment.map(|a| a.missing_terms.clone()).unwrap_or_default(),
        snippets,
        more_snippets: more + usize::from(assessment.is_some_and(|a| a.matches_truncated)),
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
        let mut steps = Vec::new();
        for (index, choice) in report.goal.selections.iter().enumerate() {
            let visit = visits.iter().enumerate().find(|(i, v)| {
                !next_visit[*i] && source_of(&v.candidate.reference).as_ref() == Some(&choice.source)
            });
            let failure = failures.iter().enumerate().find(|(i, f)| {
                !next_failure[*i] && source_of(&f.candidate.reference).as_ref() == Some(&choice.source)
            });
            let (address, title) = pages.describe(&choice.source).unwrap_or_else(|| {
                (format!("unknown source {}", choice.source.source), None)
            });
            let (depth, outcome) = match (visit, failure) {
                (Some((i, v)), _) => {
                    next_visit[i] = true;
                    let content = &v.observation.content;
                    (
                        Some(v.candidate.depth),
                        Outcome::Visited(visit_view(
                            pages,
                            &choice.source,
                            v.candidate.reference.version.clone(),
                            &content.version,
                            report.goal.assessments.get(i),
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
            steps.push(StepView {
                number: index + 1,
                address,
                title,
                source: choice.source.clone(),
                reason: selection_reason(choice),
                pending: choice.pending,
                depth,
                outcome,
            });
        }
        let o = report.traversal.omissions;
        Self {
            goal: report.goal.goal.clone(),
            terms: report.goal.terms.clone(),
            operation: report.goal.operation_version.clone(),
            tokenizer: report.goal.tokenizer.clone(),
            min_coverage_permille: report.goal.settings.min_coverage_permille,
            satisfy_after: report.goal.settings.satisfy_after,
            stop: report.stop,
            stop_reason: stop_reason(report.stop).into(),
            matched: report.goal.matched,
            requests: report.requests,
            bytes: report.bytes,
            budgets,
            steps,
            omissions: [o.depth, o.fanout, o.pending, o.duplicates],
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
