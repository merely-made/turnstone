// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Cambium rendering of the goal surface. Building the view never runs a goal;
//! only the Find button does.

use super::model::{Outcome, Passage, Segment, StepView, short, verdict_label};
use super::view_model::{model_chose, run_summary, visit_note};
use super::{GoalRunView, GoalSurface, Start};
use cambium::{AnyView, GenetCtx, GenetElement, TextInput, button, el, lens, text_field_typed};

/// Start buttons listed; a large profile searches the rest without a button each.
const MAX_STARTS: usize = 24;

pub type Child = Box<dyn AnyView<GoalSurface, (), GenetCtx, GenetElement>>;

/// A highlight that starts a wrapped line paints as an empty box when the mark
/// is an ordinary inline (the engine's inline layout drops the text of an
/// inline box that begins on a continuation line). `inline-block` lays the word
/// out as one unit, and `pre-wrap` on the snippet keeps the spaces beside it
/// that an inline-block edge would otherwise collapse. Kept in both sheets.
pub const HIGHLIGHT_RULES: [&str; 2] = [".goal-snippet{white-space:pre-wrap}", "mark{display:inline-block"];

/// Geometry and the highlight only; the host supplies theme and typography.
pub const CSS: &str = ".goal-surface{display:flex;flex-direction:column;gap:10px}.goal-surface label{display:flex;align-items:center;gap:8px}.goal-input{min-width:420px;min-height:28px;padding:4px;border:1px solid #6d7681;background:#fff;white-space:pre;overflow:hidden}.goal-small{min-width:60px}.goal-starts{display:flex;flex-wrap:wrap;gap:4px}.goal-surface button[aria-pressed=true]{border-color:#165bb0;font-weight:bold}.goal-snippet{white-space:pre-wrap}.goal-surface mark{display:inline-block;background:#ffd54a;color:#1b1b1b}.goal-surface ol{padding-left:22px}.goal-surface li{margin-bottom:8px}.goal-surface p{margin:2px 0;overflow-wrap:anywhere}.goal-meta{font-size:13px;color:#4a525a}.goal-notice{color:#9a4a00}.goal-cite{padding:6px;border:1px solid #9aa3ab;background:#fff}.goal-step-failed{color:#6b2a2a}";

fn field(label: &'static str, id: &'static str, class: &'static str, pick: fn(&mut GoalSurface) -> &mut TextInput) -> Child {
    Box::new(el(
        "label",
        (
            label,
            lens(
                move |input: &mut TextInput| {
                    text_field_typed(input)
                        .attr("aria-label", label)
                        .attr("class", class)
                        .attr("id", id)
                },
                pick,
            ),
        ),
    ))
}

/// One cited passage: plain runs as spans, each goal-term match as a mark.
fn passage_line(passage: &Passage) -> Child {
    // The ellipses are plain runs of their own, so a highlight's node holds
    // exactly the matched word, even when it opens or closes the passage.
    let run = |seg: &Segment| -> Child {
        if seg.hit { Box::new(el("mark", seg.text.clone())) } else { Box::new(el("span", seg.text.clone())) }
    };
    let mut runs: Vec<Child> = vec![Box::new(el("span", "…"))];
    runs.extend(passage.segments.iter().map(run));
    runs.push(Box::new(el("span", "…")));
    Box::new(el("p", runs).attr("class", "goal-snippet"))
}

fn heading(step: &StepView) -> String {
    step.title.clone().unwrap_or_else(|| step.address.clone())
}

fn cited(step: &StepView) -> Option<Child> {
    let Outcome::Visited(v) = &step.outcome else {
        return None;
    };
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", heading(step))),
        Box::new(el(
            "p",
            format!("{} · version {} · {}", step.address, short(&v.retained_version), verdict_label(v.verdict)),
        )),
        Box::new(
            el(
                "p",
                format!(
                    "Matched {} · missing {} · coverage {}%",
                    list(&v.matched_terms),
                    list(&v.missing_terms),
                    f64::from(v.coverage_permille) / 10.0
                ),
            )
            .attr("class", "goal-meta"),
        ),
    ];
    children.extend(visit_note(v));
    children.extend(v.passages.iter().map(passage_line));
    if v.more_passages > 0 {
        children.push(Box::new(el("p", format!("{} more passages not shown.", v.more_passages)).attr("class", "goal-meta")));
    }
    Some(Box::new(el("li", children).attr("class", "goal-cite")))
}

fn list(terms: &[String]) -> String {
    if terms.is_empty() { "none".into() } else { terms.join(", ") }
}

fn explained(step: &StepView) -> Child {
    let (outcome, class) = match &step.outcome {
        Outcome::Visited(v) => (
            format!(
                "Read version {} ({} bytes): {}; matched {}, missing {}.",
                short(&v.retained_version),
                v.bytes,
                verdict_label(v.verdict),
                list(&v.matched_terms),
                list(&v.missing_terms)
            ),
            "goal-step",
        ),
        Outcome::Failed(why) => (format!("Not read. {why}"), "goal-step goal-step-failed"),
        Outcome::Unrecorded => ("The walk ended before this lookup.".into(), "goal-step"),
    };
    let depth = step.depth.map_or(String::new(), |d| format!(" · depth {d}"));
    let by = if model_chose(step) { " [model]" } else { "" };
    let mut lines: Vec<Child> = vec![
        Box::new(el("p", format!("{}{depth}", heading(step)))),
        Box::new(el("p", format!("Chosen{by}: {}", step.reason))),
        Box::new(el("p", outcome)),
    ];
    if let Outcome::Visited(v) = &step.outcome {
        lines.extend(visit_note(v));
    }
    Box::new(el("li", lines).attr("class", class))
}

fn results(run: &GoalRunView) -> Child {
    let hits: Vec<Child> = run.matches().filter_map(cited).collect();
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Cited results")),
        Box::new(
            el(
                "p",
                format!(
                    "{} of {} sources read matched \"{}\" (terms: {}). {}",
                    run.matched,
                    run.steps.iter().filter(|s| matches!(s.outcome, Outcome::Visited(_))).count(),
                    run.goal,
                    run.terms.join(", "),
                    run.stop_reason
                ),
            )
            .attr("id", "goal-summary")
            .attr("role", "status"),
        ),
    ];
    if hits.is_empty() {
        children.push(Box::new(el("p", "No retained source met the goal.")));
    } else {
        children.push(Box::new(el("ol", hits).attr("id", "goal-hits").attr("aria-label", "Cited sources")));
    }
    children.push(Box::new(el("h2", "How the walk chose")));
    children.push(Box::new(
        el("ol", run.steps.iter().map(explained).collect::<Vec<_>>())
            .attr("id", "goal-steps")
            .attr("aria-label", "Traversal steps"),
    ));
    let b = run.budgets;
    let mut meta = format!(
        "Budgets: visits {}, depth {}, lookups {}/{}, bytes {}/{}. Matches need {}% of terms; {} matches end the walk. Lexical rule {} ({}).",
        b.max_nodes, b.max_depth, run.requests, b.max_requests, run.bytes, b.max_bytes,
        f64::from(run.min_coverage_permille) / 10.0, run.satisfy_after, run.operation, run.tokenizer
    );
    if let Some(left_out) = run.omission_summary() {
        meta.push(' ');
        meta.push_str(&left_out);
    }
    if let Some(model) = run_summary(run) {
        meta.push(' ');
        meta.push_str(&model);
    }
    children.push(Box::new(el("p", meta).attr("class", "goal-meta").attr("id", "goal-meta")));
    Box::new(el("section", children).attr("id", "goal-results"))
}

fn starts(app: &GoalSurface) -> Child {
    let mut buttons: Vec<Child> = vec![Box::new(
        button("All retained pages", |s: &mut GoalSurface, _| s.start = Start::All)
            .attr("id", "goal-start-all")
            .attr("aria-pressed", (app.start == Start::All).to_string()),
    )];
    let all = app.pages().sources();
    let hidden = all.len().saturating_sub(MAX_STARTS);
    for (i, (_, page)) in all.into_iter().enumerate().take(MAX_STARTS) {
        let label = page.title.clone().unwrap_or_else(|| page.url.clone());
        buttons.push(Box::new(
            button(label.clone(), move |s: &mut GoalSurface, _| s.start = Start::Source(i))
                .attr("id", format!("goal-start-{i}"))
                .attr("aria-pressed", (app.start == Start::Source(i)).to_string()),
        ));
    }
    if hidden > 0 {
        buttons.push(Box::new(el("span", format!("{hidden} more pages are searched but not listed here.")).attr("class", "goal-meta")));
    }
    Box::new(el("div", buttons).attr("class", "goal-starts").attr("aria-label", "Start from"))
}

/// The surface: goal, limits, start, Find, then the last run's results.
pub fn view(app: &GoalSurface) -> Child {
    let intro = app.intro.clone().unwrap_or_else(|| {
        format!(
            "Reads {} retained pages and the links between them. Nothing is fetched and the graph is not changed.",
            app.pages().sources().len()
        )
    });
    let mut children: Vec<Child> = Vec::new();
    if app.heading {
        children.push(Box::new(el("h1", "Find in what I kept")));
    }
    children.push(Box::new(el("p", intro).attr("class", "goal-meta")));
    children.extend::<Vec<Child>>(vec![
        field("Goal", "goal-input", "goal-input", |s| &mut s.goal),
        field("Visits", "goal-visits", "goal-input goal-small", |s| &mut s.max_visits),
        field("Depth", "goal-depth", "goal-input goal-small", |s| &mut s.max_depth),
        starts(app),
    ]);
    #[cfg(feature = "goal-surface-model")]
    children.push(super::model_controls::controls(app));
    children.extend::<Vec<Child>>(vec![
        Box::new(button("Find", |s: &mut GoalSurface, _| s.run()).attr("id", "goal-run")),
    ]);
    if let Some(notice) = &app.notice {
        children.push(Box::new(
            el("p", notice.clone()).attr("id", "goal-notice").attr("class", "goal-notice").attr("role", "alert"),
        ));
    }
    if let Some(run) = &app.run {
        children.push(results(run));
    }
    Box::new(el("div", children).attr("class", "goal-surface"))
}
