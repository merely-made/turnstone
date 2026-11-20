// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The native owner and guest controls, bound to the graph on screen.
use std::collections::BTreeMap;

use cambium::{TextInput, button, el, lens, text_field_typed};
use chirograph::PortableCardV1;
use graphshell_host::coop::Operation;
use mere_view::MereViewModel;
use pandect::SessionId;
use uuid::Uuid;

use super::{
    Child,
    worker::{Command, Panel},
};

pub(crate) const CSS: &str = "
.coop-panel { padding: 8px; }
.coop-panel h2, .coop-panel p { margin: 4px 0; overflow-wrap: anywhere; }
.coop-panel button { margin: 0 6px 4px 0; }
.coop-panel label { display: block; margin: 6px 0; }
.coop-input { display: block; box-sizing: border-box; width: 100%; min-height: 28px; padding: 4px; border: 1px solid #6d7783; background: var(--tabard-color-surface, #22262c); }
";

pub(crate) struct Context {
    pub(crate) attached: Option<SessionId>,
    pub(crate) focused: Option<Uuid>,
    pub(crate) busy: bool,
}

type Access<S> = fn(&S) -> Option<&Panel>;
type AccessMut<S> = fn(&mut S) -> Option<&mut Panel>;
type ReadContext<S> = fn(&S) -> Context;

fn field<S: 'static>(index: usize, label: &'static str, get: AccessMut<S>) -> Child<S> {
    Box::new(el(
        "label",
        (
            label,
            lens(
                move |input: &mut TextInput| {
                    text_field_typed(input)
                        .attr("aria-label", label)
                        .attr("class", "coop-input")
                        .attr("id", format!("coop-field-{index}"))
                },
                move |state: &mut S| &mut get(state).expect("visible co-op panel").inputs[index],
            ),
        ),
    ))
}

fn submit_edit<S>(
    state: &mut S,
    get: AccessMut<S>,
    context: ReadContext<S>,
    expected: SessionId,
    rename: bool,
) {
    let current = context(state);
    let Some(panel) = get(state) else {
        return;
    };
    let Some(data) = panel.update.data.as_ref() else {
        return;
    };
    if current.busy
        || panel.busy()
        || expected != data.graph
        || (!panel.guest && current.attached != Some(expected))
        || (panel.guest && data.access != "editor")
    {
        return;
    }
    let title = panel.inputs[1].text().trim().to_string();
    let operation = if rename {
        let Some(id) = current.focused else {
            panel.update.status = "Select a graph node first".into();
            return;
        };
        Operation::Rename { id, title }
    } else {
        Operation::Add {
            id: Uuid::new_v4(),
            address: panel.inputs[0].text().trim().into(),
            title,
        }
    };
    panel.send(Command::Edit(data.edit(operation)));
}

pub(crate) fn view<S: 'static>(
    state: &S,
    read: Access<S>,
    get: AccessMut<S>,
    context: ReadContext<S>,
) -> Child<S> {
    let Some(panel) = read(state) else {
        return Box::new(el("span", ()));
    };
    let current = context(state);
    let mut children: Vec<Child<S>> = vec![
        Box::new(el(
            "h2",
            if panel.guest {
                "Invited graph"
            } else {
                "Co-op"
            },
        )),
        Box::new(
            el("p", panel.update.status.clone())
                .attr("id", "coop-status")
                .attr("role", "status"),
        ),
    ];
    if !panel.guest {
        let attached = current.attached;
        children.push(Box::new(
            button("Use this graph for co-op", move |state: &mut S, _| {
                let now = context(state);
                if let Some(graph) = attached
                    && now.attached == attached
                    && !now.busy
                    && let Some(panel) = get(state)
                    && !panel.busy()
                {
                    panel.send(Command::Read(graph));
                }
            })
            .attr("id", "coop-use"),
        ));
    }
    if let Some(data) = &panel.update.data
        && (panel.guest || Some(data.graph) == current.attached)
    {
        let graph = data.graph;
        children.push(Box::new(
            el(
                "p",
                format!(
                    "Graph {} · {} · guest {}",
                    graph.as_uuid(),
                    data.access,
                    data.guest.as_deref().unwrap_or("none")
                ),
            )
            .attr("id", "coop-scope"),
        ));
        if !panel.guest || data.access == "editor" {
            children.push(field(0, "Address", get));
            children.push(field(1, "Title", get));
            for (label, key, rename) in [
                ("Add address", "coop-add", false),
                ("Save title", "coop-title", true),
            ] {
                children.push(Box::new(
                    button(label, move |state: &mut S, _| {
                        submit_edit(state, get, context, graph, rename)
                    })
                    .attr("id", key),
                ));
            }
        }
        if !panel.guest {
            children.push(field(2, "Guest contact file", get));
            children.push(field(3, "Invitation folder", get));
            for (label, key, write, export) in [
                ("Invite editor", "coop-invite-editor", true, false),
                ("Invite reader", "coop-invite-reader", false, false),
                ("Export current invitation", "coop-export", false, true),
            ] {
                children.push(Box::new(
                    button(label, move |state: &mut S, _| {
                        let now = context(state);
                        if now.attached == Some(graph)
                            && !now.busy
                            && let Some(panel) = get(state)
                            && !panel.busy()
                            && panel
                                .update
                                .data
                                .as_ref()
                                .is_some_and(|data| data.graph == graph)
                        {
                            panel.send(Command::Invite {
                                graph,
                                guest: if export {
                                    Default::default()
                                } else {
                                    panel.inputs[2].text().into()
                                },
                                output: panel.inputs[3].text().into(),
                                write,
                            });
                        }
                    })
                    .attr("id", key),
                ));
            }
            children.push(Box::new(
                button("End guest access", move |state: &mut S, _| {
                    let current = context(state);
                    if current.attached == Some(graph)
                        && !current.busy
                        && let Some(panel) = get(state)
                        && !panel.busy()
                        && panel
                            .update
                            .data
                            .as_ref()
                            .is_some_and(|data| data.graph == graph)
                    {
                        panel.send(Command::Revoke(graph));
                    }
                })
                .attr("id", "coop-revoke"),
            ));
        }
        let authors = data
            .authors
            .iter()
            .rev()
            .take(3)
            .map(|a| format!("{} via {}", a.id, a.via.as_deref().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(" · ");
        if !authors.is_empty() {
            children.push(Box::new(
                el("p", format!("Saved activity · {authors}")).attr("id", "coop-authors"),
            ));
        }
    }
    if panel.guest {
        children.push(Box::new(
            button("Reconnect", move |state: &mut S, _| {
                if let Some(panel) = get(state)
                    && !panel.busy()
                {
                    panel.send(Command::Reconnect);
                }
            })
            .attr("id", "coop-reconnect"),
        ));
    }
    Box::new(
        el("section", children)
            .attr("class", "coop-panel")
            .attr("aria-label", "Graph co-op"),
    )
}

/// These are the pane's displayed records and its carrier acknowledgement.
/// The test harness reads them from the same retained frame as its capture.
pub(crate) fn observe(
    panel: Option<&Panel>,
    model: &MereViewModel,
    focused: Option<&str>,
    record: Option<&PortableCardV1>,
) -> String {
    if std::env::var_os("TURNSTONE_SCENARIO").is_none() {
        return String::new();
    }
    let attached = model
        .sessions
        .iter()
        .find(|session| session.attached)
        .map(|session| session.id);
    let update = panel.map(|panel| &panel.update);
    let data = update.and_then(|update| update.data.as_ref());
    let available = data
        .is_some_and(|data| panel.is_some_and(|panel| panel.guest) || attached == Some(data.graph));
    let update = update.filter(|update| available || update.data.is_none());
    let data = data.filter(|_| available);
    let fields: BTreeMap<&str, String> = [
        ("pid", std::process::id().to_string()),
        ("status", model.status.token().to_string()),
        ("nodes", model.graph.nodes.len().to_string()),
        ("sessions", model.sessions.len().to_string()),
        (
            "archives",
            model
                .archive
                .as_ref()
                .map_or(0, |archive| archive.entries.len())
                .to_string(),
        ),
        ("relations", model.graph.relations.len().to_string()),
        (
            "graph-labels",
            model
                .graph
                .nodes
                .iter()
                .map(|node| node.label.as_str())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        (
            "attached",
            attached
                .map(|id| id.as_uuid().to_string())
                .unwrap_or_default(),
        ),
        ("selected", focused.unwrap_or_default().into()),
        (
            "record",
            record.map(|card| card.title.clone()).unwrap_or_default(),
        ),
        (
            "record-data",
            record
                .and_then(|card| serde_json::to_string(card).ok())
                .unwrap_or_default(),
        ),
        (
            "coop",
            update
                .and_then(|update| serde_json::to_string(update).ok())
                .unwrap_or_default(),
        ),
        (
            "coop-nodes",
            data.map(|data| data.nodes.len().to_string())
                .unwrap_or_default(),
        ),
        (
            "coop-access",
            data.map(|data| data.access.clone()).unwrap_or_default(),
        ),
        (
            "coop-graph",
            data.map(|data| data.graph.as_uuid().to_string())
                .unwrap_or_default(),
        ),
        ("coop-available", available.to_string()),
        (
            "coop-status",
            update
                .map(|update| update.status.clone())
                .unwrap_or_default(),
        ),
    ]
    .into();
    serde_json::to_string(&fields).expect("co-op observation fields")
}
