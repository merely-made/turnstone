// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Saved node rules and the current-selection attribute lane.
use super::*;
use cambium::{DetailRow, DetailSection, button, detail_panel};
use graphshell::node_presentation::{MAX_LIVE_NODES, NodeRuntime, Representation};
use graphshell::presentation_binding::{BoundContext, Occurrence, ViewInputs, bind};
use graphshell::presentation_editor::presentation::{Effect, PrimitiveKind, RuleSet};
use mere::canvas::Role;
#[derive(Default)]
pub(super) struct Pane {
    runtime: NodeRuntime,
    epoch: Option<u64>,
    running: bool,
    error: Option<String>,
}
pub(super) enum Input {
    Clear,
    Error(String),
    Evaluate {
        epoch: u64,
        generation: u64,
        contexts: Vec<BoundContext>,
        rules: RuleSet,
    },
}
fn error(page: &TreePage, message: String) -> Option<Input> {
    (page.node_presentation.error.as_ref() != Some(&message) || page.node_presentation.running)
        .then_some(Input::Error(message))
}
pub(super) fn refresh(page: &TreePage) -> Option<Input> {
    let pane = &page.node_presentation;
    let saved = page.presentation.saved_rules();
    let enabled = page.presentation.live_enabled()
        && saved.is_some_and(|(rules, _)| {
            rules
                .rules
                .iter()
                .any(|rule| rule.enabled && rule.target.kind == PrimitiveKind::Node)
        });
    if !enabled {
        return (pane.running || pane.error.is_some()).then_some(Input::Clear);
    }
    let (rules, generation) = saved.unwrap();
    let graph = match field_inspection::graph(page) {
        Ok(graph) => graph,
        Err(message) => return error(page, message),
    };
    let canvas = page.shared.canvas.borrow();
    let members: Vec<_> = canvas
        .graph()
        .nodes()
        .filter(|(key, _)| canvas.node_in_scope(*key))
        .take(MAX_LIVE_NODES + 1)
        .map(|(key, node)| (key, node.id))
        .collect();
    if members.len() > MAX_LIVE_NODES {
        return error(
            page,
            "Live node rules need at most 128 visible accesses; base faces restored".into(),
        );
    }
    let selected = canvas.selected_members();
    let mut contexts = Vec::with_capacity(members.len());
    for (key, member) in members {
        let mut view = ViewInputs {
            selected: Some(selected.contains(&member)),
            position_pinned: Some(canvas.arrangement_role_of(key) == Role::Pinned),
            inspected: page.inspected_access == Some(member),
            screen_size: canvas.node_presentation_screen_size(member),
            ..Default::default()
        };
        view.set_attention(
            canvas.member_attention(member),
            canvas.member_foreground_pinned(member),
        );
        match bind(graph, &Occurrence::Access { member }, &view, None) {
            Ok(context) => contexts.push(context),
            Err(message) => return error(page, message),
        }
    }
    contexts.sort_by(|a, b| a.context.id.cmp(&b.context.id));
    let epoch = canvas.graph_view_epoch();
    if pane.running
        && pane
            .runtime
            .same_inputs(epoch, generation, &contexts, rules)
    {
        None
    } else {
        Some(Input::Evaluate {
            epoch,
            generation,
            contexts,
            rules: rules.clone(),
        })
    }
}
pub(super) fn update(page: &mut TreePage, input: Input) {
    let pane = &mut page.node_presentation;
    match input {
        Input::Clear => *pane = Pane::default(),
        Input::Error(message) => {
            pane.runtime.clear();
            pane.running = false;
            pane.epoch = None;
            pane.error = Some(message);
        },
        Input::Evaluate {
            epoch,
            generation,
            contexts,
            rules,
        } => match pane.runtime.evaluate(epoch, generation, &contexts, &rules) {
            Ok(_) => {
                pane.running = true;
                pane.error = None;
                pane.epoch = Some(epoch);
            },
            Err(message) => {
                pane.running = false;
                pane.epoch = None;
                pane.error = Some(message);
            },
        },
    }
}
pub(super) fn install(page: &TreePage) {
    let pane = &page.node_presentation;
    let mut canvas = page.shared.canvas.borrow_mut();
    let epoch = canvas.graph_view_epoch();
    let faces = if pane.running && pane.epoch == Some(epoch) {
        pane.runtime
            .decisions()
            .iter()
            .filter_map(|node| match &node.face {
                Some(Effect::Representation { value }) => Some((node.member, value.clone())),
                _ => None,
            })
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    match canvas.set_node_rule_faces(epoch, faces) {
        Ok(true) => page.shared.dirty.set(true),
        Err(message) => web_sys::console::error_1(&message.into()),
        _ => {},
    }
}
fn count(page: &TreePage, face: Representation) -> usize {
    page.node_presentation
        .runtime
        .decisions()
        .iter()
        .filter(|node| matches!(&node.face,Some(Effect::Representation { value }) if *value==face))
        .count()
}
pub(super) fn section(page: &TreePage) -> Child {
    let pane = &page.node_presentation;
    if !pane.running && pane.error.is_none() {
        return Box::new(el("div", ()));
    }
    let mut children: Vec<Child> = vec![Box::new(el("h2", "Live node presentation"))];
    if let Some(error) = &pane.error {
        children.push(Box::new(el("p", error.clone()).attr("role", "status")));
    }
    if pane.running {
        children.push(Box::new(el(
            "p",
            format!(
                "{} accesses · {} glyph faces · {} ordinary Canvas faces",
                pane.runtime.decisions().len(),
                count(page, Representation::Glyph),
                count(page, Representation::Card)
            ),
        )));
        children.push(Box::new(el("p","Glyph/Card change the existing body’s face treatment. Held attributes appear here for selected accesses. Complex attributes and tags remain in the source inspector. Content snapshots and live panes need their renderer adapters.")));
        for node in pane
            .runtime
            .decisions()
            .iter()
            .filter(|node| node.attributes)
        {
            let rows = node
                .bound
                .context
                .facts
                .iter()
                .map(|(key, value)| DetailRow::new(key, serde_json::to_string(value).unwrap()))
                .collect();
            children.push(Box::new(detail_panel(&[DetailSection::new(
                format!("Selected attributes · {}", node.member),
                rows,
            )])));
            let member = node.member;
            children.push(Box::new(button(
                "Inspect selected source attributes",
                move |page: &mut TreePage, _| {
                    page.inspected_access = Some(member);
                    page.tools_open = true;
                },
            )));
            children.push(Box::new(button(
                "Preview selected presentation",
                move |page: &mut TreePage, _| {
                    presentation::choose(page, Occurrence::Access { member })
                },
            )));
        }
        for node in pane
            .runtime
            .decisions()
            .iter()
            .filter(|node| !node.unsupported.is_empty())
            .take(8)
        {
            children.push(Box::new(el(
                "p",
                format!("{} · {}", node.member, node.unsupported.join("; ")),
            )));
        }
        let extra = pane
            .runtime
            .decisions()
            .iter()
            .filter(|node| !node.unsupported.is_empty())
            .count()
            .saturating_sub(8);
        if extra > 0 {
            children.push(Box::new(el("p",format!("{extra} more accesses have unsupported requests; inspect their exact preview contexts."))));
        }
    }
    Box::new(
        el("section", children)
            .attr("class", "tools-section tree-inspection")
            .attr("aria-label", "Live node presentation"),
    )
}
pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.node_presentation;
    snapshot
        .with_field("node-rules-ready", pane.running.to_string())
        .with_field(
            "node-rules-count",
            pane.runtime.decisions().len().to_string(),
        )
        .with_field(
            "node-rules-glyphs",
            count(page, Representation::Glyph).to_string(),
        )
        .with_field(
            "node-rules-cards",
            count(page, Representation::Card).to_string(),
        )
        .with_field(
            "node-rules-attributes",
            pane.runtime
                .decisions()
                .iter()
                .filter(|node| node.attributes)
                .count()
                .to_string(),
        )
        .with_field(
            "node-rules-unsupported",
            pane.runtime
                .decisions()
                .iter()
                .map(|node| node.unsupported.len())
                .sum::<usize>()
                .to_string(),
        )
        .with_field("node-rules-error", pane.error.clone().unwrap_or_default())
}
