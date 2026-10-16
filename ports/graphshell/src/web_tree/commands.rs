// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Viewer command composition over Cambium's shared set, menu and text input.
use super::*;
use cambium::{
    Command, CommandChoices, CommandSet, MenuSession, TextInput, button, focusable_if, lens,
    on_click, request_focus, text_field_typed,
};
use graphshell::viewer_commands::{self, Action, Availability};

pub(super) const SHEET: &str = "\
.tree-command-menu { z-index:30; padding:10px; box-sizing:border-box; background:#17232b; border:1px solid #79a9be; color:#dce3e8; } \
.tree-command-menu h2 { font-size:16px; margin:0 0 8px; } \
.tree-command-menu label { display:block; margin:4px 0 8px; } \
.tree-command-menu [data-cambium-text-value] { display:block; width:100%; min-height:26px; background:#263640; color:#dce3e8; border:1px solid #637581; } \
.tree-command-menu [role=option] { display:block; width:100%; text-align:left; padding:6px 8px; margin:2px 0; border:1px solid transparent; background:#263640; color:#dce3e8; } \
.tree-command-menu [role=option][aria-selected=true] { background:#2d5a63; border-color:#79a9be; } \
.tree-command-menu [aria-disabled=true] { opacity:.6; } \
.tree-command-menu small { display:block; font-size:12px; } \
.tree-command-menu .command-actions { display:flex; flex-wrap:wrap; gap:6px; margin-top:8px; } \
.tree-command-menu .command-actions button { background:#263640; color:#dce3e8; border:1px solid #637581; padding:5px 8px; } \
.tree-command-menu p { font-size:12px; margin:6px 0 0; }";

#[derive(Default)]
pub(super) struct State {
    pub(super) open: Option<Open>,
    pub(super) choices: CommandChoices,
    pub(super) focus_graph: bool,
    pub(super) last: String,
}

pub(super) struct Open {
    session: MenuSession,
    search: TextInput,
    at: (f32, f32),
}

pub(super) enum Request {
    Context {
        /// Position in the retained root, not document/client coordinates.
        at: (f32, f32),
        node: Option<uuid::Uuid>,
    },
    Selection,
}

fn catalog(page: &TreePage) -> CommandSet {
    let canvas = page.shared.canvas.borrow();
    let locked = page.shared.selection_locked.get() || page.shared.remote_shown.get();
    let mut set = viewer_commands::commands(Availability {
        hosted: matches!(page.dataset, HostedDataset::Loaded(_)),
        selection: canvas.can_fit_selection(),
        locked,
        paused: canvas.physics_paused(),
        history: page
            .presenter
            .as_ref()
            .filter(|pane| pane.options.browsing)
            .filter(|_| {
                page.history
                    .as_ref()
                    .is_some_and(|history| history.len() > 1)
            })
            .map(|pane| pane.history_open),
    });
    if let Some(grouped) = &page.grouping {
        let picked = canvas
            .focused_url()
            .and_then(|url| url.strip_prefix("urn:host-dataset:"));
        if let Ok(projection) = grouped.projection() {
            for id in grouped
                .hierarchy
                .groups()
                .filter(|id| projection.visible.contains(*id))
            {
                let entered = grouped.state.entered.as_deref() == Some(id);
                let expanded = entered || grouped.state.expanded.contains(id);
                for (verb, label) in [
                    (
                        "toggle",
                        format!(
                            "{} {}",
                            if expanded { "Collapse" } else { "Expand" },
                            grouped.label(id)
                        ),
                    ),
                    ("enter", format!("Enter {}", grouped.label(id))),
                ] {
                    let command_id = format!("fold:{verb}:{id}");
                    set.register(Command::new(
                        &command_id,
                        label,
                        if picked == Some(id) { "node" } else { "fold" },
                    ));
                    if locked || entered {
                        set.set_disabled(
                            &command_id,
                            Some(
                                if locked {
                                    "Finish the pending graph operation first"
                                } else {
                                    "Already inside this group; use Whole graph to leave"
                                }
                                .into(),
                            ),
                        );
                    }
                }
            }
        }
        set.register(Command::new("whole-graph", "Whole graph", "canvas"));
        set.register(Command::new(
            "relationship-evidence",
            if page.grouping_evidence {
                "Hide relationship evidence"
            } else {
                "Show relationship evidence"
            },
            "canvas",
        ));
        if locked {
            set.set_disabled(
                "whole-graph",
                Some("Finish the pending graph operation first".into()),
            );
        }
    }
    set
}

pub(super) fn open(page: &mut TreePage, request: Request) {
    // A local Product owns its pending operation and its command menu.
    if page
        .product
        .as_ref()
        .is_some_and(|product| product.selection_locked())
    {
        return;
    }
    if (page.shared.selection_locked.get() || page.shared.remote_shown.get())
        && matches!(
            &request,
            Request::Selection | Request::Context { node: Some(_), .. }
        )
    {
        return;
    }
    let Request::Context { at, node } = request else {
        let canvas = page.shared.canvas.borrow();
        page.picked = canvas.focused_url().map(str::to_owned);
        if let Some(product) = &mut page.product {
            product.close_menu();
            product.select(canvas.selected_members().first().copied());
        }
        page.commands.open = None;
        page.commands.focus_graph = true;
        page.shared.dirty.set(true);
        return;
    };
    if let Some(product) = &mut page.product {
        // DOM secondary capture and keyboard invocation use the same persisted
        // local command owner as retained HostPointer delivery.
        page.commands.open = None;
        let mut canvas = page.shared.canvas.borrow_mut();
        product.open_menu(mere::canvas::ContextRequest { at, node }, &mut canvas);
        page.picked = canvas.focused_url().map(str::to_owned);
        product.select(canvas.selected_members().first().copied());
        page.commands.focus_graph = false;
        page.shared.dirty.set(true);
        return;
    }
    if let Some(node) = node {
        let mut canvas = page.shared.canvas.borrow_mut();
        // Opening on an already-selected member retains a multi-selection.
        if !canvas.selected_members().contains(&node) {
            canvas.select_member(node);
        }
        page.picked = canvas.focused_url().map(str::to_owned);
        if let Some(product) = &mut page.product {
            product.select(Some(node));
        }
    }
    let context = if node.is_some() || !page.shared.canvas.borrow().selected_members().is_empty() {
        "node"
    } else {
        "canvas"
    };
    page.commands.open = Some(Open {
        session: MenuSession::open(Some(context)),
        search: TextInput::new(""),
        at,
    });
    page.commands.focus_graph = false;
    page.shared.dirty.set(true);
}

pub(super) fn open_keyboard(page: &mut TreePage) {
    open(
        page,
        Request::Context {
            at: (12.0, 80.0),
            node: None,
        },
    );
}

pub(super) fn close(page: &mut TreePage) {
    if let Some(product) = &mut page.product {
        product.close_menu();
    }
    page.commands.open = None;
    page.commands.focus_graph = true;
    page.shared.dirty.set(true);
}

fn run(page: &mut TreePage, id: &str) {
    // Re-read current capabilities instead of trusting a row from a prior frame.
    let set = catalog(page);
    let Some(command) = set.get(id) else {
        return;
    };
    if let Some(reason) = &command.disabled_reason {
        page.commands.last = reason.clone();
        return;
    }
    match viewer_commands::action(id, matches!(page.dataset, HostedDataset::Loaded(_))) {
        Some(Action::Canvas(CanvasCommand::Pan { dx, dy })) => page.shared.pan(dx, dy),
        Some(Action::Canvas(command)) => {
            command.apply(&mut *page.shared.canvas.borrow_mut(), page.shared.size.get());
            crate::web_suggestions::after_command(command, &page.shared.canvas.borrow());
        },
        Some(Action::Wheel(mode)) => {
            page.shared.wheel_mode.set(mode);
            page.shared
                .root
                .set_attribute("data-wheel-mode", mode.name())
                .ok();
        },
        Some(Action::Tools) => page.tools_open = !page.tools_open,
        Some(Action::History) => {
            if let Some(pane) = &mut page.presenter {
                pane.history_open = !pane.history_open;
            }
        },
        None if id == "whole-graph" => page.change_group(|state| state.entered = None),
        None if id == "relationship-evidence" => page.grouping_evidence = !page.grouping_evidence,
        None => {
            if let Some(id) = id.strip_prefix("fold:enter:") {
                page.change_group(|state| state.entered = Some(id.to_owned()));
            } else if let Some(id) = id.strip_prefix("fold:toggle:") {
                page.change_group(|state| {
                    if !state.expanded.remove(id) {
                        state.expanded.insert(id.to_owned());
                    }
                });
            } else {
                return;
            }
        },
    }
    set.record_use(&mut page.commands.choices, id);
    page.commands.last = command.label.clone();
    close(page);
}

pub(super) fn shortcut(page: &mut TreePage, event: &cambium::KeyEvent) -> bool {
    let invoke = matches!(event.key, Key::Named(NamedKey::F10)) && event.mods.shift
        || matches!(&event.key, Key::Character(k) if k.eq_ignore_ascii_case("k"))
            && (event.mods.ctrl || event.mods.meta);
    if invoke {
        open_keyboard(page);
        event.prevent_default();
        event.stop_propagation();
    }
    invoke
}

fn key(page: &mut TreePage, event: cambium::KeyEvent) {
    let set = catalog(page);
    let Some(menu) = &mut page.commands.open else {
        return;
    };
    menu.session.set_query(menu.search.text());
    match event.key {
        Key::Named(NamedKey::Escape) => close(page),
        Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp) => {
            let count = menu.session.rows(&set, &page.commands.choices).len();
            menu.session.step(
                if matches!(event.key, Key::Named(NamedKey::ArrowDown)) {
                    1
                } else {
                    -1
                },
                count,
            );
        },
        Key::Named(NamedKey::Enter) => {
            let id = menu
                .session
                .chosen(&menu.session.rows(&set, &page.commands.choices))
                .map(|row| row.id.clone());
            if let Some(id) = id {
                run(page, &id);
            }
        },
        _ => return,
    }
    event.prevent_default();
    event.stop_propagation();
}

pub(super) fn view(page: &TreePage) -> Option<Child> {
    let menu = page.commands.open.as_ref()?;
    let set = catalog(page);
    let mut session = menu.session.clone();
    session.set_query(menu.search.text());
    let rows = session.rows(&set, &page.commands.choices);
    let browsing = page
        .presenter
        .as_ref()
        .is_some_and(|pane| pane.options.browsing);
    let visible = if browsing {
        viewer_commands::browsing_page(session.selected, rows.len())
    } else {
        0..rows.len()
    };
    let start = visible.start;
    let end = visible.end;
    let count = rows.len();
    let active = session
        .selected
        .map(|index| format!("graph-command-{index}"));
    let search = lens(
        move |input: &mut TextInput| {
            request_focus(
                text_field_typed(input)
                    .attr("id", "graph-command-search")
                    .attr("aria-label", "Search graph commands")
                    .attr("aria-controls", "graph-command-items")
                    .attr("aria-activedescendant", active.clone().unwrap_or_default()),
                true,
            )
        },
        |page: &mut TreePage| {
            &mut page
                .commands
                .open
                .as_mut()
                .expect("visible command search")
                .search
        },
    );
    let items: Vec<Child> = rows
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(index, command)| {
            let id = command.id.clone();
            let enabled = command.disabled_reason.is_none();
            let content = (
                el("span", command.label.clone()),
                command
                    .disabled_reason
                    .clone()
                    .map(|reason| el("small", reason)),
            );
            Box::new(
                focusable_if(
                    on_click(el("button", content), move |page: &mut TreePage, _| {
                        run(page, &id)
                    }),
                    false,
                )
                .attr("id", format!("graph-command-{index}"))
                .attr("role", "option")
                .attr("aria-label", command.label.clone())
                .attr(
                    "aria-description",
                    command.disabled_reason.clone().unwrap_or_default(),
                )
                .attr(
                    "aria-selected",
                    if session.selected == Some(index) {
                        "true"
                    } else {
                        "false"
                    },
                )
                .attr("aria-disabled", if enabled { "false" } else { "true" })
                .attr("tabindex", "-1"),
            ) as Child
        })
        .collect();
    let chosen = session.chosen(&rows).map(|row| row.id.clone());
    let kept = chosen.as_ref().is_some_and(|id| {
        set.kept(&page.commands.choices)
            .iter()
            .any(|row| &row.id == id)
    });
    let width = page.size.0.saturating_sub(24).clamp(1, 360);
    let height = page.size.1.saturating_sub(24).clamp(1, 460);
    let left = menu
        .at
        .0
        .clamp(0.0, page.size.0.saturating_sub(width) as f32);
    let top = menu
        .at
        .1
        .clamp(0.0, page.size.1.saturating_sub(height) as f32);
    let choice = focusable_if(
        button(
            if kept {
                "Remove selected command from menu"
            } else {
                "Keep selected command in menu"
            },
            move |page: &mut TreePage, _| {
                if let Some(id) = &chosen {
                    let set = catalog(page);
                    if kept {
                        set.remove(&mut page.commands.choices, id);
                    } else {
                        set.add(&mut page.commands.choices, id);
                    }
                }
            },
        ),
        !rows.is_empty(),
    );
    let paging = browsing.then(|| {
        el(
            "div",
            (
                (start > 0).then(|| {
                    button("Previous commands", move |page: &mut TreePage, _| {
                        if let Some(menu) = &mut page.commands.open {
                            menu.session.set_query(menu.search.text());
                            menu.session.selected = Some(start.saturating_sub(2));
                        }
                    })
                }),
                (end < count).then(|| {
                    button("Next commands", move |page: &mut TreePage, _| {
                        if let Some(menu) = &mut page.commands.open {
                            menu.session.set_query(menu.search.text());
                            menu.session.selected = Some(end);
                        }
                    })
                }),
            ),
        )
        .attr("class", "command-actions")
    });
    Some(Box::new(on_key(el("section", (
        el("h2", "Graph commands"),
        browsing.then(|| el("div", button("Close commands", |page: &mut TreePage, _| close(page))).attr("class", "command-actions")),
        on_key(el("label", (el("span", "Search commands"), search)), key).focusable(false),
        el("div", items).attr("id", "graph-command-items").attr("role", "listbox").attr("aria-label", "Graph commands"),
        rows.is_empty().then(|| el("p", "No command matches")),
        paging,
        el("div", (
            button("All commands", |page: &mut TreePage, _| { if let Some(menu) = &mut page.commands.open { menu.session.expand_all(); } }),
            (!browsing).then_some(choice),
            (!browsing).then(|| button("Close commands", |page: &mut TreePage, _| close(page))),
        )).attr("class", "command-actions"),
        el("p", if browsing { format!("Commands {}–{} of {}", if count == 0 { 0 } else { start + 1 }, end, count) } else { format!("Wheel: {} · {}", page.shared.wheel_mode.get().name(), page.commands.last) }).attr("role", "status"),
        (!browsing).then(|| el("p", "Menu choices apply while this viewer is open.")),
    )).attr("class", "tree-command-menu").attr("role", "dialog").attr("aria-label", "Graph commands")
        .attr("style", format!("position:absolute;left:{left}px;top:{top}px;width:{width}px;max-height:{height}px;overflow-y:auto;")), |page: &mut TreePage, event: cambium::KeyEvent| {
            if matches!(event.key, Key::Named(NamedKey::Escape)) {
                close(page);
                event.prevent_default();
                event.stop_propagation();
            }
        }).focusable(false)))
}

pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    runner.state().commands.open.as_ref()?;
    let node = runner.focus()?;
    if !taproot::matching(
        &runner.dom().borrow(),
        &Selector::role("textbox").containing("Search graph commands"),
    )
    .contains(&node)
    {
        return None;
    }
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(|page| {
            &page
                .commands
                .open
                .as_ref()
                .expect("focused command search")
                .search
        }),
        get_mut: Box::new(|page| {
            &mut page
                .commands
                .open
                .as_mut()
                .expect("focused command search")
                .search
        }),
    })
}
