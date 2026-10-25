// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A disclosure over the current document's canonical graph relationships.
use super::*;
use cambium::{ShellControl, shell_control, shell_navigation_item, shell_paged_list};
use mere::navigation::{
    NO_RELATIONSHIPS, RELATIONSHIPS_NOUN, RELATIONSHIPS_PAGE_SIZE, RelationshipRow,
    relationship_detail, relationships,
};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    source: Option<(Uuid, Uuid)>,
    start: usize,
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
            page.instrument.navigation.start = 0;
            page.instrument.panes_open = false;
            page.shared.dirty.set(true);
        },
    )
}

pub(super) fn popup(page: &TreePage) -> Option<Child> {
    if !page.instrument.navigation.open {
        return None;
    }
    let same = page.instrument.navigation.source == super::source_binding::active(page);
    let all = if same { rows(page) } else { Vec::new() };
    let mut children: Vec<Child> = vec![Box::new(
        el("p", "Document connections").attr("class", "workspace-related-heading"),
    )];
    if same {
        children.extend(shell_paged_list(
            RELATIONSHIPS_NOUN,
            NO_RELATIONSHIPS,
            &all,
            page.instrument.navigation.start,
            RELATIONSHIPS_PAGE_SIZE,
            |row| {
                let target = row.target;
                shell_navigation_item(
                    row.title.clone(),
                    relationship_detail(row),
                    ShellControl::default(),
                    move |page: &mut TreePage, _| { super::focus_card::related(page,target); },
                )
            },
            |page: &mut TreePage, start| {
                page.instrument.navigation.start = start;
                page.shared.dirty.set(true);
            },
        ));
    } else if !same {
        children.push(Box::new(
            status_note(
                "",
                "The current document changed. Reopen Related to browse its connections.",
            ),
        ));
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
    super::focus_card::fields(page,snapshot)
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
