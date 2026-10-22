// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A disclosure over the current document's canonical graph relationships.
use super::*;
use cambium::{ShellControl, shell_control, shell_navigation_item};
use mere::navigation::{RelationshipRow, RelationshipTarget, relationships};
use uuid::Uuid;

const PAGE_SIZE: usize = 6;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    source: Option<(Uuid, Uuid)>,
    page: usize,
}

fn rows(page: &TreePage) -> Vec<RelationshipRow> {
    let Some((owner, member)) = super::source_binding::active(page) else {
        return Vec::new();
    };
    relationships(owner, page.shared.canvas.borrow().graph(), member)
}

pub(super) fn control(page: &TreePage) -> Child {
    shell_control(
        "Related",
        "Document connections",
        ShellControl {
            expanded: Some(page.instrument.navigation.open),
            disabled: super::source_binding::active(page).is_none(),
            ..Default::default()
        },
        |page: &mut TreePage, _| {
            page.instrument.navigation.open = !page.instrument.navigation.open;
            page.instrument.navigation.source = super::source_binding::active(page);
            page.instrument.navigation.page = 0;
            page.instrument.panes_open = false;
            page.shared.dirty.set(true);
        },
    )
}

fn activate(page: &mut TreePage, target: RelationshipTarget) {
    if super::source_binding::active(page) != Some((target.graph, target.source))
        || page.forme.as_ref().is_none_or(|p| p.saving)
        || page.product.as_ref().is_none_or(|p| p.selection_locked())
        || !rows(page).iter().any(|row| row.target == target)
    {
        return;
    }
    super::forme::open_retained(page, target.member);
    if !page
        .forme
        .as_ref()
        .is_some_and(|p| p.current().members().contains(&target.member))
    {
        return;
    }
    page.shared.canvas.borrow_mut().select_member(target.member);
    if let Some(product) = &mut page.product {
        product.select(Some(target.member));
    }
    page.instrument.navigation.open = false;
    page.shared.dirty.set(true);
}

pub(super) fn popup(page: &TreePage) -> Option<Child> {
    if !page.instrument.navigation.open {
        return None;
    }
    let same = page.instrument.navigation.source == super::source_binding::active(page);
    let all = if same { rows(page) } else { Vec::new() };
    let count = all.len();
    let index = page
        .instrument
        .navigation
        .page
        .min(count.saturating_sub(1) / PAGE_SIZE);
    let mut children: Vec<Child> = vec![Box::new(
        el("p", "Document connections").attr("class", "workspace-related-heading"),
    )];
    if all.is_empty() {
        children.push(Box::new(
            status_note("", if same {
                    "No relationships recorded for this document"
                } else {
                    "The current document changed. Reopen Related to browse its connections."
                }),
        ));
    }
    for row in all.into_iter().skip(index * PAGE_SIZE).take(PAGE_SIZE) {
        children.push(shell_navigation_item(
            row.title,
            format!("{} · {}", row.relationship, row.address),
            ShellControl::default(),
            move |page: &mut TreePage, _| activate(page, row.target),
        ));
    }
    if count > PAGE_SIZE {
        children.push(Box::new(el(
            "p",
            format!(
                "{}–{} of {count}",
                index * PAGE_SIZE + 1,
                ((index + 1) * PAGE_SIZE).min(count)
            ),
        )));
        for (label, delta, disabled) in [
            ("Previous connections", -1, index == 0),
            ("More connections", 1, (index + 1) * PAGE_SIZE >= count),
        ] {
            children.push(shell_control(
                label,
                label,
                ShellControl {
                    disabled,
                    ..Default::default()
                },
                move |page: &mut TreePage, _| {
                    page.instrument.navigation.page = index.saturating_add_signed(delta);
                    page.shared.dirty.set(true);
                },
            ));
        }
    }
    Some(Box::new(
        el(
            "div",
            cambium::shell_popup("Document connections", children),
        )
        .attr("class", "workspace-related"),
    ))
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    snapshot
        .with_field("relationship-count", rows(page).len().to_string())
        .with_field(
            "relationship-open",
            page.instrument.navigation.open.to_string(),
        )
        .with_field(
            "relationship-source",
            super::source_binding::active(page)
                .map(|(_, m)| m.to_string())
                .unwrap_or_default(),
        )
}
