// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn answer_view(state: &AnswerState) -> AnswerView {
    let mut body: Vec<AnswerView> = vec![
        Box::new(el::<_, AnswerState, ()>("h2", "Answers").attr("class", "answer-heading")),
        row("Ask only about the saved sources you choose below."),
        row(format!(
            "{} saved sources chosen",
            state.source.chosen.len()
        )),
        Box::new(
            el::<_, AnswerState, ()>("div", state.snapshot.status.clone())
                .attr("class", "answer-status")
                .attr(
                    "data-answer-connection",
                    if state.snapshot.ready {
                        "ready"
                    } else {
                        "waiting"
                    },
                ),
        ),
        row("Question"),
        Box::new(lens(
            |input: &mut TextInput| {
                text_field_typed(input)
                    .attr("aria-label", "Question")
                    .attr("data-answer-question", "true")
                    .attr("class", "answer-question")
            },
            |state: &mut AnswerState| &mut state.question,
        )),
    ];
    let mut choices = vec![row("Saved sources for this question")];
    for choice in &state.snapshot.choices {
        if let Some(target) = &choice.target {
            let target = target.clone();
            let chosen = state.source.chosen.contains(&target);
            choices.push(button(
                format!(
                    "{}: {}",
                    if chosen {
                        "Remove source"
                    } else {
                        "Choose source"
                    },
                    choice.label
                ),
                move |state, _| {
                    let mut chosen = state.source.chosen.clone();
                    if chosen.contains(&target) {
                        chosen.retain(|item| item != &target);
                    } else {
                        chosen.push(target.clone());
                    }
                    state.bridge.send(crate::research::Intent::Selection {
                        context: state.context.clone(),
                        expected: state.source.clone(),
                        chosen,
                    });
                },
            ));
        } else {
            choices.push(row(format!(
                "Unavailable: {} · {}",
                choice.label,
                choice.error.as_deref().unwrap_or("Unreadable saved source")
            )));
        }
    }
    for chosen in &state.source.chosen {
        if !state
            .snapshot
            .choices
            .iter()
            .any(|choice| choice.target.as_ref() == Some(chosen))
        {
            let missing = chosen.clone();
            choices.push(button(
                format!("Remove unavailable source: {}", chosen.identity()),
                move |state, _| {
                    let chosen = state
                        .source
                        .chosen
                        .iter()
                        .filter(|target| *target != &missing)
                        .cloned()
                        .collect();
                    state.bridge.send(crate::research::Intent::Selection {
                        context: state.context.clone(),
                        expected: state.source.clone(),
                        chosen,
                    });
                },
            ));
        }
    }
    body.push(Box::new(
        el::<_, AnswerState, ()>("div", choices).attr("class", "answer-scope"),
    ));
    if !state.local_status.is_empty() {
        body.push(row(state.local_status.clone()));
    }
    let ask_button = if state.snapshot.ready {
        button("Ask", ask)
    } else {
        Box::new(el::<_, AnswerState, ()>("button", "Ask").attr("disabled", "disabled"))
            as AnswerView
    };
    body.push(Box::new(
        el::<_, AnswerState, ()>(
            "div",
            vec![
                ask_button,
                if state.snapshot.ready {
                    button("Ask with evidence", ask_with_evidence)
                } else {
                    Box::new(
                        el::<_, AnswerState, ()>("button", "Ask with evidence")
                            .attr("disabled", "disabled"),
                    ) as AnswerView
                },
                button("Refresh answers", |state, _| {
                    if let Err(e) = state.service.refresh() {
                        state.local_status = e;
                    }
                }),
            ],
        )
        .attr("class", "answer-controls"),
    ));
    if let Some(run) = selected(state) {
        let mut card: Vec<AnswerView> = vec![
            row(run.question.clone()),
            Box::new(
                el::<_, AnswerState, ()>(
                    "div",
                    format!("{} · {} tokens", state_name(run.state), run.tokens),
                )
                .attr("data-answer-state", state_name(run.state).to_lowercase())
                .attr("data-answer-tokens", run.tokens.to_string()),
            ),
            if state.show_raw
                || run
                    .recall
                    .as_ref()
                    .and_then(|r| r.support.as_ref())
                    .is_none_or(|s| s.error.is_some())
            {
                Box::new(
                    el::<_, AnswerState, ()>("pre", run.text.clone())
                        .attr("class", "answer-body")
                        .attr("data-answer-text", "true"),
                ) as AnswerView
            } else {
                row("Structured answer below; raw model output is retained.")
            },
        ];
        if let Some(error) = &run.error {
            card.push(row(error.clone()));
        }
        let mut controls = Vec::new();
        if matches!(run.state, RunStateV1::Queued | RunStateV1::Running) {
            let id = run.run.clone();
            controls.push(button("Pause", move |state, _| {
                if let Err(e) = state.service.control(id.clone(), Control::Pause) {
                    state.local_status = e;
                }
            }));
        }
        if run.state == RunStateV1::Paused {
            let id = run.run.clone();
            controls.push(button("Resume", move |state, _| {
                if let Err(e) = state.service.control(id.clone(), Control::Resume) {
                    state.local_status = e;
                }
            }));
        }
        if !run.state.is_terminal() {
            let id = run.run.clone();
            controls.push(button("Cancel", move |state, _| {
                if let Err(e) = state.service.control(id.clone(), Control::Cancel) {
                    state.local_status = e;
                }
            }));
        }
        card.push(Box::new(
            el::<_, AnswerState, ()>("div", controls).attr("class", "answer-controls"),
        ));
        if let Some(evidence) = &run.recall {
            if let Some(support) = &evidence.support {
                card.push(button(
                    if state.show_raw {
                        "Hide raw output"
                    } else {
                        "Show raw output"
                    },
                    |state, _| {
                        state.show_raw = !state.show_raw;
                    },
                ));
                card.push(row(
                    "Claim support · model interpretation; semantic support has not been reviewed.",
                ));
                if let Some(error) = &support.error {
                    card.push(row(error.clone()));
                }
                for (index, claim) in support.claims.iter().enumerate() {
                    use eidetic::models::answer::ClaimStatus;
                    let status = match claim.status {
                        ClaimStatus::Supported => "Support proposed",
                        ClaimStatus::Insufficient => "Insufficient evidence",
                        ClaimStatus::Conflicting => "Conflicting evidence",
                    };
                    let mut parts: Vec<AnswerView> =
                        vec![row(format!("{}. {status}: {}", index + 1, claim.text))];
                    if let Some(review) = &claim.review {
                        use eidetic::models::answer::CandidateAgreement;
                        let relevant = review.candidates.iter().filter(|c| c.answers).count();
                        let agreement = match review.agreement {
                            CandidateAgreement::None => "no answer found",
                            CandidateAgreement::Single => "one answer found",
                            CandidateAgreement::Same => "answers judged to agree",
                            CandidateAgreement::Different => "answers judged to disagree",
                        };
                        parts.push(row(format!(
                            "Model evidence review: {relevant} of {} supplied passages marked relevant; {agreement}.",
                            review.candidates.len()
                        )));
                    }
                    for issue in &claim.issues {
                        parts.push(row(format!("Support check: {issue}")));
                    }
                    for (link_index, link) in claim.evidence.iter().enumerate() {
                        parts.push(row(format!("Quotation [{}]: {}", link.handle, link.quote)));
                        let current = evidence
                            .citations
                            .iter()
                            .any(|c| c.handle == link.handle && Some(c.passage) == link.passage);
                        if current && link.error.is_none() && link.selector.is_some() {
                            let id = run.run.clone();
                            let handle = link.handle;
                            parts.push(button(
                                format!("Inspect claim {} source {handle}", index + 1),
                                move |state, _| {
                                    if let Err(error) = state.service.claim_source(
                                        id.clone(),
                                        handle,
                                        index,
                                        link_index,
                                    ) {
                                        state.local_status = error;
                                    }
                                },
                            ));
                            let selector = link.selector.as_ref().unwrap();
                            parts.push(row(format!(
                                "Exact source quotation · characters {}..{}",
                                selector.start, selector.end
                            )));
                        } else if link.error.is_none() {
                            parts.push(row(
                                "Source changed or is unavailable; inspection is withdrawn.",
                            ));
                        }
                    }
                    card.push(Box::new(
                        el::<_, AnswerState, ()>("section", parts)
                            .attr("data-answer-claim", (index + 1).to_string())
                            .attr("class", "answer-source"),
                    ));
                }
                card.push(row("Raw model output is available through Show raw output. Exact quotations establish text identity; they do not verify the surrounding claim."));
            }
            if evidence.citations.is_empty() && run.state == RunStateV1::Finished {
                let insufficient = evidence.support.as_ref().is_some_and(|s| {
                    s.error.is_none()
                        && !s.claims.is_empty()
                        && s.claims
                            .iter()
                            .all(|c| c.status == eidetic::models::answer::ClaimStatus::Insufficient)
                });
                card.push(row(if insufficient {
                    "The model reports insufficient evidence in this chosen scope."
                } else {
                    "No validated citations in this answer."
                }));
            }
            if !evidence.stale.is_empty() {
                card.push(row("Some retained sources changed or became unavailable. Their citations are withdrawn."));
            }
            if !evidence.unknown_citations.is_empty() {
                card.push(row("Some answer markers do not name supplied material."));
            }
            for citation in &evidence.citations {
                if let Some(supplied) = evidence
                    .passages
                    .iter()
                    .find(|p| p.handle == citation.handle && p.passage.id() == citation.passage)
                {
                    let id = run.run.clone();
                    let handle = citation.handle;
                    let basis = match citation.basis {
                        distillery::inference_protocol::recall::CitationBasis::ModelMarker => {
                            "Source marker"
                        },
                        distillery::inference_protocol::recall::CitationBasis::ClaimQuotation => {
                            "Claim quotation"
                        },
                        distillery::inference_protocol::recall::CitationBasis::VerbatimPassage => {
                            "Exact quotation"
                        },
                    };
                    card.push(button(
                        format!(
                            "Source {handle}: {}",
                            state
                                .snapshot
                                .choices
                                .iter()
                                .find(|choice| choice.target.as_ref().is_some_and(|target| {
                                    crate::research::source_id(target).ok().as_ref()
                                        == Some(&supplied.passage.source)
                                }))
                                .map(|choice| choice.label.as_str())
                                .unwrap_or("Saved source")
                        ),
                        move |state, _| {
                            if let Err(e) = state.service.source(id.clone(), handle) {
                                state.local_status = e;
                            }
                        },
                    ));
                    card.push(Box::new(
                        el::<_, AnswerState, ()>(
                            "div",
                            format!(
                                "{basis} · {} · characters {}..{}",
                                supplied
                                    .passage
                                    .version
                                    .strip_prefix("blake3:")
                                    .unwrap_or(&supplied.passage.version)
                                    .chars()
                                    .take(12)
                                    .collect::<String>(),
                                supplied.passage.selector.start,
                                supplied.passage.selector.end
                            ),
                        )
                        .attr("class", "answer-version"),
                    ));
                }
            }
        }
        body.push(Box::new(
            el::<_, AnswerState, ()>("section", card).attr("data-answer-run", run.run.clone()),
        ));
    }
    if !state.snapshot.runs.is_empty() {
        let mut history = vec![row("Past asks")];
        for run in state.snapshot.runs.iter().rev() {
            let id = run.run.clone();
            history.push(button(
                format!(
                    "Reopen: {} ({})",
                    run.question.chars().take(70).collect::<String>(),
                    state_name(run.state)
                ),
                move |state, _| {
                    state.active = Some(id.clone());
                    state.local_status = String::new();
                },
            ));
        }
        body.push(Box::new(
            el::<_, AnswerState, ()>("div", history).attr("class", "answer-history"),
        ));
    }
    Box::new(
        el::<_, AnswerState, ()>("div", body)
            .attr("class", "answer-pane")
            .attr(
                "style",
                format!("width: {}px; min-height: {}px;", state.width, state.height),
            ),
    )
}
