// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The current graph location is also the local document search. Editing is
//! transient; only accepting a destination enters scope history.
use super::*;
use graphshell::scope_policy::{ScopeCommand, ScopeLevel};
use cambium::{KeyEvent, TextFieldMode, TextInput, caret_field_children, on_key, request_focus};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    input: TextInput,
    selected: usize,
    pending: Option<Destination>,
}
#[derive(Clone)]
enum Destination {
    Scope(ScopeLevel),
    Document(Uuid),
}
struct Choice {
    title: String,
    address: String,
    destination: Destination,
}

pub(super) fn edit(page: &mut TreePage) {
    page.instrument.location = State {
        open: true,
        ..Default::default()
    };
    page.instrument.panes_open = false;
    page.instrument.navigation.open = false;
    page.shared.dirty.set(true);
}
fn choices(page: &TreePage) -> Vec<Choice> {
    let query = page.instrument.location.input.text().trim().to_lowercase();
    let mut candidates = vec![
        Choice {
            title: "Meres".into(),
            address: "mere://".into(),
            destination: Destination::Scope(ScopeLevel::Overmap),
        },
        Choice {
            title: "Local Mere".into(),
            address: "mere://local".into(),
            destination: Destination::Scope(ScopeLevel::Graph),
        },
    ];
    if super::scope_navigation::place(page).is_some_and(|p| p.focus.is_some()) {
        candidates.push(Choice {
            title: "Neighborhood".into(),
            address: "mere://local/neighborhood".into(),
            destination: Destination::Scope(ScopeLevel::Neighborhood),
        });
    }
    let canvas = page.shared.canvas.borrow();
    candidates.extend(canvas.graph().nodes().map(|(key, node)| Choice {
        title: canvas.graph().node_display_label(key),
        address: node.url().to_owned(),
        destination: Destination::Document(node.id),
    }));
    let score = |c: &Choice| {
        if query.is_empty() {
            return if matches!(c.destination, Destination::Document(_)) {
                1
            } else {
                2
            };
        }
        if c.address.to_lowercase() == query {
            0
        } else if c.title.to_lowercase().starts_with(&query) {
            1
        } else if c.title.to_lowercase().contains(&query) {
            2
        } else if c.address.to_lowercase().contains(&query) {
            3
        } else {
            99
        }
    };
    candidates.retain(|c| score(c) < 99);
    candidates.sort_by_key(score);
    candidates.truncate(8);
    candidates
}
fn accept(page: &mut TreePage, destination: Destination) {
    if page
        .forme
        .as_ref()
        .is_none_or(|p| p.saving || p.retry.is_some())
    {
        page.instrument.location.pending = Some(destination);
        page.shared.dirty.set(true);
        return;
    }
    page.instrument.location.open = false;
    match destination {
        Destination::Scope(level) => {
            super::scope_navigation::command(page, ScopeCommand::To(level.into()))
        },
        Destination::Document(member) => {
            page.shared.canvas.borrow_mut().select_member(member);
            super::forme::selection_changed(page);
            super::forme::read_related(page, member);
        },
    }
    page.shared.dirty.set(true);
}
/// Read the member that was activated, retaining its identity while a save
/// finishes instead of resolving a potentially changed canvas focus later.
pub(super) fn read_document(page: &mut TreePage, member: Uuid) {
    accept(page, Destination::Document(member));
}
pub(super) fn needs_poll(page: &TreePage) -> bool {
    page.instrument.location.pending.is_some()
        && page
            .forme
            .as_ref()
            .is_some_and(|p| !p.saving && p.retry.is_none())
}
pub(super) fn poll(page: &mut TreePage) {
    if let Some(destination) = page.instrument.location.pending.take() {
        accept(page, destination);
    }
}
pub(super) fn view(page: &TreePage) -> Child {
    if !page.instrument.location.open {
        return super::scope_navigation::trail(page).unwrap_or_else(|| Box::new(el("div", ())));
    }
    let field = request_focus(
        on_key(
            el(
                "div",
                caret_field_children::<TreePage, ()>(&page.instrument.location.input, &[]),
            )
            .attr("class", "workspace-location-input")
            .attr("role", "combobox")
            .attr("aria-label", "Address or graph search")
            .attr("aria-expanded", "true")
            .attr(
                "data-cambium-text-value",
                page.instrument.location.input.text(),
            )
            .attr("style", cambium::SINGLE_LINE_FIELD_STYLE),
            |page: &mut TreePage, event: KeyEvent| {
                event.prevent_default();
                event.stop_propagation();
                match event.key {
                    Key::Named(NamedKey::Escape) => page.instrument.location.open = false,
                    Key::Named(NamedKey::Enter) => {
                        let available = choices(page);
                        if let Some(choice) = available.get(
                            page.instrument
                                .location
                                .selected
                                .min(available.len().saturating_sub(1)),
                        ) {
                            accept(page, choice.destination.clone());
                        }
                    },
                    Key::Named(NamedKey::ArrowDown) => {
                        let count = choices(page).len();
                        page.instrument.location.selected =
                            (page.instrument.location.selected + 1).min(count.saturating_sub(1));
                    },
                    Key::Named(NamedKey::ArrowUp) => {
                        page.instrument.location.selected =
                            page.instrument.location.selected.saturating_sub(1)
                    },
                    _ => {
                        page.instrument
                            .location
                            .input
                            .apply_key(&event, TextFieldMode::SingleLine);
                        page.instrument.location.selected = 0;
                    },
                }
                page.shared.dirty.set(true);
            },
        ),
        true,
    );
    cambium::scope_navigation::scope_location(
        Vec::new(),
        Some(Box::new(field)),
        super::scope_navigation::command,
    )
}
pub(super) fn popup(page: &TreePage) -> Option<Child> {
    if !page.instrument.location.open {
        return None;
    }
    let available = choices(page);
    let selected = page
        .instrument
        .location
        .selected
        .min(available.len().saturating_sub(1));
    let mut rows: Vec<Child> = available
        .into_iter()
        .enumerate()
        .map(|(index, choice)| {
            cambium::shell_navigation_item(
                choice.title,
                choice.address,
                cambium::ShellControl {
                    pressed: Some(index == selected),
                    ..Default::default()
                },
                move |page: &mut TreePage, _| accept(page, choice.destination.clone()),
            )
        })
        .collect();
    if rows.is_empty() {
        rows.push(Box::new(
            el("p", "No matching document in this Mere").attr("role", "status"),
        ));
    }
    Some(Box::new(
        el("div", cambium::shell_popup("Documents and scopes", rows))
            .attr("class", "workspace-locations"),
    ))
}
pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    if !runner.state().instrument.location.open {
        return None;
    }
    let node = runner.focus()?;
    if !focused_field_has_label(&runner.dom().borrow(), node, "Address or graph search") {
        return None;
    }
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(|p| &p.instrument.location.input),
        get_mut: Box::new(|p| &mut p.instrument.location.input),
    })
}
