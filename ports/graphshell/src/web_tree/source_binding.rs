// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The small browser shell has Map, Read and one Inspector publisher binding.
use super::*;
use graphshell::pane_source::InspectorSource;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Context {
    pub owner: Uuid,
    forme: Uuid,
    pub map: Option<Uuid>,
    pub reading: Option<Uuid>,
    pub read_active: bool,
    pub rule: InspectorSource,
}

pub(super) fn context(page: &TreePage) -> Option<Context> {
    let pane = page.forme.as_ref()?;
    let workspace = pane.current();
    let (_, owner) = page.product.as_ref()?.forme_source()?;
    if owner != workspace.document.graph_id {
        return None;
    }
    Some(Context {
        owner,
        forme: workspace.document.id.as_uuid(),
        map: page
            .shared
            .canvas
            .borrow()
            .selected_members()
            .first()
            .copied(),
        reading: workspace
            .preferences
            .focused_reading
            .filter(|id| workspace.members().contains(id)),
        read_active: pane.workbench,
        rule: workspace.preferences.inspector_source,
    })
}

pub(super) fn active(page: &TreePage) -> Option<(Uuid, Uuid)> {
    let ctx = context(page)?;
    let member = InspectorSource::Active.resolve(
        page.shared.canvas.borrow().graph(),
        ctx.map,
        ctx.reading,
        ctx.read_active,
    )?;
    Some((ctx.owner, member))
}

pub(super) fn member(page: &TreePage) -> Option<Uuid> {
    let ctx = context(page)?;
    ctx.rule.resolve(
        page.shared.canvas.borrow().graph(),
        ctx.map,
        ctx.reading,
        ctx.read_active,
    )
}

pub(super) fn pin(page: &mut TreePage, member: Uuid) {
    if context(page).is_none()
        || page
            .shared
            .canvas
            .borrow()
            .graph()
            .get_node_by_id(member)
            .is_none()
    {
        return;
    }
    super::forme::bind_inspector(page, InspectorSource::Pinned(member));
    page.tools_open = true;
    page.instrument.binding_open = false;
}

pub(super) fn close(page: &mut TreePage) {
    super::forme::bind_inspector(page, InspectorSource::Paused);
}

pub(super) fn view(page: &TreePage) -> Option<Child> {
    if !super::instrument::enabled(page) {
        return None;
    }
    let ctx = context(page);
    let member = member(page);
    let name = member
        .and_then(|id| {
            page.shared
                .canvas
                .borrow()
                .graph()
                .get_node_by_id(id)
                .map(|(key, _)| page.shared.canvas.borrow().graph().node_display_label(key))
        })
        .unwrap_or_else(|| "Source unavailable".into());
    let current = ctx.map(|c| c.rule).unwrap_or_default();
    let busy = page.forme.as_ref().is_none_or(|p| p.saving) || ctx.is_none();
    let mut choices = Vec::new();
    let mut rules = vec![
        ("Follow active view", InspectorSource::Active),
        ("Follow Map", InspectorSource::Map),
        ("Follow Read", InspectorSource::Read),
    ];
    if let Some(member) = member {
        rules.push(("Pin this document", InspectorSource::Pinned(member)));
    }
    for (label, rule) in rules {
        choices.push(cambium::shell_control(
            label,
            label,
            cambium::ShellControl {
                pressed: Some(rule == current),
                disabled: busy,
                ..Default::default()
            },
            move |page: &mut TreePage, _| {
                if context(page) != ctx {
                    return;
                }
                super::forme::bind_inspector(page, rule);
                page.instrument.binding_open = false;
            },
        ));
    }
    Some(cambium::shell_source_binding(
        current.label(),
        name,
        member.is_some(),
        page.instrument.binding_open,
        choices,
        |page: &mut TreePage, _| {
            page.instrument.binding_open = !page.instrument.binding_open;
            page.shared.dirty.set(true);
        },
    ))
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    snapshot
        .with_field(
            "pane-source-mode",
            context(page)
                .map(|c| c.rule.label())
                .unwrap_or("Unavailable"),
        )
        .with_field(
            "pane-source-member",
            member(page).map(|m| m.to_string()).unwrap_or_default(),
        )
        .with_field("pane-source-available", member(page).is_some().to_string())
}
