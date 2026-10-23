// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A disclosure over the current document's canonical graph relationships.
use super::*;
use cambium::{ShellControl, shell_control, shell_navigation_item, shell_paged_list};
use mere::navigation::{
    NO_RELATIONSHIPS, RELATIONSHIPS_NOUN, RELATIONSHIPS_PAGE_SIZE, RelationshipRow,
    RelationshipTarget, relationship_detail, relationships,
};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    source: Option<(Uuid, Uuid)>,
    start: usize,
    preview: Option<RelationshipTarget>,
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
            page.instrument.navigation.preview = None;
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
    super::forme::read_related(page, target.member);
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
    let mut children: Vec<Child> = vec![Box::new(
        el("p", "Document connections").attr("class", "workspace-related-heading"),
    )];
    if let Some(row)=page.instrument.navigation.preview.and_then(|target|all.iter().find(|row|row.target==target)) {
        let target=row.target;
        let excerpt=super::reading::preview_excerpt(page,target.member);
        children.push(cambium::relationship_preview(row.title.clone(),row.relationship.clone(),row.address.clone(),excerpt,move |page:&mut TreePage,command| {
            use cambium::RelationshipPreviewCommand as C;
            if !rows(page).iter().any(|row|row.target==target) {return;}
            match command {C::Read=>activate(page,target), C::ShowGraph=> {super::reading::show_graph(page,target.member);page.instrument.navigation.open=false;}, C::Close=>page.instrument.navigation.preview=None}
            page.shared.dirty.set(true);
        }));
    }
    if same && page.instrument.navigation.preview.is_none() {
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
                    move |page: &mut TreePage, _| { page.instrument.navigation.preview = Some(target); page.shared.dirty.set(true); },
                )
            },
            |page: &mut TreePage, start| {
                page.instrument.navigation.start = start;
                page.shared.dirty.set(true);
            },
        ));
    } else {
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
    snapshot
        .with_field("relationship-preview",page.instrument.navigation.preview.map(|target|target.member.to_string()).unwrap_or_default())
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
