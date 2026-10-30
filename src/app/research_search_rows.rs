// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::{
    App,
    research_search_state::{Command, View},
};
use crate::action::Action;
use crate::research_search::Member;
use crate::ui::Suggestion;
use eidetic_search::query::{Coverage, ProducerKind, TagId};

impl App {
    pub(crate) fn research_rows(&mut self, row_limit: usize) {
        let state = &self.research_search;
        let mut rows = Vec::new();
        let mut push = |label: String, command: Command| {
            rows.push(Suggestion::Act {
                label,
                action: Action::ResearchSearch(command),
            })
        };
        if !matches!(state.view, View::Results | View::Coverage) {
            if let Some(error) = &state.error {
                push(format!("Error: {error}"), Command::Coverage);
            }
        }
        match &state.view {
            View::Results => {
                push("Search options…".into(), Command::Options);
                if let Some(error) = &state.error {
                    push(format!("Search unavailable: {error}"), Command::Coverage);
                }
                push("Coverage…".into(), Command::Coverage);
                if let Some(result) = &state.result {
                    for hit in &result.hits {
                        if let Ok(mut target) = crate::research::target(&hit.source, &hit.version) {
                            if let Some(passage) = &hit.passage {
                                target.set_selector(crate::saved_sources::Selector {
                                    start: passage.selector.start,
                                    end: passage.selector.end,
                                });
                            }
                            let excerpt = hit
                                .passage
                                .as_ref()
                                .map(|p| p.text.as_str())
                                .unwrap_or("metadata match");
                            push(
                                format!(
                                    "{:?}: {} · {}",
                                    hit.producer,
                                    hit.title.as_deref().unwrap_or("Untitled"),
                                    excerpt
                                ),
                                Command::Open {
                                    target,
                                    request: state
                                        .accepted
                                        .as_ref()
                                        .expect("result has request")
                                        .id,
                                },
                            );
                        }
                    }
                    if state.page > 0 {
                        push("Previous results".into(), Command::Page(-1));
                    }
                    if result.settings.offset.saturating_add(result.hits.len()) < result.total {
                        push("Next results".into(), Command::Page(1));
                    }
                }
            },
            View::Options => {
                for (label, command) in [
                    ("Search mode", Command::Modes),
                    ("Source kinds", Command::Types),
                    ("Sources and scope", Command::Sources),
                    ("Date bounds", Command::Dates),
                    ("Tags", Command::Tags),
                    ("Search settings", Command::Settings),
                    ("Back to results", Command::Results),
                ] {
                    push(label.into(), command);
                }
            },
            View::Modes => {
                for (mode, label) in [
                    (0, "Terms"),
                    (1, "Phrase"),
                    (2, "Address"),
                    (3, "Prefix"),
                    (4, "Browse"),
                ] {
                    push(
                        format!("{} {label}", if state.mode == mode { "✓" } else { "○" }),
                        Command::Mode(mode),
                    );
                }
                push("Back".into(), Command::Options);
            },
            View::Types => {
                push(
                    format!(
                        "{} All source types",
                        if state.query.kinds.is_empty() {
                            "✓"
                        } else {
                            "○"
                        }
                    ),
                    Command::AllKinds,
                );
                for kind in [
                    ProducerKind::Capture,
                    ProducerKind::Selection,
                    ProducerKind::Note,
                    ProducerKind::Other,
                ] {
                    push(
                        format!(
                            "{} {:?}",
                            if state.query.kinds.contains(&kind) {
                                "✓"
                            } else {
                                "○"
                            },
                            kind
                        ),
                        Command::Kind(kind),
                    );
                }
                push("Back".into(), Command::Options);
            },
            View::Sources => {
                push(
                    format!(
                        "{} All admitted",
                        if state.selected.is_none() {
                            "✓"
                        } else {
                            "○"
                        }
                    ),
                    Command::ScopeAll,
                );
                push(
                    format!(
                        "{} Choose sources",
                        if state.selected.is_some() {
                            "✓"
                        } else {
                            "○"
                        }
                    ),
                    Command::ScopeChosen,
                );
                if let Ok(context) = self.research.context() {
                    for admission in context.admissions {
                        let member = Member::Capture(admission.key);
                        let on = state.selected.as_ref().is_some_and(|v| v.contains(&member));
                        push(
                            format!(
                                "{} {}{}",
                                if on { "✓" } else { "○" },
                                admission.title,
                                admission
                                    .error
                                    .map(|e| format!(" · unavailable: {e}"))
                                    .unwrap_or_default()
                            ),
                            Command::Member(member),
                        );
                    }
                    for clip in context.clips {
                        let member = Member::Clip(clip.clone());
                        let on = state.selected.as_ref().is_some_and(|v| v.contains(&member));
                        let title = self
                            .graph_runtimes
                            .canvas(clip.graph)
                            .and_then(|canvas| canvas.graph().get_node_by_id(clip.node))
                            .map(|(_, node)| node.title.as_str())
                            .filter(|title| !title.trim().is_empty())
                            .unwrap_or("Retained clip");
                        let short = clip
                            .manifest
                            .to_string()
                            .chars()
                            .take(18)
                            .collect::<String>();
                        push(
                            format!("{} {} · {}", if on { "✓" } else { "○" }, title, short),
                            Command::Member(member),
                        );
                    }
                }
                push("Back".into(), Command::Options);
            },
            View::Dates => {
                push(
                    format!("From date (UTC): {}", show_date(state.query.date_from)),
                    Command::Input("date_from".into()),
                );
                push(
                    format!("Through date (UTC): {}", show_date(state.query.date_to)),
                    Command::Input("date_to".into()),
                );
                push("Back".into(), Command::Options);
            },
            View::Tags => {
                let mut tags = Vec::<TagId>::new();
                if let Some(result) = &state.result {
                    for input in &result.inputs {
                        for tag in &input.tags {
                            if !tags.contains(tag) {
                                tags.push(tag.clone());
                            }
                        }
                    }
                }
                for tag in tags {
                    push(
                        format!(
                            "{} {} / {}",
                            if state.query.tags.contains(&tag) {
                                "✓"
                            } else {
                                "○"
                            },
                            tag.owner,
                            tag.concept
                        ),
                        Command::Tag(tag),
                    );
                }
                push("Back".into(), Command::Options);
            },
            View::Settings => {
                let settings = &state.preferences.settings;
                for (field, label, value) in [
                    ("limit", "Results per page", settings.limit.to_string()),
                    (
                        "snippet_context",
                        "Characters around each match",
                        settings.snippet_context.to_string(),
                    ),
                    (
                        "max_sources",
                        "Maximum sources",
                        settings.max_sources.to_string(),
                    ),
                    (
                        "max_corpus_chars",
                        "Maximum body characters",
                        settings.max_corpus_chars.to_string(),
                    ),
                    (
                        "max_metadata_chars",
                        "Maximum metadata characters",
                        settings.max_metadata_chars.to_string(),
                    ),
                    (
                        "max_query_chars",
                        "Maximum query characters",
                        settings.max_query_chars.to_string(),
                    ),
                    (
                        "weight_url",
                        "Address ranking weight",
                        settings.weights.url.to_string(),
                    ),
                    (
                        "weight_title",
                        "Title ranking weight",
                        settings.weights.title.to_string(),
                    ),
                    (
                        "weight_text",
                        "Body ranking weight",
                        settings.weights.text.to_string(),
                    ),
                    (
                        "aliases",
                        "Alternative terms",
                        format!("{} rules", state.preferences.aliases.len()),
                    ),
                ] {
                    push(format!("{label}: {value}"), Command::Input(field.into()));
                }
                push("Back".into(), Command::Options);
            },
            View::Coverage => {
                if let Some(result) = &state.result {
                    let scope = if state.selected.is_some() {
                        "chosen sources"
                    } else {
                        "all admitted sources"
                    };
                    let coverage = if matches!(result.coverage, Coverage::Complete) {
                        "Complete"
                    } else {
                        "Incomplete"
                    };
                    push(
                        format!(
                            "{coverage}: {scope}; {} sources, {} results; page {}",
                            result.inputs.len(),
                            result.total,
                            state.page + 1
                        ),
                        Command::Coverage,
                    );
                    push(
                        format!(
                            "Expanded terms: {}",
                            if result.resolved_terms.is_empty() {
                                "none".into()
                            } else {
                                result.resolved_terms.join(", ")
                            }
                        ),
                        Command::Coverage,
                    );
                    if let Coverage::Incomplete { exclusions } = &result.coverage {
                        for exclusion in exclusions {
                            push(format!("Unavailable: {exclusion}"), Command::Coverage);
                        }
                    }
                    for input in &result.inputs {
                        push(
                            format!(
                                "{:?}: {} · {}",
                                input.producer,
                                input.title.as_deref().unwrap_or("Untitled"),
                                input.source.source
                            ),
                            Command::Coverage,
                        );
                    }
                } else {
                    push(
                        state
                            .error
                            .clone()
                            .unwrap_or_else(|| "Search is loading".into()),
                        Command::Coverage,
                    );
                }
                push("Back".into(), Command::Results);
            },
            View::Input(field) => {
                push(
                    match field.as_str() {
                        "date_from" | "date_to" => "Enter YYYY-MM-DD (UTC); empty clears".into(),
                        "aliases" => {
                            "Enter one term = alternative, alternative per line; empty clears"
                                .into()
                        },
                        _ => format!("Enter a value for {}", field.replace('_', " ")),
                    },
                    Command::Submit(self.omnibar.text.clone()),
                );
            },
        }
        let start = state.first_row.min(rows.len().saturating_sub(1));
        self.omnibar.suggestions = rows
            .iter()
            .skip(start)
            .take(row_limit.max(1))
            .cloned()
            .collect();
        self.omnibar.selected = 0;
        self.research_search.rows = rows;
    }
}

fn show_date(value: Option<i64>) -> String {
    value
        .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .map(|ts| ts.to_zoned(jiff::tz::TimeZone::UTC).date().to_string())
        .unwrap_or_else(|| "any date".into())
}
