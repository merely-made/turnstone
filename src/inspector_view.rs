// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Inspector pane's data half: focused-object detail sections off app
//! truth (rung 5 slice D, last pane; turnstone's outright per the pane
//! taxonomy — fetch/content introspection is app-runtime truth, so there is
//! no mere crate and none is wanted). The behavioral donor is meerkat's
//! `inspector.rs`, re-cut for turnstone's shape: node facts come from the
//! kernel node, content facts from the [`ContentFacts`] mirror the content
//! port stamps at spawn (the structural read arrives through genet's
//! `DocumentSession::inspect`, the trait accessor this pane pulled into
//! existence).
//!
//! Pure functions of `App`; the view half ([`crate::inspector_pane`])
//! renders these sections on cambium's `detail_panel`, and the observation
//! snapshot flattens them so a scenario can assert a row.
//!
//! Not carried from meerkat (deliberately): the re-parse of the fetched
//! body through nematic engines — turnstone's structural read comes from the
//! LIVE session, not a second parse (one truth, no drift). The sidecar rows
//! (viewer override, compat mode) joined at rung 6 with the browser-state
//! sidecar itself.

use crate::app::App;
use crate::content::NodeContent;

/// One section: a header and its key/value rows. The shape cambium's
/// `DetailSection` takes, kept as plain data here so this module stays
/// toolkit-free (the P8 pattern: explicit inputs in, neutral items out).
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorSection {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

/// The Inspector's sections for the current app state.
pub fn inspector_sections(app: &App) -> Vec<InspectorSection> {
    let pane = app
        .focused_graph_pane()
        .unwrap_or_else(|| app.default_graph_pane());
    inspector_sections_for_pane(app, pane)
}

/// Build the inspector from the pane's followed context. The Inspector is a
/// follower: it does not read a process-wide canvas selection.
pub fn inspector_sections_for_pane(app: &App, pane: crate::panes::PaneId) -> Vec<InspectorSection> {
    let context = app.follower_context(pane);
    let graph_id = context
        .and_then(|context| context.graph)
        .or_else(|| app.graph_for_pane(pane));
    let canvas = graph_id.and_then(|graph| app.graph_runtimes.canvas(graph));
    let member = inspector_member(app, pane);
    inspector_sections_for_context(app, canvas, member)
}

/// The member this Inspector follows, shared by readouts and editable controls.
pub(crate) fn inspector_member(app: &App, pane: crate::panes::PaneId) -> Option<uuid::Uuid> {
    if app.has_pane_binding(pane) {
        app.pane_bound_context(pane)
            .and_then(|context| context.member)
    } else {
        app.follower_context(pane)
            .and_then(|context| context.member)
            .or_else(|| app.graph_pane_focused_member(pane))
    }
}

fn inspector_sections_for_context(
    app: &App,
    canvas: Option<&mere::canvas::Canvas>,
    member: Option<uuid::Uuid>,
) -> Vec<InspectorSection> {
    let graph = canvas.map(mere::canvas::Canvas::graph);
    let focused = member.and_then(|member| graph.and_then(|graph| graph.get_node_by_id(member)));

    let node_rows = match focused {
        Some((key, node)) => vec![
            (
                "Title".to_string(),
                display_title(node.title.trim(), node.url()),
            ),
            ("URL".to_string(), node.url().to_string()),
            ("Node id".to_string(), node.id.to_string()),
            ("Addresses".to_string(), node.addresses.len().to_string()),
            (
                "Map position pinned".to_string(),
                yes_no(
                    graph
                        .and_then(|graph| graph.node_is_pinned(key))
                        .unwrap_or_default(),
                ),
            ),
            (
                "Tags".to_string(),
                graph
                    .map(|graph| {
                        let tags = crate::content_tags::content_tags(graph, key);
                        summarize_strings(tags.iter().map(String::as_str))
                    })
                    .unwrap_or_else(|| "none".to_string()),
            ),
            (
                "Import provenance".to_string(),
                graph
                    .map(|graph| summarize_import_provenance(graph, key))
                    .unwrap_or_else(|| "none".to_string()),
            ),
            (
                "Classifications".to_string(),
                graph
                    .map(|graph| summarize_classifications(graph, key))
                    .unwrap_or_else(|| "none".to_string()),
            ),
            (
                "Mime hint".to_string(),
                node.media_type.as_deref().unwrap_or("none").to_string(),
            ),
            // The sidecar rows (rung 6): the browser's handling of the node.
            (
                "Viewer".to_string(),
                app.browser
                    .get(node.id)
                    .and_then(|b| b.viewer_override.as_deref())
                    .unwrap_or("auto")
                    .to_string(),
            ),
            (
                "Compat".to_string(),
                yes_no(app.browser.get(node.id).is_some_and(|b| b.compat_mode)),
            ),
        ],
        None => vec![("Focused node".to_string(), "none".to_string())],
    };

    let content_rows = content_rows(app, focused.map(|(_, node)| node.id));

    let mut sections = vec![
        InspectorSection {
            title: "Node".to_string(),
            rows: node_rows,
        },
        InspectorSection {
            title: "Content".to_string(),
            rows: content_rows,
        },
    ];
    if let Some(member) = focused.map(|(_, node)| node.id)
        && let Some(rows) = feed_rows(app, member)
    {
        sections.push(InspectorSection {
            title: "Feed".to_string(),
            rows,
        });
    }
    let member = focused.map(|(_, node)| node.id);
    sections.push(InspectorSection {
        title: "Source capture".into(),
        rows: vec![
            ("Source save".into(), app.source_capture_status(member)),
            (
                "Saved captures".into(),
                member
                    .and_then(|member| {
                        app.graph_runtimes
                            .graph_containing_member(member)
                            .and_then(|graph| app.graph_runtimes.get(graph))
                            .map(|runtime| {
                                crate::source_capture::references(runtime.canvas.facets(), member)
                                    .len()
                            })
                    })
                    .unwrap_or(0)
                    .to_string(),
            ),
            (
                "Kept".into(),
                yes_no(member.is_some_and(|member| app.node_is_kept(member))),
            ),
        ],
    });
    sections.push(InspectorSection {
        title: "Journal".to_string(),
        rows: journal_rows(app),
    });
    sections
}

fn feed_rows(app: &App, member: uuid::Uuid) -> Option<Vec<(String, String)>> {
    match app.feeds.member_info(member)? {
        crate::feed::FeedMemberInfo::Source {
            period,
            last_checked_ms,
            last_error,
            unread,
        } => Some(vec![
            ("Refresh".to_string(), format!("every {period}")),
            (
                "Last checked".to_string(),
                last_checked_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "never".to_string()),
            ),
            (
                "Status".to_string(),
                last_error.unwrap_or_else(|| "current".to_string()),
            ),
            ("Unread".to_string(), unread.to_string()),
        ]),
        crate::feed::FeedMemberInfo::Entry {
            source_url,
            date,
            unread,
        } => Some(vec![
            ("Source".to_string(), source_url),
            (
                "Published".to_string(),
                date.unwrap_or_else(|| "unknown".to_string()),
            ),
            (
                "Status".to_string(),
                if unread { "unread" } else { "read" }.to_string(),
            ),
        ]),
    }
}

/// The attributed edit spine's tail, newest first (participant gate B1: WHO
/// changed the graph, readable). A script's id renders as its resident's
/// label while kind/version/route remain visible; only the trusted UI person
/// renders as `you`. An engine or script named `user` is still a machine.
fn journal_rows(app: &App) -> Vec<(String, String)> {
    let Some(origin) = app.behavior_origin() else {
        return Vec::new();
    };
    let Ok(journal) = app.journal.lock() else {
        return Vec::new();
    };
    journal
        .entries()
        .iter()
        .rev()
        .filter(|entry| entry.origin == origin)
        .take(5)
        .map(|entry| {
            let author = journal_author_label(app, &entry.author);
            let debug = format!("{:?}", entry.delta);
            let kind = debug
                .split(|c: char| c == ' ' || c == '{' || c == '(')
                .next()
                .unwrap_or("edit")
                .to_string();
            (author, kind)
        })
        .collect()
}

fn journal_author_label(app: &App, author: &mere::kernel::graph::Author) -> String {
    use mere::kernel::graph::{AuthorKind, USER_AUTHOR};
    if author.kind == AuthorKind::Person && author.id == USER_AUTHOR {
        return "you".to_string();
    }
    let mut display = author.clone();
    if author.kind == AuthorKind::Script {
        if let Some(resident) = app
            .denizens
            .residents
            .values()
            .find(|resident| resident.subject.to_hex() == author.id)
        {
            display.id = resident.label.clone();
        }
    }
    display.to_string()
}

/// The sections flattened to "Key: value" lines (the observation snapshot's
/// rendering; `assert row` matches against these).
pub fn inspector_lines(app: &App) -> Vec<String> {
    inspector_sections(app)
        .into_iter()
        .flat_map(|s| s.rows)
        .map(|(k, v)| format!("{k}: {v}"))
        .collect()
}

fn content_rows(app: &App, node: Option<uuid::Uuid>) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let state = node.and_then(|n| app.content.get(n));
    rows.push((
        "Content state".to_string(),
        match state {
            Some(NodeContent::Requested) => "requested".to_string(),
            Some(NodeContent::Live) => "live".to_string(),
            Some(NodeContent::AwaitingInput) => "awaiting input".to_string(),
            Some(NodeContent::AwaitingIdentity) => "awaiting client identity".to_string(),
            Some(NodeContent::AwaitingTrust) => "awaiting certificate decision".to_string(),
            Some(NodeContent::Failed(err)) => format!("failed: {}", truncate(err, 120)),
            None => "none".to_string(),
        },
    ));
    let shared = node.and_then(|n| app.content.cue_report(n)).is_some();
    if shared {
        rows.extend(
            crate::inspector_controls::capabilities(app, node)
                .into_iter()
                .filter_map(|row| {
                    row.split_once(": ")
                        .map(|(key, value)| (key.to_owned(), value.to_owned()))
                }),
        );
    }
    let Some(facts) = node.and_then(|n| app.content.facts(n)) else {
        return rows;
    };
    if !shared {
        rows.push(("Engine".to_string(), facts.engine.clone()));
        rows.extend([
            (
                "Find in page".to_string(),
                facts.capabilities.find_in_page.describe(),
            ),
            (
                "Page zoom".to_string(),
                facts.capabilities.page_zoom.describe(),
            ),
            // The REQUESTED scale, which is what the sidecar holds. A retained
            // session answers with the level it actually settled on, and that
            // rides beside the request; a hosted surface has no read-back, so the
            // row stays request-only rather than echoing the ask as the applied.
            (
                "Requested page zoom".to_string(),
                crate::app::page_zoom_display(
                    node.and_then(|n| app.browser.get(n))
                        .and_then(|state| state.page_scale),
                    node.and_then(|n| app.content.page_zoom(n))
                        .map(|zoom| zoom.applied),
                ),
            ),
            (
                "Page capture".to_string(),
                facts.capabilities.page_capture.describe(),
            ),
            (
                "Navigation controls".to_string(),
                facts.capabilities.navigation.describe(),
            ),
        ]);
    }
    if let Some(lineage) = &facts.lineage {
        rows.push((
            "Extraction lineage".to_string(),
            format!(
                "{} {} · {}{} · {} blocks",
                lineage.tool,
                lineage.version,
                lineage.selector,
                lineage
                    .score
                    .map(|score| format!(" score {score}"))
                    .unwrap_or_default(),
                lineage.block_count
            ),
        ));
    }
    match &facts.structure {
        Some(s) => {
            rows.push((
                "Document title".to_string(),
                s.title.as_deref().unwrap_or("none").to_string(),
            ));
            rows.push((
                "Document structure".to_string(),
                format!("headings={} outline={}", s.headings, s.outline.len()),
            ));
            rows.push(("Outgoing links".to_string(), s.links.to_string()));
        },
        // The lane has no structural read: said, not synthesized.
        None => rows.push((
            "Structural read".to_string(),
            "none for this lane".to_string(),
        )),
    }
    rows
}

fn display_title(title: &str, url: &str) -> String {
    if title.is_empty() {
        url.to_string()
    } else {
        title.to_string()
    }
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}

/// Up to four values, sorted, "+N" for the rest (the donor's shape).
fn summarize_strings<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let mut values: Vec<_> = values.filter(|v| !v.trim().is_empty()).collect();
    values.sort_unstable();
    if values.is_empty() {
        return "none".to_string();
    }
    let shown = values
        .iter()
        .take(4)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > 4 {
        format!("{shown}, +{}", values.len() - 4)
    } else {
        shown
    }
}

fn summarize_import_provenance(
    graph: &mere::kernel::graph::Graph,
    key: mere::kernel::graph::NodeKey,
) -> String {
    let provenance = graph.node_import_provenance(key).unwrap_or_default();
    if provenance.is_empty() {
        return "none".to_string();
    }
    summarize_strings(provenance.iter().map(|p| {
        if p.source_label.is_empty() {
            p.source_id.as_str()
        } else {
            p.source_label.as_str()
        }
    }))
}

fn summarize_classifications(
    graph: &mere::kernel::graph::Graph,
    key: mere::kernel::graph::NodeKey,
) -> String {
    let classifications = graph.node_classifications(key).unwrap_or_default();
    if classifications.is_empty() {
        return "none".to_string();
    }
    let labels = classifications
        .iter()
        .map(|c| c.label.as_deref().unwrap_or(c.value.as_str()));
    format!("{} ({})", classifications.len(), summarize_strings(labels))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
#[path = "inspector_view/tests.rs"]
mod tests;
