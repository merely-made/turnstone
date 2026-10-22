// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The chrome as a cambium view over a FOREST of window-roots — the toolkit
//! question's endpoint, executed 2026-07-18 ("chrome migrates to a cambium
//! view"), and turnstone's literal consumption of the two forest primitives:
//! cambium's `push_forest_projection` (every window's chrome is a window-root
//! subtree of ONE shared document) and retained Livery/Buckram subtree layout (each
//! window lays out and paints ITS root at its own size).
//!
//! What changed from the rung-3 hand chrome (`ui::chrome_scene`, retired with
//! this): the DOM is RETAINED and diffed per state change instead of rebuilt
//! wholesale per frame; suggestion rows carry real `on_click` handlers (a row
//! click commits — new capability, lowered as `OmnibarCommitRow` through the
//! spine); and lens windows get chrome (the caption chip) as their own forest
//! projections — one chrome document, N window-roots.
//!
//! What deliberately did NOT migrate, and why (consumer-pull teaching the
//! catalog, recorded against the surfaces-in-cambium mapping's prediction):
//!
//! - `command_palette`/`action_list` own their `query` state and filter a
//!   static item list; turnstone's omnibar keys lower through the Action spine
//!   (doctrine 2) and its suggestions are GRAPH TRUTH re-queried per edit.
//!   The rows here render `OmnibarState`, they do not own it.
//! - The omnibar is a controlled mirror, not an editor: keys lower through the
//!   Action spine and the view must not mutate a second text model. Cambium now
//!   exposes `caret_field_children`, the pure text/preedit/caret projection
//!   beneath its editable field. Turnstone consumes that rendering while
//!   retaining app truth and input authority here.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetCtx, GenetElement, GenetMultiRunner, PointerClick, ProjectionId,
    TextInput, caret_field_children, el, on_click,
};
use genet_scripted_dom::ScriptedDom;
// `drain_mutations` is the mutation queue's only reader; the chrome owns that
// drain for the whole forest (see `absorb_dom_mutations`).
use layout_dom_api::{LayoutDom, LayoutDomMut};

use crate::app::App;
use crate::panes::{ChromeEdge, ChromePlacement};
use crate::ui::{CARD_TOP, CARD_W, OmnibarState, Suggestion};

/// What a chrome interaction produces: commit the suggestion row at this
/// original index (the shell lowers `Action::OmnibarCommitRow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeIntent {
    CommitRow(usize),
    /// Keep or drop the command on this suggestion row.
    ToggleKeep(usize),
    NavBack,
    NavForward,
    Reload,
    Stop,
    KeepNode(uuid::Uuid),
    Address,
    WorkspaceMap(bool),
    Tool(&'static str),
    ReadMember(uuid::Uuid),
    NavigateRelationship(crate::relationship_navigation::RelationshipTarget),
    RevealMember(uuid::Uuid),
    Layout(crate::chrome_layout::ChromeLayout),
    AllPanes,
    Territory,
    FitGraph,
    MenuChanged,
    FindPrevious,
    FindNext,
    FindClose,
    Permission {
        request: crate::user_agent_decision::UserAgentRequestKey,
        choice: crate::user_agent_decision::PermissionChoice,
    },
    FocusAuthentication(crate::user_agent_decision::AuthenticationField),
    ToggleAuthenticationMemory,
    SubmitAuthentication(crate::user_agent_decision::UserAgentRequestKey),
    CancelAuthentication(crate::user_agent_decision::UserAgentRequestKey),
}

impl cambium::Action for ChromeIntent {}

/// How one suggestion row renders.
#[derive(Clone, Debug, PartialEq)]
struct RowView {
    text: String,
    /// Class: selected / plain / muted (hints are inert).
    class: &'static str,
    /// The row's index in `OmnibarState::suggestions` (what a click commits);
    /// `None` for inert hint rows.
    commit: Option<usize>,
    /// For a command row, whether the person keeps it; it then carries a
    /// Keep/Drop control beside its label.
    keep: Option<bool>,
}

#[derive(Clone, PartialEq)]
enum DecisionCard {
    Permission {
        request: crate::user_agent_decision::UserAgentRequestKey,
        prompt: String,
        queued: usize,
        submitting: bool,
        error: Option<String>,
    },
    Authentication {
        request: crate::user_agent_decision::UserAgentRequestKey,
        prompt: String,
        queued: usize,
        username: TextInput,
        password: TextInput,
        field: crate::user_agent_decision::AuthenticationField,
        remember_for_process: bool,
        submitting: bool,
        error: Option<String>,
    },
}

impl std::fmt::Debug for DecisionCard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Permission {
                request,
                prompt,
                queued,
                submitting,
                error,
            } => formatter
                .debug_struct("Permission")
                .field("request", request)
                .field("prompt", prompt)
                .field("queued", queued)
                .field("submitting", submitting)
                .field("error", error)
                .finish(),
            Self::Authentication {
                request,
                prompt,
                queued,
                field,
                remember_for_process,
                submitting,
                error,
                ..
            } => formatter
                .debug_struct("Authentication")
                .field("request", request)
                .field("prompt", prompt)
                .field("queued", queued)
                .field("username", &"[redacted]")
                .field("password", &"[redacted]")
                .field("field", field)
                .field("remember_for_process", remember_for_process)
                .field("submitting", submitting)
                .field("error", error)
                .finish(),
        }
    }
}

/// One window's chrome inputs: its size and its caption.
#[derive(Clone, Debug, Default, PartialEq)]
struct WindowChrome {
    w: f32,
    h: f32,
    caption: Option<String>,
    /// Only the primary window carries the omnibar; lenses show the chip.
    primary: bool,
    titlebar_insets: cambium_rootstock::TitlebarInsets,
    display_scale: f32,
}

/// The one chrome state every projection renders (the one-state contract).
struct ChromeState {
    menu: Option<ChromeMenu>,
    menu_selected: usize,
    documents_start: usize,
    relationships_start: usize,
    layout: crate::chrome_layout::ChromeLayout,
    map_active: bool,
    territory: bool,
    tools: Vec<(&'static str, String, bool)>,
    connections: Vec<crate::relationship_navigation::RelationshipRow>,
    documents: Vec<(uuid::Uuid, String, bool)>,
    reading_stacks: Vec<Vec<(uuid::Uuid, String, bool)>>,
    windows: Vec<WindowChrome>,
    open: bool,
    inline_address: bool,
    /// App-owned omnibar truth projected into Cambium's pure caret renderer.
    /// This runner never dispatches edit keys into it.
    omnibar_input: TextInput,
    rows: Vec<RowView>,
    omnibar_placement: ChromePlacement,
    shellbar_placement: ChromePlacement,
    shellbar_visible: bool,
    zoom: f32,
    can_back: bool,
    can_forward: bool,
    focused_node: Option<uuid::Uuid>,
    focused_kept: bool,
    fetching: bool,
    fetch_status: Option<String>,
    focused_url: Option<String>,
    link_preview: Option<String>,
    find_open: bool,
    find_input: TextInput,
    find_status: String,
    decision: Option<DecisionCard>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChromeMenu {
    Panes,
    Connections,
    Documents,
}

type ChromeView = Box<dyn AnyView<ChromeState, ChromeIntent, GenetCtx, GenetElement>>;

/// One window's chrome view: the caption chip, plus the omnibar card on the
/// primary while open. Positioned by transform-translate (the property the
/// canvas gnode pool proves the retained engine honors on absolutes).
fn window_chrome_view(state: &ChromeState, slot: usize) -> ChromeView {
    let Some(win) = state.windows.get(slot).cloned() else {
        return Box::new(el::<_, ChromeState, ChromeIntent>("div", ()));
    };
    let scale = if win.display_scale > 0.0 { win.display_scale } else { 1.0 };
    let zoom = state.zoom * scale;
    let mut children: Vec<ChromeView> = Vec::new();
    let navigation_height = navigation_height(state.zoom) * scale;
    let status_height = status_height(state.zoom) * scale;
    let caption_at_top = matches!(
        state.shellbar_placement,
        ChromePlacement::Docked(ChromeEdge::Top)
    );
    let title_height = navigation_height.max(win.titlebar_insets.height);
    let merged = win.primary && state.shellbar_visible && !state.layout.toolbar_bottom;
    let navigation_top = if state.layout.toolbar_bottom {
        (win.h - navigation_height).max(title_height)
    } else {
        0.0
    };
    children.push(window_title_bar(state, &win, slot));

    if win.primary && state.shellbar_visible {
        let leading = if merged {
            win.titlebar_insets.left
        } else {
            0.0
        };
        let available = win.w - leading;
        let density = cambium::ShellDensity::for_width(available, zoom);
        let tiny = density == cambium::ShellDensity::Tiny;
        let compact = density.is_compact();
        let wide = density == cambium::ShellDensity::Wide;
        let height = if merged {
            title_height
        } else {
            navigation_height
        };
        let mut controls: Vec<ChromeView> = Vec::new();
        if !tiny {
            controls.push(browser_control(
                "‹",
                "Back",
                state.can_back.then_some(ChromeIntent::NavBack),
            ));
        }
        if !compact {
            controls.push(browser_control(
                "›",
                "Forward",
                state.can_forward.then_some(ChromeIntent::NavForward),
            ));
            controls.push(browser_control(
                if state.fetching { "Stop" } else { "Reload" },
                if state.fetching { "Stop" } else { "Reload" },
                state.focused_node.map(|_| {
                    if state.fetching {
                        ChromeIntent::Stop
                    } else {
                        ChromeIntent::Reload
                    }
                }),
            ));
        }
        let leading_controls = std::mem::take(&mut controls);
        let status = state
            .focused_url
            .clone()
            .or_else(|| state.fetch_status.clone())
            .unwrap_or_else(|| "Search the document graph or enter an address".into());
        if state.inline_address {
            controls.push(Box::new(on_click(
                el::<_, ChromeState, ChromeIntent>(
                    "div",
                    caret_field_children::<ChromeState, ChromeIntent>(&state.omnibar_input, &[]),
                )
                .attr("class", "browser-status omni-input")
                .attr("role", "combobox")
                .attr("aria-label", "Address or search document graph")
                .attr("aria-expanded", "true"),
                |_: &mut ChromeState, _: PointerClick| ChromeIntent::MenuChanged,
            )));
        } else {
            controls.push(Box::new(on_click(
                el::<_, ChromeState, ChromeIntent>("button", status)
                    .attr("class", "browser-status")
                    .attr("aria-label", "Address or search document graph"),
                |_state: &mut ChromeState, _: PointerClick| ChromeIntent::Address,
            )));
        }
        let location = controls.pop().expect("address slot");
        if !compact {
            controls.push(browser_control(
                if state.focused_kept { "Kept" } else { "Keep" },
                "Keep document",
                state
                    .focused_node
                    .filter(|_| !state.focused_kept)
                    .map(ChromeIntent::KeepNode),
            ));
        }
        if !state.layout.activity_rail && (!tiny || available >= 230.0 * zoom) {
            controls.push(cambium::shell_projection_switch(state.map_active, tiny,
                |_: &mut ChromeState, map| ChromeIntent::WorkspaceMap(map)));
        }
        if !compact {
            controls.push(menu_button(
                state,
                format!("Documents {}", state.documents.len()),
                "Documents in this map",
                ChromeMenu::Documents,
            ));
        }
        if !compact {
            controls.push(menu_button(
                state,
                format!("Related {}", state.connections.len()),
                "Document connections",
                ChromeMenu::Connections,
            ));
        }
        if wide && !state.layout.activity_rail {
            for (id, label) in [(crate::panes::kind::INSPECTOR, "Inspect"), (crate::panes::kind::TRAIL, "Trail")] {
                let visible = state.tools.iter().any(|(kind, _, visible)| *kind == id && *visible);
                controls.push(selected_button(label, ChromeIntent::Tool(id), visible));
            }
        }
        controls.push(menu_button(
            state,
            if tiny { "⋯".into() } else { "Panes".into() },
            "Panes",
            ChromeMenu::Panes,
        ));
        let left_padding = leading
            + if merged && !tiny {
                38.0 * zoom
            } else {
                6.0
            };
        children.push(Box::new(el::<_, ChromeState, ChromeIntent>("div", cambium::shell_toolbar("Document graph navigation", leading_controls, location, controls))
            .attr("class", if state.fetching { "browser-strip browser-loading" } else { "browser-strip" })
            .attr("style", format!("transform:translate({left_padding}px,{navigation_top}px);width:{}px;height:{height}px;padding-left:0px;--app-region:{};",(win.w-left_padding).max(0.0), if merged { "drag" } else { "no-drag" }))));

        let top_status = matches!(state.shellbar_placement, ChromePlacement::Docked(ChromeEdge::Top));
        let rail_top = title_height + if top_status { status_height } else { 0.0 };
        let rail_bottom = if top_status { 0.0 } else { status_height }
            + if state.layout.toolbar_bottom { navigation_height } else { 0.0 };
        let rail_height = (win.h - rail_top - rail_bottom).max(0.0);
        let rail_width = if state.layout.activity_rail { cambium::ShellMetrics::ACTIVITY_RAIL_WIDTH * zoom } else { 0.0 };
        if state.layout.activity_rail {
        let projections = vec![
            activity_button("▤", "Read", "Read", ChromeIntent::WorkspaceMap(false), Some(!state.map_active)),
            activity_button("◇", "Map", "Map", ChromeIntent::WorkspaceMap(true), Some(state.map_active)),
        ];
        let mut tools = Vec::new();
        for (mark, label, id) in [("ⓘ", "Inspect", crate::panes::kind::INSPECTOR), ("↶", "Trail", crate::panes::kind::TRAIL)] {
            let visible = state.tools.iter().any(|(kind, _, visible)| *kind == id && *visible);
            tools.push(activity_button(mark, label, label, ChromeIntent::Tool(id), Some(visible)));
        }
        tools.push(activity_button("⌕", "Search", "Search document graph", ChromeIntent::Address, None));
        tools.push(activity_button("▥", "Tabs", "Reading tabs", ChromeIntent::Layout(crate::chrome_layout::ChromeLayout { document_rail: !state.layout.document_rail, ..state.layout }), Some(state.layout.document_rail)));
        let utilities = vec![activity_button("⌖", "Fit", "Fit graph", ChromeIntent::FitGraph, None)];
        children.push(Box::new(el("div", cambium::shell_activity_rail(projections, tools, utilities))
            .attr("class", "turnstone-activity-rail")
            .attr("style", format!("position:absolute;left:0px;top:{rail_top}px;width:{rail_width}px;height:{rail_height}px;--shell-rail-width:{rail_width}px;--shell-activity-size:{}px;--shell-activity-mark-size:{}px;--shell-activity-caption-size:{}px;", 46.0*zoom,18.0*zoom,10.0*zoom))));
        }
        if state.layout.document_rail {
            let groups = if state.reading_stacks.is_empty() {
                vec![("Reading tiles".into(), vec![browser_button("No arranged documents", None)])]
            } else {
                state.reading_stacks.iter().enumerate().map(|(i, stack)| {
                    (format!("Tile {}", i + 1), stack.iter().map(|(member, title, active)| selected_button(title, ChromeIntent::ReadMember(*member), *active)).collect())
                }).collect()
            };
            let width = cambium::ShellMetrics::DOCUMENT_NAV_WIDTH * zoom;
            children.push(Box::new(el("div", cambium::shell_document_rail(groups))
                .attr("class", "turnstone-reading-tabs")
                .attr("style", format!("position:absolute;left:{rail_width}px;top:{rail_top}px;width:{width}px;height:{rail_height}px;--shell-document-width:{width}px;"))));
        }

        if let Some(menu) = state.menu {
            children.push(Box::new(on_click(
                el::<_, ChromeState, ChromeIntent>("div", ())
                    .attr("class", "chrome-menu-dismiss")
                    .attr(
                        "style",
                        format!(
                            "position:absolute;left:0;top:{title_height}px;width:{}px;height:{}px;",
                            win.w,
                            (win.h - title_height).max(0.0)
                        ),
                    ),
                |state: &mut ChromeState, _: PointerClick| {
                    state.menu = None;
                    ChromeIntent::MenuChanged
                },
            )));
            let mut rows: Vec<ChromeView> = Vec::new();
            match menu {
                ChromeMenu::Panes => {
                    rows.push(browser_button(
                        "Read document",
                        Some(ChromeIntent::WorkspaceMap(false)),
                    ));
                    rows.push(browser_button(
                        "Show Map",
                        Some(ChromeIntent::WorkspaceMap(true)),
                    ));
                    for (kind, label, visible) in &state.tools {
                        rows.push(selected_button(label, ChromeIntent::Tool(kind), *visible));
                    }
                    rows.push(selected_button("Territory", ChromeIntent::Territory, state.territory));
                    rows.push(selected_button("Activity rail", ChromeIntent::Layout(crate::chrome_layout::ChromeLayout {
                        activity_rail: !state.layout.activity_rail, ..state.layout
                    }), state.layout.activity_rail));
                    rows.push(selected_button("Reading tabs", ChromeIntent::Layout(crate::chrome_layout::ChromeLayout {
                        document_rail: !state.layout.document_rail, ..state.layout
                    }), state.layout.document_rail));
                    rows.push(browser_button("Fit graph", Some(ChromeIntent::FitGraph)));
                    rows.push(menu_button(
                        state,
                        "Document connections…".into(),
                        "Browse document connections",
                        ChromeMenu::Connections,
                    ));
                    rows.push(menu_button(
                        state,
                        "Documents in this map…".into(),
                        "Documents in this map",
                        ChromeMenu::Documents,
                    ));
                    rows.push(browser_button(
                        "All commands…",
                        Some(ChromeIntent::AllPanes),
                    ));
                    for (label, bottom) in [("Toolbar at top", false), ("Toolbar at bottom", true)]
                    {
                        let layout = crate::chrome_layout::ChromeLayout {
                            toolbar_bottom: bottom,
                            ..state.layout
                        };
                        rows.push(selected_button(
                            label,
                            ChromeIntent::Layout(layout),
                            state.layout.toolbar_bottom == bottom,
                        ));
                    }
                    for (label, edge) in [
                        ("Tabs at top", crate::chrome_layout::TabEdge::Top),
                        ("Tabs at bottom", crate::chrome_layout::TabEdge::Bottom),
                        ("Tabs at left", crate::chrome_layout::TabEdge::Left),
                        ("Tabs at right", crate::chrome_layout::TabEdge::Right),
                    ] {
                        let layout = crate::chrome_layout::ChromeLayout {
                            document_tabs: edge,
                            ..state.layout
                        };
                        rows.push(selected_button(
                            label,
                            ChromeIntent::Layout(layout),
                            state.layout.document_tabs == edge,
                        ));
                    }
                },
                ChromeMenu::Connections => {
                    rows.push(browser_button(
                        "Reveal this document in Map",
                        state.focused_node.map(ChromeIntent::RevealMember),
                    ));
                    rows.extend(cambium::shell_paged_list(
                        mere::navigation::RELATIONSHIPS_NOUN,
                        mere::navigation::NO_RELATIONSHIPS,
                        &state.connections,
                        state.relationships_start,
                        mere::navigation::RELATIONSHIPS_PAGE_SIZE,
                        |row| {
                            let target = row.target;
                            cambium::shell_navigation_item(
                                row.title.clone(),
                                mere::navigation::relationship_detail(row),
                                Default::default(),
                                move |_: &mut ChromeState, _| ChromeIntent::NavigateRelationship(target),
                            )
                        },
                        |state: &mut ChromeState, start| {
                            state.relationships_start = start;
                            state.menu_selected = 0;
                            ChromeIntent::MenuChanged
                        },
                    ));
                },
                ChromeMenu::Documents => {
                    rows.push(browser_button(
                        "Reveal current document in Map",
                        state.focused_node.map(ChromeIntent::RevealMember),
                    ));
                    rows.extend(cambium::shell_paged_list(
                        "documents",
                        "Enter an address to add to this map",
                        &state.documents,
                        state.documents_start,
                        8,
                        |(member, label, arranged)| {
                            selected_button(
                                format!("{label}{}", if *arranged { " · in Read" } else { "" }),
                                ChromeIntent::ReadMember(*member),
                                state.focused_node == Some(*member),
                            )
                        },
                        |state: &mut ChromeState, start| {
                            state.documents_start = start;
                            state.menu_selected = 0;
                            ChromeIntent::MenuChanged
                        },
                    ));
                },
            }
            let width = (win.w - 16.0).clamp(
                0.0,
                if menu == ChromeMenu::Panes {
                    420.0 * scale
                } else {
                    360.0 * scale
                },
            );
            let row_count = if menu == ChromeMenu::Panes {
                rows.len().div_ceil(2)
            } else {
                rows.len()
            };
            let menu_height = if menu == ChromeMenu::Connections {
                row_count as f32 * (38.0 * scale + 6.0 * zoom) + 16.0 * scale
            } else { row_count as f32 * (22.0 * scale + 6.0 * zoom) + 16.0 * scale };
            let menu_height = menu_height.min((win.h - title_height - navigation_height - 12.0).max(0.0));
            let top = if state.layout.toolbar_bottom {
                (navigation_top - menu_height - 4.0).max(title_height)
            } else {
                height + 4.0
            };
            let rows = rows
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    Box::new(
                        el::<_, ChromeState, ChromeIntent>("div", row)
                            .attr("class", "chrome-menu-entry")
                            .attr(
                                "data-menu-focus",
                                if index == state.menu_selected {
                                    "true"
                                } else {
                                    "false"
                                },
                            ),
                    ) as ChromeView
                })
                .collect::<Vec<_>>();
            let popup = cambium::shell_popup(
                match menu {
                    ChromeMenu::Panes => "Panes and layout",
                    ChromeMenu::Connections => "Document connections",
                    ChromeMenu::Documents => "Documents in this map",
                },
                rows,
            );
            children.push(Box::new(
                el::<_, ChromeState, ChromeIntent>("div", popup)
                    .attr(
                        "class",
                        if menu == ChromeMenu::Panes {
                            "chrome-menu chrome-panes-menu"
                        } else if menu == ChromeMenu::Connections {
                            "chrome-menu chrome-relations-menu"
                        } else {
                            "chrome-menu"
                        },
                    )
                    .attr(
                        "style",
                        format!(
                            "position:absolute;transform:translate({}px,{top}px);width:{width}px;max-height:{menu_height}px;overflow-y:auto;",
                            (win.w - width - 8.0).max(0.0)
                        ),
                    ),
            ));
        }

        if let Some(target) = &state.link_preview {
            children.push(Box::new(
                el::<_, ChromeState, ChromeIntent>("div", format!("Prospective node  {target}"))
                    .attr("class", "link-preview")
                    .attr(
                        "style",
                        format!(
                            "transform: translate(12px, {}px); max-width: {}px;",
                            (win.h
                                - status_height
                                - if state.layout.toolbar_bottom {
                                    navigation_height
                                } else {
                                    0.0
                                }
                                - 38.0)
                                .max(8.0),
                            (win.w - 24.0).max(120.0)
                        ),
                    ),
            ));
        }
    }

    if win.primary && state.find_open {
        // Keys reach the field through the shell's find routing, so the
        // chrome mounts the shared field without its own key handler.
        let find_children = vec![cambium::find_row(
            cambium::find_field_display::<ChromeState, ChromeIntent>(
                &state.find_input,
                &state.find_status,
            ),
            std::rc::Rc::new(|_: &mut ChromeState, command| match command {
                cambium::ReadCommand::Previous => Some(ChromeIntent::FindPrevious),
                cambium::ReadCommand::Next => Some(ChromeIntent::FindNext),
                _ => Some(ChromeIntent::FindClose),
            }),
        )];
        let left = ((win.w - 620.0) / 2.0).max(8.0);
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", find_children)
                .attr("class", "document-find")
                .attr(
                    "style",
                    format!(
                        "transform: translate({left}px, {}px); width: {}px;",
                        title_height + 8.0,
                        (win.w - 16.0).clamp(0.0, 620.0)
                    ),
                ),
        ));
    }

    if state.shellbar_visible {
        let top = if caption_at_top {
            title_height
        } else {
            (win.h
                - status_height
                - if win.primary && state.layout.toolbar_bottom {
                    navigation_height
                } else {
                    0.0
                })
            .max(0.0)
        };
        let align = match state.shellbar_placement {
            ChromePlacement::Docked(ChromeEdge::Right) => "right",
            ChromePlacement::Docked(ChromeEdge::Top | ChromeEdge::Bottom) => "center",
            _ => "left",
        };
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", win.caption.clone().unwrap_or_default())
                .attr("class", "whereami")
                .attr("role", "status")
                .attr("style", format!("transform: translate(0px, {top}px); width: {}px; height: {status_height}px; text-align: {align};", win.w)),
        ));
    }

    if win.primary
        && state.open
        && let Some((left, top)) = chrome_position(&state.omnibar_placement, win.w, win.h, CARD_W)
    {
        let input = el::<_, ChromeState, ChromeIntent>(
            "div",
            caret_field_children::<ChromeState, ChromeIntent>(&state.omnibar_input, &[]),
        )
        .attr("class", "omni-input");

        let mut card_children: Vec<ChromeView> = if state.inline_address {
            Vec::new()
        } else {
            vec![Box::new(input)]
        };
        for row in &state.rows {
            if let (Some(index), Some(kept)) = (row.commit, row.keep) {
                // A command row: its label commits it, and its own control
                // keeps or drops it. Two siblings, so neither click reaches
                // the other's handler.
                let label = on_click(
                    el::<_, ChromeState, ChromeIntent>("div", row.text.clone())
                        .attr("class", "omni-row-label"),
                    move |_state: &mut ChromeState, _click: PointerClick| {
                        ChromeIntent::CommitRow(index)
                    },
                );
                let toggle = on_click(
                    el::<_, ChromeState, ChromeIntent>("div", if kept { "Drop" } else { "Keep" })
                        .attr("class", "omni-keep"),
                    move |_state: &mut ChromeState, _click: PointerClick| {
                        ChromeIntent::ToggleKeep(index)
                    },
                );
                let children: Vec<ChromeView> = vec![Box::new(label), Box::new(toggle)];
                card_children.push(Box::new(
                    el::<_, ChromeState, ChromeIntent>("div", children).attr(
                        "class",
                        if row.class == "omni-row-sel" {
                            "omni-row-sel omni-row-command"
                        } else {
                            "omni-row omni-row-command"
                        },
                    ),
                ));
                continue;
            }
            let base = el::<_, ChromeState, ChromeIntent>("div", row.text.clone())
                .attr("class", row.class);
            card_children.push(match row.commit {
                // A row click COMMITS that row — the handler is the point of
                // the retained view (the hand chrome had no row handlers).
                Some(index) => Box::new(on_click(
                    base,
                    move |_state: &mut ChromeState, _click: PointerClick| {
                        ChromeIntent::CommitRow(index)
                    },
                )),
                None => Box::new(base),
            });
        }
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", card_children)
                .attr("class", "omni")
                .attr(
                    "style",
                    format!("transform: translate({left}px, {top}px); width: {CARD_W}px;"),
                ),
        ));
    }

    if win.primary
        && let Some(decision) = &state.decision
    {
        let (prompt, queued, submitting, error, mut card_children) = match decision {
            DecisionCard::Permission {
                request,
                prompt,
                queued,
                submitting,
                error,
            } => {
                let intent = |choice| {
                    (!*submitting).then_some(ChromeIntent::Permission {
                        request: *request,
                        choice,
                    })
                };
                let row = vec![
                    browser_button(
                        "Allow once",
                        intent(crate::user_agent_decision::PermissionChoice::AllowOnce),
                    ),
                    browser_button(
                        "Always allow",
                        intent(crate::user_agent_decision::PermissionChoice::AlwaysAllow),
                    ),
                    browser_button(
                        "Deny",
                        intent(crate::user_agent_decision::PermissionChoice::Deny),
                    ),
                ];
                (
                    prompt,
                    *queued,
                    *submitting,
                    error,
                    vec![Box::new(
                        el::<_, ChromeState, ChromeIntent>("div", row)
                            .attr("class", "decision-actions"),
                    ) as ChromeView],
                )
            },
            DecisionCard::Authentication {
                request,
                prompt,
                queued,
                username,
                password,
                field,
                remember_for_process,
                submitting,
                error,
            } => {
                let username_field = el::<_, ChromeState, ChromeIntent>(
                    "div",
                    caret_field_children::<ChromeState, ChromeIntent>(username, &[]),
                )
                .attr(
                    "class",
                    if *field == crate::user_agent_decision::AuthenticationField::Username {
                        "decision-input decision-input-active"
                    } else {
                        "decision-input"
                    },
                )
                .attr("role", "textbox")
                .attr("aria-label", "Username");
                let username_field = on_click(
                    username_field,
                    |_state: &mut ChromeState, _click: PointerClick| {
                        ChromeIntent::FocusAuthentication(
                            crate::user_agent_decision::AuthenticationField::Username,
                        )
                    },
                );
                let password_field = el::<_, ChromeState, ChromeIntent>(
                    "div",
                    caret_field_children::<ChromeState, ChromeIntent>(password, &[]),
                )
                .attr(
                    "class",
                    if *field == crate::user_agent_decision::AuthenticationField::Password {
                        "decision-input decision-input-active"
                    } else {
                        "decision-input"
                    },
                )
                .attr("role", "textbox")
                .attr("aria-label", "Password");
                let password_field = on_click(
                    password_field,
                    |_state: &mut ChromeState, _click: PointerClick| {
                        ChromeIntent::FocusAuthentication(
                            crate::user_agent_decision::AuthenticationField::Password,
                        )
                    },
                );
                let remember = browser_button(
                    if *remember_for_process {
                        "Remember until close: on"
                    } else {
                        "Remember until close: off"
                    },
                    (!*submitting).then_some(ChromeIntent::ToggleAuthenticationMemory),
                );
                let actions = vec![
                    browser_button(
                        "Sign in",
                        (!*submitting).then_some(ChromeIntent::SubmitAuthentication(*request)),
                    ),
                    browser_button(
                        "Cancel",
                        (!*submitting).then_some(ChromeIntent::CancelAuthentication(*request)),
                    ),
                ];
                (
                    prompt,
                    *queued,
                    *submitting,
                    error,
                    vec![
                        Box::new(
                            el::<_, ChromeState, ChromeIntent>(
                                "div",
                                vec![
                                    Box::new(username_field) as ChromeView,
                                    Box::new(password_field) as ChromeView,
                                ],
                            )
                            .attr("class", "decision-fields"),
                        ) as ChromeView,
                        remember,
                        Box::new(
                            el::<_, ChromeState, ChromeIntent>("div", actions)
                                .attr("class", "decision-actions"),
                        ) as ChromeView,
                    ],
                )
            },
        };
        card_children.insert(
            0,
            Box::new(
                el::<_, ChromeState, ChromeIntent>("div", prompt.clone())
                    .attr("class", "decision-prompt")
                    .attr("role", "heading"),
            ),
        );
        if queued > 1 {
            card_children.insert(
                1,
                Box::new(
                    el::<_, ChromeState, ChromeIntent>(
                        "div",
                        format!("1 of {queued} pending requests"),
                    )
                    .attr("class", "decision-status"),
                ),
            );
        }
        if submitting {
            card_children.push(Box::new(
                el::<_, ChromeState, ChromeIntent>("div", "Applying decision")
                    .attr("class", "decision-status")
                    .attr("role", "status"),
            ));
        }
        if let Some(error) = error {
            card_children.push(Box::new(
                el::<_, ChromeState, ChromeIntent>("div", error.clone())
                    .attr("class", "decision-error")
                    .attr("role", "alert"),
            ));
        }
        let left = ((win.w - 620.0) / 2.0).max(8.0);
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", card_children)
                .attr("class", "decision-card")
                .attr("role", "dialog")
                .attr(
                    "style",
                    format!("transform: translate({left}px, 108px); width: 620px;"),
                ),
        ));
    }

    Box::new(el::<_, ChromeState, ChromeIntent>("div", children))
}

fn window_title_bar(state: &ChromeState, win: &WindowChrome, slot: usize) -> ChromeView {
    let scale = if win.display_scale > 0.0 { win.display_scale } else { 1.0 };
    let height = (navigation_height(state.zoom) * scale).max(win.titlebar_insets.height);
    let merged = win.primary && state.shellbar_visible && !state.layout.toolbar_bottom;
    let title: cambium::TitleBarSlot<ChromeState> = Box::new(
        el(
            "div",
            (
                el("span", if merged { "" } else { "Turnstone" })
                    .attr("class", "turnstone-title-name"),
                el(
                    "span",
                    if merged {
                        String::new()
                    } else if win.primary {
                        "Workspace".into()
                    } else {
                        format!("Lens {slot}")
                    },
                )
                .attr("class", "turnstone-title-context"),
            ),
        )
        .attr("class", "turnstone-title-identity"),
    );
    let ornament: cambium::TitleBarSlot<ChromeState> = Box::new(
        el("span", el("span", ()).attr("class", "turnstone-stone-vein"))
            .attr(
                "class",
                if merged && win.w - win.titlebar_insets.left < 300.0 * state.zoom {
                    "turnstone-stone turnstone-stone-hidden"
                } else {
                    "turnstone-stone"
                },
            )
            .attr("aria-hidden", "true"),
    );
    let bar = cambium::title_bar(
        ornament,
        title,
        Box::new(el("span", ())),
        Box::new(el("span", ())),
    );
    // The identity slots are inert; window dragging is interpreted by the
    // native shell from Cambium's computed app-region after retained hit testing.
    let bar = cambium::map_message_result(bar, |_: &mut ChromeState, result| match result {
        cambium::MessageResult::Action(()) | cambium::MessageResult::Nop => {
            cambium::MessageResult::<ChromeIntent>::Nop
        },
        cambium::MessageResult::RequestRebuild => cambium::MessageResult::RequestRebuild,
        cambium::MessageResult::Stale => cambium::MessageResult::Stale,
    });
    Box::new(el::<_, ChromeState, ChromeIntent>("div", bar)
        .attr("class", "turnstone-title-frame")
        .attr("style", format!(
            "position:absolute; transform:translate(0px, 0px); width:{}px; height:{height}px; --titlebar-min-height:{height}px; --titlebar-gap:{}px; {}",
            win.w, 10.0 * state.zoom, win.titlebar_insets.declarations(win.w),
        )))
}

pub(crate) fn titlebar_height(zoom: f32, insets: cambium_rootstock::TitlebarInsets) -> f32 {
    cambium::ShellMetrics::toolbar_height(zoom).max(insets.height)
}

fn browser_button(label: impl Into<String>, intent: Option<ChromeIntent>) -> ChromeView {
    let label = label.into();
    browser_control(label.clone(), &label, intent)
}

fn browser_control(
    label: impl Into<String>,
    name: &str,
    intent: Option<ChromeIntent>,
) -> ChromeView {
    cambium::shell_control(
        label,
        name,
        cambium::ShellControl {
            disabled: intent.is_none(),
            ..Default::default()
        },
        move |_: &mut ChromeState, _: PointerClick| intent,
    )
}

fn selected_button(label: impl Into<String>, intent: ChromeIntent, selected: bool) -> ChromeView {
    let label = label.into();
    cambium::shell_control(
        label.clone(),
        label,
        cambium::ShellControl {
            pressed: Some(selected),
            ..Default::default()
        },
        move |_: &mut ChromeState, _: PointerClick| intent,
    )
}

fn menu_button(state: &ChromeState, label: String, name: &str, menu: ChromeMenu) -> ChromeView {
    cambium::shell_control(
        label,
        name,
        cambium::ShellControl {
            expanded: Some(state.menu == Some(menu)),
            ..Default::default()
        },
        move |state: &mut ChromeState, _: PointerClick| {
            state.menu = if state.menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
            state.menu_selected = 0;
            state.documents_start = 0;
            ChromeIntent::MenuChanged
        },
    )
}

fn activity_button(mark: &str, label: &str, name: &str, intent: ChromeIntent, pressed: Option<bool>) -> ChromeView {
    cambium::shell_activity_control(mark, label, name, cambium::ShellControl { pressed, ..Default::default() },
        move |_: &mut ChromeState, _: PointerClick| intent)
}

/// Shared reserved rows for the retained workspace and the chrome overlay.
/// The control line and its padding grow with the UI zoom.
pub(crate) fn navigation_height(zoom: f32) -> f32 {
    cambium::ShellMetrics::toolbar_height(zoom)
}
fn status_height(zoom: f32) -> f32 {
    12.0 + 10.0 * zoom
}
pub(crate) fn workspace_area(
    chrome: &crate::shell_services::ShellChromeConfig,
    primary: bool,
    width: u32,
    height: u32,
) -> crate::surface::Rect {
    workspace_area_for_window(chrome, if primary { 0 } else { 1 }, width, height)
}

pub(crate) fn workspace_area_for_window(
    chrome: &crate::shell_services::ShellChromeConfig,
    slot: usize,
    width: u32,
    height: u32,
) -> crate::surface::Rect {
    let primary = slot == 0;
    let scale = chrome.display_scale(slot);
    let title = (navigation_height(chrome.appearance.zoom()) * scale)
        .max(chrome.titlebar_insets.get(&slot).copied().unwrap_or_default().height);
    let visible = chrome.projects_shellbar();
    let status = if visible {
        status_height(chrome.appearance.zoom()) * scale
    } else {
        0.0
    };
    let top_status = visible
        && matches!(
            chrome.shellbar.placement,
            ChromePlacement::Docked(ChromeEdge::Top)
        );
    let top = title + if top_status { status } else { 0.0 };
    let bottom = (if top_status { 0.0 } else { status })
        + if visible && primary && chrome.layout.toolbar_bottom {
            navigation_height(chrome.appearance.zoom()) * scale
        } else {
            0.0
        };
    let top = top.min(height as f32);
    let rail = if primary && visible {
        ((if chrome.layout.activity_rail { cambium::ShellMetrics::ACTIVITY_RAIL_WIDTH } else { 0.0 })
            + if chrome.layout.document_rail { cambium::ShellMetrics::DOCUMENT_NAV_WIDTH } else { 0.0 })
            * chrome.appearance.zoom() * scale
    } else { 0.0 };
    let left = rail.min(width as f32);
    crate::surface::Rect::new(
        left,
        top,
        (width as f32 - left).max(0.0),
        (height as f32 - top - bottom).max(0.0),
    )
}

/// Map a configured projection placement to this pre-A4 chrome surface. A
/// `Pane` placement has no station yet, so it deliberately renders nothing
/// rather than fabricating a second pane tree.
pub(crate) fn chrome_position(
    placement: &ChromePlacement,
    width: f32,
    height: f32,
    item_width: f32,
) -> Option<(f32, f32)> {
    let centred = || ((width - item_width) / 2.0).max(8.0);
    match placement {
        ChromePlacement::Overlay | ChromePlacement::Floating => Some((centred(), CARD_TOP)),
        ChromePlacement::Docked(ChromeEdge::Top) => Some((centred(), 8.0)),
        ChromePlacement::Docked(ChromeEdge::Bottom) => Some((centred(), (height - 52.0).max(8.0))),
        ChromePlacement::Docked(ChromeEdge::Left) => Some((8.0, (height * 0.5 - 18.0).max(8.0))),
        ChromePlacement::Docked(ChromeEdge::Right) => Some((
            (width - item_width - 8.0).max(8.0),
            (height * 0.5 - 18.0).max(8.0),
        )),
        ChromePlacement::Pane(_) | ChromePlacement::Hidden => None,
    }
}
/// The per-projection logic: one closure definition (so every projection
/// shares one `Logic` type), instantiated with its window slot.
fn chrome_logic(slot: usize) -> impl FnMut(&ChromeState) -> ChromeView {
    move |state| window_chrome_view(state, slot)
}

type ChromeLogic = Box<dyn FnMut(&ChromeState) -> ChromeView>;

/// The chrome surfaces: one shared document, one forest projection per
/// window. Slot 0 is the primary; a lens's slot is `ordinal + 1`.
pub struct ChromeSurfaces {
    dom: DomHandle,
    runner: GenetMultiRunner<ChromeState, ChromeLogic, ChromeView, ChromeIntent>,
    projections: Vec<ProjectionId>,
    /// One retained layout per window-root, slot-indexed beside `projections`.
    /// Before these existed, every frame and every pointer press re-cascaded
    /// the whole chrome subtree from scratch; with the palette open that was
    /// 36 ms of a 44 ms frame.
    layouts: Vec<crate::ui::RetainedSubtreeLayout>,
    /// The host-owned presentation value applied to the retained DOM during
    /// scene layout and hit testing.
    appearance: crate::shell_services::AppearanceConfig,
}

/// A suggestion row's display text (the same rendering the hand chrome drew).
pub(crate) fn row_text(s: &Suggestion) -> String {
    match s {
        Suggestion::Node { label, host, .. } if host.is_empty() => label.clone(),
        Suggestion::Node { label, host, .. } => format!("{label}  \u{00b7}  {host}"),
        Suggestion::Go { url } => format!("\u{2192} open {url}"),
        // The clock face marks provenance: this row came from what you
        // visited, not from what is on the canvas or what you just typed.
        Suggestion::Recall { url, title, .. } => match title {
            Some(title) if !title.trim().is_empty() => {
                format!("\u{1f552} {title}  \u{00b7}  {url}")
            },
            _ => format!("\u{1f552} {url}"),
        },
        Suggestion::Act { label, .. } => format!("\u{203a} {label}"),
        Suggestion::Hint(hint) => (*hint).to_string(),
        Suggestion::Prompt(prompt) => prompt.clone(),
    }
}

/// The one action row allowed to grow vertically rather than clip.
pub(crate) fn is_install_review(s: &Suggestion) -> bool {
    matches!(
        s,
        Suggestion::Act {
            action: crate::action::Action::ConfirmInstallDenizen,
            ..
        }
    )
}

impl ChromeSurfaces {
    pub(crate) fn selector_point(&self, selector: &taproot::Selector) -> Option<(f32, f32)> {
        let root = self.runner.window_root(*self.projections.first()?)?;
        self.layouts
            .first()?
            .selector_point(&self.dom.borrow(), root, selector)
    }
    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = ChromeState {
            menu: None,
            menu_selected: 0,
            documents_start: 0,
            relationships_start: 0,
            layout: Default::default(),
            map_active: false,
            territory: false,
            tools: Vec::new(),
            connections: Vec::new(),
            documents: Vec::new(),
            reading_stacks: Vec::new(),
            windows: vec![WindowChrome {
                primary: true,
                ..WindowChrome::default()
            }],
            open: false,
            inline_address: false,
            omnibar_input: TextInput::default(),
            rows: Vec::new(),
            omnibar_placement: ChromePlacement::Overlay,
            shellbar_placement: ChromePlacement::Docked(ChromeEdge::Right),
            shellbar_visible: true,
            zoom: 1.0,
            can_back: false,
            can_forward: false,
            focused_node: None,
            focused_kept: false,
            fetching: false,
            fetch_status: None,
            focused_url: None,
            link_preview: None,
            find_open: false,
            find_input: TextInput::default(),
            find_status: String::new(),
            decision: None,
        };
        let mut runner = GenetMultiRunner::new(state);
        let primary =
            runner.push_forest_projection(dom.clone(), Box::new(chrome_logic(0)) as ChromeLogic);
        Self {
            dom,
            runner,
            projections: vec![primary],
            layouts: vec![crate::ui::RetainedSubtreeLayout::new()],
            appearance: crate::shell_services::AppearanceConfig::default(),
        }
    }

    /// Make sure a projection exists for `slot` (a lens window registering).
    pub fn ensure_slot(&mut self, slot: usize) {
        while self.projections.len() <= slot {
            let next = self.projections.len();
            let id = self.runner.push_forest_projection(
                self.dom.clone(),
                Box::new(chrome_logic(next)) as ChromeLogic,
            );
            self.projections.push(id);
            self.layouts.push(crate::ui::RetainedSubtreeLayout::new());
        }
    }

    /// How many full cascades one window-root has paid for.
    ///
    /// The retention contract in one number: it reaches 1 and stays there
    /// while the chrome is merely repainted. Exposed so a receipt can assert
    /// retention rather than infer it from a duration, which would make the
    /// guard a benchmark and therefore flaky.
    pub fn layout_rebuilds(&self, slot: usize) -> u32 {
        self.layouts.get(slot).map_or(0, |layout| layout.rebuilds())
    }

    /// Drain the shared DOM once and stale every root that reads it.
    ///
    /// This is the whole reason [`crate::ui::RetainedSubtreeLayout`] does not
    /// drain for itself. The chrome is N window-roots over ONE `ScriptedDom`,
    /// so a per-root drain would let whichever root ran first consume the
    /// batch and leave the rest painting a tree that no longer exists. Draining
    /// here, and invalidating every root when the batch is non-empty, cannot
    /// produce that split.
    ///
    /// Every entry point that reads the layout calls this, not just `sync`: a
    /// click dispatch mutates the DOM between frames, and a lens window can
    /// render in a frame where the primary did not sync.
    fn absorb_dom_mutations(&mut self) {
        let mut mutations = Vec::new();
        self.dom.borrow_mut().drain_mutations(&mut mutations);
        if mutations.is_empty() {
            return;
        }
        for layout in &mut self.layouts {
            layout.invalidate();
        }
    }

    /// Mirror app truth into the chrome state and rebuild every projection
    /// (one update, every window re-reads — the one-state contract). The
    /// caller passes each live window's size, slot-indexed.
    pub fn sync(&mut self, app: &App, sizes: &[(usize, f32, f32)]) {
        for &(slot, _, _) in sizes {
            self.ensure_slot(slot);
        }
        let omnibar: &OmnibarState = &app.omnibar;
        let mut omnibar_input = TextInput::new(omnibar.presented_text());
        omnibar_input.set_caret_byte(omnibar.presented_cursor(), false);
        if let Some(preedit) = omnibar.presented_preedit() {
            omnibar_input.set_preedit(preedit);
        }
        let chrome = app.shell_chrome_config();
        // Only the `>` lane's command rows carry Keep/Drop.
        let keep_states = (omnibar.open && omnibar.text.trim().starts_with('>'))
            .then(|| app.command_keep_states());
        let rows: Vec<RowView> = omnibar
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| RowView {
                text: row_text(s),
                class: match s {
                    Suggestion::Hint(_) | Suggestion::Prompt(_) => "omni-row-muted",
                    _ if is_install_review(s) && i == omnibar.selected => {
                        "omni-row-sel omni-row-review"
                    },
                    _ if is_install_review(s) => "omni-row omni-row-review",
                    _ if i == omnibar.selected => "omni-row-sel",
                    _ => "omni-row",
                },
                commit: match s {
                    Suggestion::Hint(_) | Suggestion::Prompt(_) => None,
                    _ => Some(i),
                },
                keep: match s {
                    Suggestion::Act { label, action }
                        if !is_install_review(s)
                            && *action != crate::action::Action::OmnibarShowAllCommands =>
                    {
                        keep_states.as_ref().and_then(|states| states.of(label))
                    },
                    _ => None,
                },
            })
            .collect();
        let open = omnibar.open;
        let browser_target = app.browser_command_target();
        let focused = browser_target.as_ref().map(|(member, _)| *member);
        let caption = crate::app::navigation_caption(app);
        let visible = app
            .frisket
            .outer_workspace()
            .ok()
            .map(|frame| frame.active_ids())
            .unwrap_or_default();
        let tools = crate::panes::pane_palette_entries()
            .into_iter()
            .filter_map(|(_, kind)| {
                let definition = crate::panes::pane_definition(kind.as_str())?;
                if matches!(
                    definition.id,
                    crate::panes::kind::GRAPH | crate::panes::kind::WORKBENCH
                ) {
                    return None;
                }
                let shown = app.frisket.iter_leaves().any(|(pane, content, _)| {
                    content.kind_id().as_str() == definition.id
                        && visible.contains(&workbench::TileId(pane.0))
                });
                Some((definition.id, definition.display_name.to_string(), shown))
            })
            .collect::<Vec<_>>();
        let map_active = app
            .active_pane
            .and_then(|pane| match app.pane_content(pane) {
                Some(crate::panes::PaneContent::Orrery) => Some(true),
                Some(crate::panes::PaneContent::Workbench) => Some(false),
                _ => None,
            })
            .unwrap_or_else(|| {
                visible.iter().any(|id| {
                    matches!(
                        app.pane_content(crate::panes::PaneId(id.0)),
                        Some(crate::panes::PaneContent::Orrery)
                    )
                })
            });
        // The document navigator projects graph membership, not open tabs.
        let navigation_graph = app.active_pane.and_then(|pane| {
            let context = if app.has_pane_binding(pane) { app.pane_bound_context(pane) } else {
                app.pane_context.resolve_context(pane, &crate::panes::ContextBinding::Own, crate::panes::SourceSelector::Graph)
            };
            context.filter(|context| context.member == focused).and_then(|context| context.graph)
        }).or_else(|| focused.and_then(|member| app.graph_runtimes.graph_containing_member(member)))
            .or_else(|| app.active_pane.and_then(|pane| app.graph_for_pane(pane)))
            .unwrap_or_else(|| app.graph_runtimes.active_graph());
        let arranged = app
            .frisket
            .iter_leaves()
            .filter(|(_, _, graph)| *graph == navigation_graph)
            .filter_map(|(pane, _, _)| app.workbench_for_pane(pane))
            .flat_map(|workbench| workbench.open_members())
            .collect::<std::collections::HashSet<_>>();
        let reading_stacks = app.primary_graph_for_navigation(focused)
            .and_then(|pane| app.workbench_for_pane(pane))
            .map(|workbench| workbench.slot_views().map(|slot| {
                slot.members.iter().enumerate().map(|(i, member)| {
                    let title = app.graph_runtimes.canvas(navigation_graph)
                        .and_then(|canvas| canvas.graph().get_node_by_id(*member).map(|(key, _)| canvas.graph().node_display_label(key)))
                        .unwrap_or_else(|| "Unavailable document".into());
                    (*member, title, slot.active == i)
                }).collect::<Vec<_>>()
            }).collect::<Vec<_>>()).unwrap_or_default();
        let mut documents = app
            .graph_runtimes
            .canvas(navigation_graph)
            .into_iter()
            .flat_map(|canvas| {
                canvas.graph().nodes().map(|(_, node)| {
                    (
                        node.id,
                        if node.title.trim().is_empty() || node.title == node.url() {
                            node.url().to_string()
                        } else {
                            node.title.clone()
                        },
                        arranged.contains(&node.id),
                    )
                })
            })
            .collect::<Vec<_>>();
        documents.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        let connections = focused.and_then(|member| app.graph_runtimes.canvas(navigation_graph).map(|canvas| {
            crate::relationship_navigation::relationships(navigation_graph, canvas.graph(), member)
        })).unwrap_or_default();
        let focused_kept = focused.is_some_and(|member| app.node_is_kept(member));
        let can_back = focused.is_some_and(|member| app.member_can_back(member));
        let can_forward = focused.is_some_and(|member| app.member_can_forward(member));
        let fetching = focused.is_some_and(|node| app.content.fetch_in_progress(node));
        let fetch_status = focused.and_then(|node| match app.content.fetch_phase(node) {
            Some(crate::content::PageFetchPhase::Requested) => Some("Requested".to_string()),
            Some(crate::content::PageFetchPhase::Streaming { received_bytes, .. }) => {
                Some(format!("Streaming  {received_bytes} bytes"))
            },
            Some(crate::content::PageFetchPhase::Loading { progress_millis }) => {
                Some(match progress_millis {
                    Some(value) => format!("Loading  {}%", value / 10),
                    None => "Loading".to_string(),
                })
            },
            Some(crate::content::PageFetchPhase::Stopped { received_bytes }) => {
                Some(format!("Stopped  {received_bytes} bytes"))
            },
            Some(crate::content::PageFetchPhase::Settled { .. }) | None => None,
        });
        let focused_url = browser_target.map(|(_, url)| url);
        let link_preview = app.link_preview.clone();
        let mut find_input = TextInput::new(app.document_find.query.clone());
        find_input.set_caret_byte(app.document_find.query.len(), false);
        let find_open = app.document_find.open;
        let find_status = app.document_find.status();
        let decision = app.user_agent_decision.active().map(|decision| {
            let request = decision.request();
            let prompt = decision.prompt();
            let queued = app.user_agent_decision.len();
            let submitting = app.user_agent_decision.submitting == Some(request);
            let error = app.user_agent_decision.error.clone();
            match decision {
                crate::user_agent_decision::PendingUserAgentDecision::Permission { .. } => {
                    DecisionCard::Permission {
                        request,
                        prompt,
                        queued,
                        submitting,
                        error,
                    }
                },
                crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. } => {
                    let mut username = TextInput::new(
                        app.user_agent_decision
                            .authentication
                            .username
                            .as_str()
                            .to_string(),
                    );
                    username.set_caret_byte(
                        app.user_agent_decision
                            .authentication
                            .username
                            .as_str()
                            .len(),
                        false,
                    );
                    let password_text = app.user_agent_decision.authentication.password_mask();
                    let mut password = TextInput::new(password_text.clone());
                    password.set_caret_byte(password_text.len(), false);
                    DecisionCard::Authentication {
                        request,
                        prompt,
                        queued,
                        username,
                        password,
                        field: app.user_agent_decision.authentication.field,
                        remember_for_process: app
                            .user_agent_decision
                            .authentication
                            .remember_for_process,
                        submitting,
                        error,
                    }
                },
            }
        });
        self.runner.update(|state| {
            if open
                || decision.is_some()
                || state.layout != chrome.layout
                || state.focused_node != focused
            {
                state.menu = None;
                state.documents_start = 0;
            }
            state.layout = chrome.layout;
            state.tools = tools.clone();
            if state.focused_node != focused || state.connections != connections {
                state.relationships_start = 0;
                if state.menu == Some(ChromeMenu::Connections) { state.menu_selected = 0; }
            }
            state.connections = connections.clone();
            state.documents = documents.clone();
            state.reading_stacks = reading_stacks.clone();
            state.map_active = map_active;
            state.territory = app.graph_runtimes.canvas(navigation_graph).is_some_and(|canvas| canvas.territory_field());
            for &(slot, w, h) in sizes {
                while state.windows.len() <= slot {
                    state.windows.push(WindowChrome::default());
                }
                let win = &mut state.windows[slot];
                win.display_scale = chrome.display_scale(slot);
                win.w = w;
                win.h = h;
                win.caption = caption.clone();
                win.primary = slot == 0;
                win.titlebar_insets = chrome
                    .titlebar_insets
                    .get(&slot)
                    .copied()
                    .unwrap_or_default();
            }
            state.open = open;
            state.inline_address = open
                && matches!(omnibar.mode, crate::ui::OmnibarMode::Address)
                && !omnibar.text.trim_start().starts_with('>')
                && chrome.projects_shellbar();
            state.omnibar_input = omnibar_input.clone();
            state.rows = rows.clone();
            state.omnibar_placement = chrome.omnibar.placement.clone();
            state.shellbar_placement = chrome.shellbar.placement.clone();
            state.shellbar_visible = chrome.projects_shellbar();
            state.zoom = chrome.appearance.zoom();
            state.can_back = can_back;
            state.can_forward = can_forward;
            state.focused_node = focused;
            state.focused_kept = focused_kept;
            state.fetching = fetching;
            state.fetch_status = fetch_status.clone();
            state.focused_url = focused_url.clone();
            state.link_preview = link_preview.clone();
            state.find_open = find_open;
            state.find_input = find_input.clone();
            state.find_status = find_status.clone();
            state.decision = decision.clone();
        });
        self.appearance = chrome.appearance.clone();
        self.absorb_dom_mutations();
    }

    /// One window's chrome scene: ITS window-root laid out at its own size
    /// through a per-root Livery/Buckram subtree cascade — the true forest-dom F2
    /// path. (This replaced the `display: none` visibility flip the chrome
    /// bridged with while the retired genet-stylo sharing cache panicked on
    /// subtree roots; the fix published as 0.19.1 and the flip is gone.)
    pub fn scene(&mut self, slot: usize, w: u32, h: u32) -> netrender::Scene {
        self.absorb_dom_mutations();
        let Some(&id) = self.projections.get(slot) else {
            return netrender::Scene::new(w, h);
        };
        let Some(root) = self.runner.window_root(id) else {
            return netrender::Scene::new(w, h);
        };
        let sheet = crate::ui::chrome_sheet(&{ let mut a = self.appearance.clone(); a.display_scale = self.runner.state().windows.get(slot).map_or(1.0, |w|w.display_scale.max(0.5)); a });
        let dom = self.dom.borrow();
        let Some(layout) = self.layouts.get_mut(slot) else {
            return netrender::Scene::new(w, h);
        };
        layout.scene(&dom, root, &sheet, w, h)
    }

    pub(crate) fn app_region_at(
        &mut self,
        slot: usize,
        x: f32,
        y: f32,
        w: u32,
        h: u32,
    ) -> cambium_rootstock::AppRegion {
        self.absorb_dom_mutations();
        let Some(&id) = self.projections.get(slot) else {
            return Default::default();
        };
        let Some(root) = self.runner.window_root(id) else {
            return Default::default();
        };
        let sheet = crate::ui::chrome_sheet(&{ let mut a = self.appearance.clone(); a.display_scale = self.runner.state().windows.get(slot).map_or(1.0, |w|w.display_scale.max(0.5)); a });
        let dom = self.dom.borrow();
        let Some(layout) = self.layouts.get_mut(slot) else {
            return Default::default();
        };
        layout.app_region_at(&dom, root, &sheet, w, h, x, y)
    }

    /// Route a click at window-local `(x, y)` into `slot`'s chrome: hit-test
    /// that window-root's own subtree layout, dispatch through the runner,
    /// return what bubbled (a row commit).
    pub fn click(&mut self, slot: usize, x: f32, y: f32, w: u32, h: u32) -> Vec<ChromeIntent> {
        let Some(&id) = self.projections.get(slot) else {
            return Vec::new();
        };
        self.absorb_dom_mutations();
        let hit = {
            let Some(root) = self.runner.window_root(id) else {
                return Vec::new();
            };
            let sheet = crate::ui::chrome_sheet(&{ let mut a = self.appearance.clone(); a.display_scale = self.runner.state().windows.get(slot).map_or(1.0, |w|w.display_scale.max(0.5)); a });
            let dom = self.dom.borrow();
            let Some(layout) = self.layouts.get_mut(slot) else {
                return Vec::new();
            };
            layout.hit_test(&dom, root, &sheet, w, h, x, y)
        };
        let had_menu = slot == 0 && self.runner.state().menu.is_some();
        let mut intents = match hit {
            Some(node) => self
                .runner
                .dispatch_click(id, node, PointerClick::at((x, y))),
            None => Vec::new(),
        };
        if intents
            .iter()
            .any(|intent| !matches!(intent, ChromeIntent::MenuChanged))
        {
            self.runner.update(|state| state.menu = None);
        }
        if had_menu && intents.is_empty() {
            self.dismiss_menu();
            intents.push(ChromeIntent::MenuChanged);
        }
        intents
    }

    pub(crate) fn dismiss_menu(&mut self) -> bool {
        if self.runner.state().menu.is_none() {
            return false;
        }
        self.runner.update(|state| state.menu = None);
        true
    }
    pub(crate) fn has_menu(&self) -> bool {
        self.runner.state().menu.is_some()
    }

    /// Menu keys use the same retained buttons and click dispatch as a pointer.
    pub(crate) fn menu_key(&mut self, key: &str) -> Option<Vec<ChromeIntent>> {
        self.runner.state().menu?;
        if key == "escape" {
            self.dismiss_menu();
            return Some(Vec::new());
        }
        if !matches!(key, "up" | "down" | "left" | "right" | "enter") {
            return None;
        }
        // Keyboard activation targets the retained control itself. Measuring
        // every menu row with a separate full cascade made arrow navigation
        // increasingly expensive as the shared shell gained more controls.
        let targets = {
            let dom = self.dom.borrow();
            let root = self.runner.window_root(self.projections[0])?;
            let menu = dom.all_with_class(root, "chrome-menu").into_iter().next()?;
            dom.all_with_class(menu, "shell-control").into_iter().filter(|node| {
                dom.attribute(*node, &layout_dom_api::Namespace::from(""), &layout_dom_api::LocalName::from("aria-disabled")) != Some("true")
            }).collect::<Vec<_>>()
        };
        if targets.is_empty() { return Some(Vec::new()); }
        let selected = self.runner.state().menu_selected.min(targets.len() - 1);
        if key == "enter" {
            let intents = self.runner.dispatch_click(self.projections[0], targets[selected], PointerClick::at((0.0, 0.0)));
            if intents.iter().any(|intent| !matches!(intent, ChromeIntent::MenuChanged)) {
                self.runner.update(|state| state.menu = None);
            }
            return Some(intents);
        }
        let next = if matches!(key, "up" | "left") {
            (selected + targets.len() - 1) % targets.len()
        } else {
            (selected + 1) % targets.len()
        };
        self.runner.update(|state| state.menu_selected = next);
        Some(Vec::new())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use layout_dom_api::LayoutDom;

    use super::*;
    use crate::action::{Action, Update};

    fn toolbar_point(chrome: &ChromeSurfaces, name: &str, w: u32, h: u32) -> (f32, f32) {
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        let node = dom
            .all_with_class(root, "shell-control")
            .into_iter()
            .chain(dom.all_with_class(root, "browser-status"))
            .find(|node| {
                dom.outer_html(*node).starts_with("<button")
                    && dom
                        .outer_html(*node)
                        .contains(&format!("aria-label=\"{name}\""))
            })
            .expect("named toolbar control");
        let (x, y, w, h) = crate::ui::subtree_node_rect(
            &dom,
            root,
            node,
            &crate::ui::chrome_sheet(&chrome.appearance),
            w,
            h,
        )
        .unwrap();
        (x + w / 2.0, y + h / 2.0)
    }

    #[test]
    fn native_density_is_per_window_and_keeps_workspace_reservations_in_step() {
        let mut app=App::test_stub();
        let mut config=app.shell_chrome_config().clone();
        config.native_scales.insert(0,2.0); config.native_scales.insert(1,1.0);
        app.set_shell_chrome_config(config.clone());
        let mut chrome=ChromeSurfaces::new(); chrome.sync(&app,&[(0,1600.,1200.),(1,800.,600.)]);
        let primary=workspace_area_for_window(&config,0,1600,1200);
        let lens=workspace_area_for_window(&config,1,800,600);
        assert!((primary.y-2.0*lens.y).abs()<0.01);
        assert_eq!(chrome.runner.state().windows[0].display_scale,2.0);
        assert_eq!(chrome.runner.state().windows[1].display_scale,1.0);
        assert_eq!(config.appearance.ui_zoom,1.1);
        assert_eq!(config.appearance_for_window(0).ui_zoom,1.1);
        assert_ne!(crate::ui::chrome_sheet(&config.appearance_for_window(0)),crate::ui::chrome_sheet(&config.appearance_for_window(1)));
    }

    #[test]
    fn toolbar_is_default_and_panes_switches_activity_navigation_without_duplicate_controls() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/first".into()));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(!app.shell_chrome_config().layout.activity_rail);
        assert_eq!(workspace_area(app.shell_chrome_config(), true, 1024, 600).x, 0.0);
        assert!(chrome.dom.borrow().all_with_class(chrome.dom.borrow().document(), "turnstone-activity-rail").is_empty());
        let (_, y) = toolbar_point(&chrome, "Read", 1024, 600);
        assert!(y < workspace_area(app.shell_chrome_config(), true, 1024, 600).y);
        let (x,y) = toolbar_point(&chrome, "Panes", 1024, 600);
        chrome.click(0,x,y,1024,600);
        let (x,y) = toolbar_point(&chrome, "Activity rail", 1024, 600);
        let intents = chrome.click(0,x,y,1024,600);
        let [ChromeIntent::Layout(layout)] = intents.as_slice() else { panic!("layout toggle must dispatch through the existing action spine"); };
        assert!(layout.activity_rail);
        app.update(Action::SetChromeLayout(*layout));
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let (x,y) = toolbar_point(&chrome, "Read", 1024, 600);
        let area = workspace_area(app.shell_chrome_config(), true, 1024, 600);
        assert!(x < area.x && y > area.y);
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        assert_eq!(dom.all_with_class(root, "shell-control").into_iter().filter(|node| {
            dom.outer_html(*node).contains("aria-label=\"Read\"")
        }).count(), 1);
    }

    #[test]
    fn rail_tabs_project_each_tile_and_keep_native_content_outside_the_rail() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/first".into()));
        let first = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::ReadGraphMember(first));
        let reader = app.active_pane.unwrap();
        let map = app.default_graph_pane();
        let second = app.with_graph_pane(map, |canvas| canvas.open_member_as_new_node(Some(first), "https://example.test/second")).unwrap();
        app.update(Action::ReadGraphMember(second));
        app.update(Action::WorkbenchSplitOut { dragged: second, axis: crate::action::WbAxis::Row, after: true });
        let before = (app.workbench_for_pane(reader).unwrap().slot_views().map(|slot| (slot.members.to_vec(), slot.weight)).collect::<Vec<_>>(), app.workbench_for_pane(reader).unwrap().weights());
        let mut config = app.shell_chrome_config().clone();
        config.layout.document_rail = true;
        config.layout.activity_rail = true;
        app.set_shell_chrome_config(config);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert_eq!(chrome.runner.state().reading_stacks.len(), 2);
        assert_eq!(chrome.runner.state().reading_stacks.iter().flatten().filter(|(_, _, active)| *active).count(), 2);
        let title = chrome.runner.state().reading_stacks[0][0].1.clone();
        let (x,y) = toolbar_point(&chrome, &title, 1024, 600);
        assert_eq!(chrome.click(0, x, y, 1024, 600), vec![ChromeIntent::ReadMember(first)]);
        app.update(Action::ReadGraphMember(first));
        assert_eq!((app.workbench_for_pane(reader).unwrap().slot_views().map(|slot| (slot.members.to_vec(), slot.weight)).collect::<Vec<_>>(), app.workbench_for_pane(reader).unwrap().weights()), before);
        assert_eq!(app.browser_command_target().unwrap().0, first);
        let area = workspace_area(app.shell_chrome_config(), true, 1024, 600);
        assert_eq!(area.x, (cambium::ShellMetrics::ACTIVITY_RAIL_WIDTH + cambium::ShellMetrics::DOCUMENT_NAV_WIDTH) * app.shell_chrome_config().appearance.zoom());
        assert_eq!(area.x + area.w, 1024.0);
    }

    #[test]
    fn toolbar_is_one_native_row_with_controls_clear_of_caption_buttons_at_all_widths() {
        for width in [320, 420, 800, 1024, 1400] {
            let mut app = App::test_stub();
            app.update(Action::OpenAddress("https://example.test/reading".into()));
            let mut config = app.shell_chrome_config().clone();
            config.titlebar_insets.insert(
                0,
                cambium_rootstock::TitlebarInsets {
                    left: 156.0,
                    right: 0.0,
                    height: 56.0,
                },
            );
            app.set_shell_chrome_config(config);
            let mut chrome = ChromeSurfaces::new();
            chrome.sync(&app, &[(0, width as f32, 600.0)]);
            let point = toolbar_point(&chrome, "Address or search document graph", width, 600);
            assert!(point.0 >= 156.0 && point.0 < width as f32);
            assert!(point.1 <= 56.0);
            assert_eq!(
                chrome.app_region_at(0, point.0, point.1, width, 600),
                cambium_rootstock::AppRegion::NoDrag
            );
            let (px, py) = toolbar_point(&chrome, "Panes", width, 600);
            assert!(px < width as f32 && py <= 56.0);
            assert_eq!(
                workspace_area(app.shell_chrome_config(), true, width, 600).y,
                56.0
            );
        }
    }

    #[test]
    fn panes_territory_toggle_projects_the_current_maps_state() {
        let mut app = browser_history_app(true);
        app.update(Action::SetWorkspaceMap(false));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0,1024.0,600.0)]);
        assert!(!chrome.runner.state().territory);
        let (x,y) = toolbar_point(&chrome,"Panes",1024,600);
        chrome.click(0,x,y,1024,600);
        let (x,y) = toolbar_point(&chrome,"Territory",1024,600);
        assert_eq!(chrome.click(0,x,y,1024,600),vec![ChromeIntent::Territory]);
        app.update(Action::ToggleTerritory);
        chrome.sync(&app,&[(0,1024.0,600.0)]);
        assert!(chrome.runner.state().territory);
    }

    #[test]
    fn panes_menu_dispatches_real_tools_and_keyboard_uses_the_same_retained_buttons() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        app.update(Action::SetWorkspaceMap(false));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let (x, y) = toolbar_point(&chrome, "Panes", 1024, 600);
        assert_eq!(
            chrome.click(0, x, y, 1024, 600),
            vec![ChromeIntent::MenuChanged]
        );
        assert!(chrome.has_menu());
        let first = chrome.runner.state().tools[0].0;
        chrome.menu_key("down");
        chrome.menu_key("down");
        assert_eq!(
            chrome.menu_key("enter").unwrap(),
            vec![ChromeIntent::Tool(first)]
        );
        assert!(!chrome.has_menu());
        let (x, y) = toolbar_point(&chrome, "Panes", 1024, 600);
        chrome.click(0, x, y, 1024, 600);
        assert!(chrome.dismiss_menu());
        assert!(!chrome.has_menu());
    }

    #[test]
    fn relationship_menu_opens_from_panes_without_an_ambiguous_toolbar_target() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/root".into()));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0,360.0,600.0)]);
        let (x,y) = toolbar_point(&chrome,"Panes",360,600);
        chrome.click(0,x,y,360,600);
        let (x,y) = toolbar_point(&chrome,"Browse document connections",360,600);
        assert_eq!(chrome.click(0,x,y,360,600),vec![ChromeIntent::MenuChanged]);
        assert_eq!(chrome.runner.state().menu,Some(ChromeMenu::Connections));
    }

    #[test]
    fn relationship_menu_keeps_direction_and_pages_past_twelve_neighbors() {
        use mere::kernel::graph::{EdgeAssertion, SemanticSubKind, apply::assert_relation};
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/root".into()));
        let pane = app.default_graph_pane();
        let source = app.graph_pane_focused_member(pane).unwrap();
        let mut members = Vec::new();
        for i in 0..15 {
            let member = app.with_graph_pane(pane, |canvas| canvas.open_member_as_new_node(None, &format!("https://example.test/{i:02}"))).unwrap();
            app.with_graph_pane(pane, |canvas| {
                canvas.ingest_graph(|graph| {
                let a = graph.get_node_by_id(source).unwrap().0;
                let b = graph.get_node_by_id(member).unwrap().0;
                assert_relation(graph,a,b,EdgeAssertion::Semantic { sub_kind: SemanticSubKind::Cites,label:None }).unwrap();
                true
                });
            });
            members.push(member);
        }
        app.graph_pane_select_member(pane,source);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app,&[(0,1024.0,700.0)]);
        let (x,y) = toolbar_point(&chrome,"Document connections",1024,700);
        chrome.click(0,x,y,1024,700);
        for _ in 0..2 {
            let (x,y) = toolbar_point(&chrome,"More relationships",1024,700);
            assert_eq!(chrome.click(0,x,y,1024,700),vec![ChromeIntent::MenuChanged]);
        }
        assert_eq!(chrome.runner.state().relationships_start,12);
        let row = chrome.runner.state().connections.last().unwrap().clone();
        let (x,y) = toolbar_point(&chrome,&format!("{} · {}: {}",row.relationship,row.address,row.title),1024,700);
        assert_eq!(chrome.click(0,x,y,1024,700),vec![ChromeIntent::NavigateRelationship(row.target)]);
        assert_eq!(row.target.member,*members.last().unwrap());
        assert_eq!(row.relationship,"Cites");
        assert_eq!(app.graph_runtimes.graph().nodes().count(),16);
    }

    #[test]
    fn toolbar_connections_come_from_graph_edges_and_keep_member_identity() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        let a = app.graph_runtimes.focused_member().unwrap();
        let pane = app.default_graph_pane();
        let b = app
            .with_graph_pane(pane, |canvas| {
                canvas.open_member_as_new_node(Some(a), "https://example.test/b")
            })
            .unwrap();
        assert_eq!(app.graph_pane_focused_member(pane), Some(b));
        assert!(
            app.graph_runtimes
                .graph()
                .projected_relations()
                .next()
                .is_some(),
            "fixture has a held relationship"
        );
        app.update(Action::SetWorkspaceMap(false));
        assert_eq!(app.browser_command_target().map(|(id, _)| id), Some(b));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(
            chrome
                .runner
                .state()
                .connections
                .iter()
                .any(|row| row.target.member == a)
        );
        app.update(Action::ReadGraphMember(a));
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(
            chrome
                .runner
                .state()
                .connections
                .iter()
                .any(|row| row.target.member == b)
        );
        assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
    }

    #[test]
    fn graph_navigation_lists_unarranged_members_and_opens_their_existing_identity() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/reading".into()));
        let first = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::ReadGraphMember(first));
        let map = app.default_graph_pane();
        let other = app
            .with_graph_pane(map, |canvas| {
                canvas.open_member_as_new_node(Some(first), "https://example.test/unarranged")
            })
            .unwrap();
        app.update(Action::WorkbenchActivate(first));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(
            chrome
                .runner
                .state()
                .documents
                .iter()
                .any(|(id, _, arranged)| *id == other && !arranged)
        );
        let (x, y) = toolbar_point(&chrome, "Documents in this map", 1024, 600);
        chrome.click(0, x, y, 1024, 600);
        let label = chrome
            .runner
            .state()
            .documents
            .iter()
            .find(|(id, _, _)| *id == other)
            .unwrap()
            .1
            .clone();
        let (x, y) = toolbar_point(&chrome, &label, 1024, 600);
        assert_eq!(
            chrome.click(0, x, y, 1024, 600),
            vec![ChromeIntent::ReadMember(other)]
        );
        app.update(Action::ReadGraphMember(other));
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(
            chrome
                .runner
                .state()
                .documents
                .iter()
                .any(|(id, _, arranged)| *id == other && *arranged)
        );
        assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
    }

    #[test]
    fn graph_navigation_can_reach_members_beyond_the_first_menu_page() {
        let mut app = App::test_stub();
        for index in 0..19 {
            app.update(Action::OpenAddress(format!(
                "https://example.test/document-{index:02}"
            )));
        }
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let last = chrome.runner.state().documents.last().unwrap().clone();
        let (x, y) = toolbar_point(&chrome, "Documents in this map", 1024, 600);
        chrome.click(0, x, y, 1024, 600);
        for _ in 0..2 {
            let (x, y) = toolbar_point(&chrome, "More documents", 1024, 600);
            chrome.click(0, x, y, 1024, 600);
        }
        let (x, y) = toolbar_point(&chrome, &last.1, 1024, 600);
        assert!(
            x < 1024.0 && y < 600.0,
            "later members remain within the menu viewport"
        );
        let (x, y) = toolbar_point(&chrome, "Previous documents", 1024, 600);
        chrome.click(0, x, y, 1024, 600);
        let earlier = chrome.runner.state().documents[8].clone();
        let (x, y) = toolbar_point(&chrome, &earlier.1, 1024, 600);
        assert!(y < 600.0 && x < 1024.0);
        let (x, y) = toolbar_point(&chrome, "More documents", 1024, 600);
        chrome.click(0, x, y, 1024, 600);
        let (x, y) = toolbar_point(&chrome, &last.1, 1024, 600);
        assert_eq!(
            chrome.click(0, x, y, 1024, 600),
            vec![ChromeIntent::ReadMember(last.0)]
        );
    }
    #[test]
    fn shared_title_bar_respects_native_controls_and_drag_regions() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".into()));
        let mut config = app.shell_chrome_config().clone();
        config.appearance.ui_zoom = 1.0;
        config.titlebar_insets.insert(
            0,
            cambium_rootstock::TitlebarInsets {
                left: 156.0,
                right: 0.0,
                height: 56.0,
            },
        );
        config.titlebar_insets.insert(
            2,
            cambium_rootstock::TitlebarInsets {
                left: 78.0,
                right: 0.0,
                height: 28.0,
            },
        );
        app.set_shell_chrome_config(config);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0), (2, 640.0, 480.0)]);
        for (slot, width, height, leading, expected_height) in
            [(0, 1024, 600, 156.0, 56.0), (2, 640, 480, 78.0, 40.0)]
        {
            let (ornament, title) = {
                let dom = chrome.dom.borrow();
                let root = chrome.runner.window_root(chrome.projections[slot]).unwrap();
                let sheet = crate::ui::chrome_sheet(&chrome.appearance);
                let node = |class| dom.all_with_class(root, class)[0];
                if slot != 0 {
                    assert!(
                        dom.outer_html(node("turnstone-title-name"))
                            .contains("Turnstone")
                    );
                }
                let rect = |class| {
                    crate::ui::subtree_node_rect(&dom, root, node(class), &sheet, width, height)
                        .unwrap()
                };
                (
                    rect("turnstone-stone"),
                    rect(if slot == 0 {
                        "turnstone-stone"
                    } else {
                        "turnstone-title-name"
                    }),
                )
            };
            assert!(ornament.0 >= leading, "ornament must clear native captions");
            assert!(title.0 >= leading);
            let point = (title.0 + title.2 / 2.0, title.1 + title.3 / 2.0);
            assert_eq!(
                chrome.app_region_at(slot, point.0, point.1, width, height),
                cambium_rootstock::AppRegion::Drag
            );
            assert!(
                chrome
                    .click(slot, point.0, point.1, width, height)
                    .is_empty(),
                "identity is inert"
            );
            let area = workspace_area_for_window(app.shell_chrome_config(), slot, width, height);
            assert_eq!(area.y, expected_height);
        }
        // The adjacent navigation row uses ordinary controls, outside the
        // shared title bar's inherited drag region.
        assert_eq!(
            chrome.app_region_at(0, 20.0, 80.0, 1024, 600),
            cambium_rootstock::AppRegion::NoDrag
        );
        // Authored descendants can explicitly opt out of the inherited drag
        // region without changing the window bridge or its hit geometry.
        let mut config = app.shell_chrome_config().clone();
        config.appearance.theme_presentation =
            Some(tabard::ThemePresentation::AuthoredStylesheet(vec![
                ".turnstone-stone { --app-region: no-drag; }".into(),
            ]));
        app.set_shell_chrome_config(config);
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let point = {
            let dom = chrome.dom.borrow();
            let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
            let title = dom.all_with_class(root, "turnstone-stone")[0];
            let rect = crate::ui::subtree_node_rect(
                &dom,
                root,
                title,
                &crate::ui::chrome_sheet(&chrome.appearance),
                1024,
                600,
            )
            .unwrap();
            (rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0)
        };
        assert_eq!(
            chrome.app_region_at(0, point.0, point.1, 1024, 600),
            cambium_rootstock::AppRegion::NoDrag
        );
    }

    #[test]
    fn navigation_and_context_reserve_rows_in_the_actual_frame() {
        for (width, height, zoom, edge) in [
            (1024, 600, 1.0, ChromeEdge::Left),
            (420, 400, 1.0, ChromeEdge::Right),
            (800, 600, 2.0, ChromeEdge::Bottom),
            (1024, 600, 1.5, ChromeEdge::Top),
        ] {
            let mut app = App::test_stub();
            app.update(Action::OpenAddress("mere://alpha".into()));
            let mut config = app.shell_chrome_config().clone();
            config.appearance.ui_zoom = zoom;
            config.shellbar.placement = ChromePlacement::Docked(edge);
            app.set_shell_chrome_config(config);
            let config = app.shell_chrome_config();
            let area = workspace_area(config, true, width, height);
            let mut chrome = ChromeSurfaces::new();
            chrome.sync(&app, &[(0, width as f32, height as f32)]);
            let dom = chrome.dom.borrow();
            let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let rect = |class| {
                let node = dom.all_with_class(dom.document(), class)[0];
                crate::ui::subtree_node_rect(&dom, root, node, &sheet, width, height).unwrap()
            };
            let (nx, ny, nw, nh) = rect("browser-strip");
            let (_, cy, _, ch) = rect("whereami");
            assert!(nx >= 0.0 && nx + nw <= width as f32 + 0.1);
            assert!(
                ny + nh <= area.y + 0.1,
                "navigation must end before the frame"
            );
            if edge == ChromeEdge::Top {
                assert!(cy >= ny + nh - 0.1);
                assert!(cy + ch <= area.y + 0.1);
            } else {
                assert!(cy >= area.y + area.h - 0.1);
                assert!((cy + ch - height as f32).abs() < 0.1);
            }
            let mut frame = crate::outer_frame::OuterFrame::default();
            frame.set_workspace_area(area);
            let bodies = frame
                .prepare(
                    &app.frisket,
                    app.active_pane,
                    None,
                    (width, height),
                    &config.appearance,
                    app.session_id.as_uuid().as_u128(),
                )
                .unwrap();
            assert!(!bodies.is_empty());
            for (_, body) in bodies {
                assert!(body.y >= area.y);
                assert!(body.y + body.h <= area.y + area.h + 0.1);
            }
            assert_eq!(frame.header_height(), 0.0,
                "the main Read/Map switch replaces the singleton frame header");
            assert_eq!(frame.selector_point(&taproot::Selector::class("frisket-tab")).unwrap(), None);
            let (_, body) = frame.prepare(&app.frisket, app.active_pane, None,
                (width, height), &config.appearance,
                app.session_id.as_uuid().as_u128()).unwrap()[0];
            assert_eq!(body, area, "the body uses exactly the space reserved below navigation");
        }
    }

    #[test]
    fn hidden_browser_chrome_keeps_only_the_title_row() {
        let mut config = crate::shell_services::ShellChromeConfig::default();
        config.shellbar.visible = false;
        assert_eq!(
            workspace_area(&config, true, 1024, 600),
            crate::surface::Rect::new(
                0.0,
                titlebar_height(config.appearance.zoom(), Default::default()),
                1024.0,
                600.0 - titlebar_height(config.appearance.zoom(), Default::default())
            )
        );
        config.shellbar.visible = true;
        let lens = workspace_area(&config, false, 800, 500);
        assert_eq!(
            lens.y,
            titlebar_height(config.appearance.zoom(), Default::default()),
            "lenses reserve only the title row above content"
        );
        assert!(lens.h < 500.0, "lenses keep context outside their content");
        let tiny = workspace_area(&config, true, 100, 20);
        assert!(tiny.h >= 0.0 && tiny.y <= 20.0);
    }

    #[test]
    fn admitted_document_title_reaches_tabs_and_the_active_context() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress(format!(
            "mere://clip/{}",
            "ab".repeat(32)
        )));
        let member = app.graph_runtimes.focused_member().unwrap();
        let effects = app.apply_update(Update::ContentSpawned {
            node: member,
            facts: Some(crate::content::ContentFacts {
                engine: "retained-clip".into(),
                lane: crate::content::ContentLane::Document,
                structure: Some(crate::content::StructureFacts {
                    title: Some("Knot selection".into()),
                    headings: 1,
                    links: 0,
                    outline: vec![],
                }),
                lineage: None,
                capabilities: Default::default(),
            }),
        });
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, crate::action::Effect::SaveSession))
        );
        app.update(Action::OpenInWorkbench);
        let pane = app.active_pane.unwrap();
        let mut workbench = crate::workbench_pane::WorkbenchPane::new();
        workbench.sync(&app, pane, 800.0, 500.0);
        let dom = workbench.dom_ref();
        assert!(dom.outer_html(dom.document()).contains("Knot selection"));
        drop(dom);
        app.update(Action::OpenAddress("mere://other".into()));
        app.update(Action::WorkbenchActivate(member));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        let context = dom.all_with_class(dom.document(), "whereami")[0];
        assert!(dom.outer_html(context).contains("Knot selection"));
    }

    fn open_omnibar_app() -> App {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".to_string()));
        app.update(Action::OmnibarOpen { command: true });
        app.update(Action::OmnibarChar('r'));
        app
    }

    fn long_install_review_app() -> (App, String) {
        let watch_url = format!(
            "https://watch.example/{}",
            "averyverylongunbrokenwatchtarget".repeat(6)
        );
        let mut app = App::test_stub();
        app.viewport = (1024.0, 320.0);
        app.pending_install = Some(crate::denizen::PendingInstall {
            path: std::path::PathBuf::from("watcher.lua"),
            label: "watcher".into(),
            body: crate::denizen::PackBody::Scenario("mere.snapshot()".into()),
            subject: servitor::Subject::new([7; 32]),
            rings: crate::denizen::default_rings(),
            watch_url: Some(watch_url.clone()),
            deadband: None,
        });
        app.update(Action::OmnibarOpen { command: true });
        (app, watch_url)
    }

    fn browser_history_app(settled: bool) -> App {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("gemini://capsule.test/a".into()));
        let node = app.graph_runtimes.focused_member().expect("focused node");
        let request = app.content.active_fetch(node).expect("initial fetch");
        assert!(app.content.settle_fetch(node, request));
        app.update(Action::ContentNavigationCommitted {
            member: node,
            url: "gemini://capsule.test/b".into(),
        });
        app.update(Action::ContentNavigationCommitted {
            member: node,
            url: "gemini://capsule.test/c".into(),
        });
        app.update(Action::NavBack);
        if settled {
            let request = app.content.active_fetch(node).expect("Back fetch");
            assert!(app.content.settle_fetch(node, request));
        }
        app
    }

    fn browser_button_intents(chrome: &mut ChromeSurfaces) -> Vec<ChromeIntent> {
        let rects = {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            dom.all_with_class(dom.document(), "shell-control")
                .into_iter()
                .filter(|button| {
                    let html = dom.outer_html(*button);
                    [
                        "Back",
                        "Forward",
                        "Reload",
                        "Stop",
                        "Keep document",
                        "Allow once",
                        "Always allow",
                        "Deny",
                    ]
                    .iter()
                    .any(|name| html.contains(&format!("aria-label=\"{name}\"")))
                })
                .map(|button| {
                    crate::ui::subtree_node_rect(&dom, root, button, &sheet, 1024, 600)
                        .expect("browser button has a rect")
                })
                .collect::<Vec<_>>()
        };
        rects
            .into_iter()
            .flat_map(|(x, y, w, h)| chrome.click(0, x + w / 2.0, y + h / 2.0, 1024, 600))
            .collect()
    }

    #[test]
    fn browser_strip_routes_per_node_back_forward_and_reload() {
        let app = browser_history_app(true);
        assert!(app.focused_can_back() && app.focused_can_forward());
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert_eq!(
            browser_button_intents(&mut chrome),
            vec![
                ChromeIntent::NavBack,
                ChromeIntent::NavForward,
                ChromeIntent::Reload,
                ChromeIntent::KeepNode(app.graph_runtimes.focused_member().expect("focused node")),
            ]
        );
    }

    #[test]
    fn active_fetch_switches_reload_to_stop_and_projects_link_preview() {
        let mut app = browser_history_app(false);
        app.set_link_preview(Some("gemini://capsule.test/prospective".into()));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        {
            let dom = chrome.dom.borrow();
            assert_eq!(
                dom.all_with_class(dom.document(), "browser-loading").len(),
                1
            );
            assert_eq!(dom.all_with_class(dom.document(), "link-preview").len(), 1);
        }
        assert_eq!(
            browser_button_intents(&mut chrome),
            vec![
                ChromeIntent::NavBack,
                ChromeIntent::NavForward,
                ChromeIntent::Stop,
                ChromeIntent::KeepNode(app.graph_runtimes.focused_member().expect("focused node")),
            ]
        );
    }

    #[test]
    fn document_find_projects_controlled_input_status_and_controls() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("gemini://capsule.test/find".into()));
        let node = app.graph_runtimes.focused_member().expect("focused node");
        app.content.note_live(
            node,
            Some(crate::content::ContentFacts {
                engine: "test.surface".into(),
                lane: crate::content::ContentLane::Surface,
                structure: None,
                lineage: None,
                capabilities: crate::content::DocumentCapabilityFacts {
                    find_in_page: crate::content::CapabilityStatus::Supported,
                    ..Default::default()
                },
            }),
        );
        app.update(Action::OpenDocumentFind);
        app.document_find.query = "needle".into();
        app.document_find.model = crate::action::DocumentFindModel {
            count: 1,
            matches: vec![crate::action::DocumentFindMatch {
                label: "Needle heading".into(),
                role: Some("heading".into()),
            }],
            current: Some(0),
            complete: true,
        };

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        assert_eq!(dom.all_with_class(dom.document(), "document-find").len(), 1);
        assert_eq!(
            dom.all_with_class(dom.document(), "read-query")
                .len(),
            1
        );
        assert_eq!(
            dom.all_with_class(dom.document(), "read-result")
                .len(),
            1
        );
        let find = dom.all_with_class(dom.document(), "document-find")[0];
        let enabled = dom.all_with_class(find, "shell-control").len();
        let disabled = dom.all_with_class(find, "browser-button-disabled").len();
        assert_eq!(enabled + disabled, 3, "find owns previous, next, and close");
    }

    #[test]
    fn permission_decision_projects_origin_choices_and_accessibility() {
        let mut app = App::test_stub();
        let node = uuid::Uuid::new_v4();
        let id = inker::UserAgentRequestId::new(41);
        app.apply_update(Update::PermissionRequested {
            node,
            id,
            origin: "https://maps.example/path".into(),
            descriptors: vec![inker::PermissionDescriptor::Geolocation],
        });

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let intents = browser_button_intents(&mut chrome);
        assert!(intents.contains(&ChromeIntent::Permission {
            request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
            choice: crate::user_agent_decision::PermissionChoice::AllowOnce,
        }));
        assert!(intents.contains(&ChromeIntent::Permission {
            request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
            choice: crate::user_agent_decision::PermissionChoice::AlwaysAllow,
        }));
        assert!(intents.contains(&ChromeIntent::Permission {
            request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
            choice: crate::user_agent_decision::PermissionChoice::Deny,
        }));
        let dom = chrome.dom.borrow();
        assert_eq!(dom.all_with_class(dom.document(), "decision-card").len(), 1);
        assert!(dom.outer_html(dom.document()).contains("maps.example"));
        drop(dom);
        let a11y = crate::a11y::a11y_lines(&app);
        assert!(a11y.iter().any(|line| line == "button: Always allow"));
        assert!(
            a11y.iter()
                .any(|line| { line == "dialog: https://maps.example wants location" })
        );
    }

    #[test]
    fn authentication_card_masks_password_and_a11y_withholds_credentials() {
        let mut app = App::test_stub();
        let node = uuid::Uuid::new_v4();
        app.apply_update(Update::AuthenticationRequested {
            node,
            id: inker::UserAgentRequestId::new(42),
            host: "secure.example".into(),
            port: 443,
            realm: Some("private".into()),
            scheme: "basic".into(),
            is_proxy: false,
        });
        app.update(Action::InsertAuthentication("private-user".into()));
        app.update(Action::FocusAuthenticationField(
            crate::user_agent_decision::AuthenticationField::Password,
        ));
        app.update(Action::InsertAuthentication("private-password".into()));

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        let html = dom.outer_html(dom.document());
        assert!(html.contains("private-user"));
        assert!(!html.contains("private-password"));
        assert!(html.contains("\u{2022}"));
        drop(dom);
        let a11y = crate::a11y::a11y_lines(&app).join("\n");
        assert!(a11y.contains("textinput: Username"));
        assert!(a11y.contains("passwordinput: Password"));
        assert!(!a11y.contains("private-user"));
        assert!(!a11y.contains("private-password"));
    }

    #[test]
    fn authentication_card_debug_withholds_credential_fields() {
        let card = DecisionCard::Authentication {
            request: crate::user_agent_decision::UserAgentRequestKey::new(
                uuid::Uuid::new_v4(),
                inker::UserAgentRequestId::new(43),
            ),
            prompt: "Sign in".into(),
            queued: 1,
            username: TextInput::new("private-user"),
            password: TextInput::new("private-password"),
            field: crate::user_agent_decision::AuthenticationField::Username,
            remember_for_process: false,
            submitting: false,
            error: None,
        };
        let debug = format!("{card:?}");
        assert!(!debug.contains("private-user"));
        assert!(!debug.contains("private-password"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn browser_strip_targets_the_workbench_member_without_moving_graph_selection() {
        let mut app = crate::app::App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        let a = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);
        app.update(Action::OpenAddress("https://example.test/b".into()));
        let b = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);
        app.update(Action::WorkbenchActivate(a));
        // Settle the fixture fetches so the strip displays the page URL,
        // rather than the higher-priority in-flight fetch status.
        for member in [a, b] {
            let request = app.content.active_fetch(member).unwrap();
            assert!(app.content.settle_fetch(member, request));
        }
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let intents = browser_button_intents(&mut chrome);
        assert!(intents.contains(&ChromeIntent::KeepNode(a)));
        assert!(!intents.contains(&ChromeIntent::KeepNode(b)));
        let dom = chrome.dom.borrow();
        let status = dom.all_with_class(dom.document(), "browser-status");
        assert_eq!(status.len(), 1);
        assert!(dom.outer_html(status[0]).contains("https://example.test/a"));
        assert_eq!(app.graph_runtimes.focused_member(), Some(b));
    }

    #[test]
    fn keep_button_targets_the_member_then_projects_kept_state() {
        let mut app = browser_history_app(true);
        let member = app.graph_runtimes.focused_member().expect("focused node");
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(browser_button_intents(&mut chrome).contains(&ChromeIntent::KeepNode(member)));

        app.update(Action::KeepNode { member });
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(app.node_is_kept(member));
        assert!(!browser_button_intents(&mut chrome).contains(&ChromeIntent::KeepNode(member)));
        let dom = chrome.dom.borrow();
        assert_eq!(
            dom.all_with_class(dom.document(), "shell-control")
                .into_iter()
                .filter(|&node| {
                    dom.attribute(node, &"".into(), &"aria-label".into())
                        .is_some_and(|value| value.to_string() == "Keep document")
                        && dom.attribute(node, &"".into(), &"aria-disabled".into())
                            .is_some_and(|value| value.to_string() == "true")
                })
                .count(),
            1,
            "the one-way control remains visible as disabled Kept state"
        );
    }

    #[test]
    fn hidden_shellbar_is_removed_from_the_chrome_and_a11y_projections() {
        let mut app = open_omnibar_app();
        let mut config = app.shell_chrome_config().clone();
        config.shellbar.visible = false;
        app.set_shell_chrome_config(config);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        assert!(dom.all_with_class(dom.document(), "whereami").is_empty());
        drop(dom);
        assert!(
            !crate::a11y::a11y_lines(&app)
                .iter()
                .any(|line| line.starts_with("label: ")),
            "the hidden shellbar cannot leave an inaccessible caption behind"
        );
    }

    #[test]
    fn live_appearance_is_applied_to_the_retained_chrome_surface() {
        let mut app = open_omnibar_app();
        let mut config = app.shell_chrome_config().clone();
        config.appearance.theme_id = Some("theme:night".into());
        config.appearance.theme_mode = crate::shell_services::ThemeMode::Light;
        config.appearance.ui_zoom = 1.5;
        app.set_shell_chrome_config(config);

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert_eq!(chrome.appearance, app.shell_chrome_config().appearance);
        assert!(
            crate::ui::chrome_sheet(&chrome.appearance).contains("24.00px"),
            "the retained chrome is laid out at the live zoom"
        );
        assert!(
            !chrome.scene(0, 1024, 600).ops.is_empty(),
            "the live appearance stylesheet drives the rendered chrome surface"
        );
    }

    /// The forest topology: primary + a lens chrome are sibling window-roots
    /// of ONE document, and each window's scene lays out only ITS subtree —
    /// the omnibar card exists under the primary root and not the lens's.
    #[test]
    fn chrome_windows_are_forest_siblings_with_scoped_layout() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0), (1, 800.0, 500.0)]);
        let dom = chrome.dom.borrow();
        let doc = dom.document();
        let roots: Vec<_> = dom.dom_children(doc).collect();
        assert_eq!(roots.len(), 2, "two window-roots under one document");
        // The omnibar card renders under the PRIMARY root only.
        let cards = dom.all_with_class(doc, "omni");
        assert_eq!(cards.len(), 1, "one omnibar card in the whole forest");
        let chips = dom.all_with_class(doc, "whereami");
        assert_eq!(chips.len(), 2, "both windows carry the caption chip");
        drop(dom);
        // Scoped scenes: the primary's has content; the lens's renders too
        // (its chip), from its own root at its own size.
        let primary = chrome.scene(0, 1024, 600);
        let lens = chrome.scene(1, 800, 500);
        assert!(!primary.ops.is_empty(), "primary chrome paints");
        assert!(!lens.ops.is_empty(), "lens chrome paints its chip");
    }

    /// A suggestion-row click bubbles its ORIGINAL index — the new capability
    /// the retained view adds over the hand chrome.
    #[test]
    fn clicking_a_suggestion_row_bubbles_its_index() {
        let app = open_omnibar_app();
        assert!(
            !app.omnibar.suggestions.is_empty(),
            "the '>r' query filters the registry to something"
        );
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        // Resolve the first selectable row's centre off the laid-out chrome
        // (the primary root's OWN subtree layout, the per-window pass the
        // shell runs).
        let (x, y) = {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let row = dom
                .all_with_class(dom.document(), "omni-row-sel")
                .into_iter()
                .next()
                .expect("a selected row is drawn");
            let (rx, ry, rw, rh) = crate::ui::subtree_node_rect(&dom, root, row, &sheet, 1024, 600)
                .expect("row has a rect");
            (rx + rw / 2.0, ry + rh / 2.0)
        };
        let intents = chrome.click(0, x, y, 1024, 600);
        assert_eq!(intents, vec![ChromeIntent::CommitRow(0)]);
    }

    /// A command row's label commits it and its own control keeps or drops
    /// it, each with the row's original index.
    #[test]
    fn a_command_rows_keep_control_is_its_own_target() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let centre_of = |chrome: &ChromeSurfaces, class: &str| {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let node = dom
                .all_with_class(dom.document(), class)
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("a {class} is drawn"));
            let (x, y, w, h) = crate::ui::subtree_node_rect(&dom, root, node, &sheet, 1024, 600)
                .expect("it has a rect");
            (x + w / 2.0, y + h / 2.0)
        };
        let (kx, ky) = centre_of(&chrome, "omni-keep");
        let (lx, ly) = centre_of(&chrome, "omni-row-label");
        assert!(kx > lx, "the control sits after the label");
        assert_eq!(
            chrome.click(0, kx, ky, 1024, 600),
            vec![ChromeIntent::ToggleKeep(0)]
        );
        assert_eq!(
            chrome.click(0, lx, ly, 1024, 600),
            vec![ChromeIntent::CommitRow(0)]
        );
    }

    /// The actual retained Confirm row, shared with the staged-install test.
    pub(crate) fn assert_install_review_layout(app: &App, expected_review: &str) -> f32 {
        let review = app
            .omnibar
            .suggestions
            .iter()
            .find(|suggestion| is_install_review(suggestion))
            .expect("the pending install review is visible");
        let expected = row_text(review);
        assert_eq!(
            expected,
            format!("\u{203a} {expected_review}"),
            "the displayed ask is complete"
        );
        assert!(
            crate::a11y::a11y_lines(app)
                .iter()
                .any(|line| line == &format!("button: {expected}")),
            "accessibility carries the same complete review"
        );

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(app, &[(0, 1024.0, 320.0)]);
        let (x, y, row_height) = {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let row = dom
                .all_with_class(dom.document(), "omni-row-review")
                .into_iter()
                .next()
                .expect("the review has its wrapping class");
            let mut nodes = vec![row];
            let mut displayed = String::new();
            while let Some(node) = nodes.pop() {
                if let Some(text) = dom.text(node) {
                    displayed.push_str(text);
                }
                nodes.extend(dom.dom_children(node).collect::<Vec<_>>().into_iter().rev());
            }
            assert_eq!(
                displayed, expected,
                "the retained DOM preserves the whole ask"
            );
            let (rx, ry, rw, rh) = crate::ui::subtree_node_rect(&dom, root, row, &sheet, 1024, 320)
                .expect("review has a rect");
            assert!(
                rw <= CARD_W,
                "the wrapped review stays within the card: {rw}px"
            );
            assert!(
                rh.is_finite() && rh > 0.0,
                "the review has visible height: {rh}px"
            );

            let card = dom
                .all_with_class(dom.document(), "omni")
                .into_iter()
                .next()
                .expect("the omnibar card exists");
            let (cx, cy, cw, ch) =
                crate::ui::subtree_node_rect(&dom, root, card, &sheet, 1024, 320)
                    .expect("card has a rect");
            assert!(
                rx >= cx && ry >= cy && rx + rw <= cx + cw && ry + rh <= cy + ch,
                "the whole review remains inside the card: row=({rx}, {ry}, {rw}, {rh}) card=({cx}, {cy}, {cw}, {ch})"
            );
            assert!(
                cy + ch <= 320.0 - 16.0,
                "the row budget leaves the expanded card inside the viewport: top={cy}px card={ch}px review={rh}px rows={} extra={}px",
                app.omnibar.suggestions.len(),
                crate::ui::chrome_review_row_extra_height(&expected, &chrome.appearance)
            );
            (rx + rw / 2.0, ry + rh / 2.0, rh)
        };
        assert_eq!(
            chrome.click(0, x, y, 1024, 320),
            vec![ChromeIntent::CommitRow(0)],
            "the expanded row remains one clickable review"
        );
        row_height
    }

    #[test]
    fn install_review_wraps_intact_and_keeps_card_and_click_geometry() {
        let (app, watch_url) = long_install_review_app();
        let review = app
            .omnibar
            .suggestions
            .iter()
            .find(|suggestion| is_install_review(suggestion))
            .expect("the pending install review is visible");
        let expected = row_text(review);
        assert!(
            crate::ui::chrome_row_width(&expected) > crate::ui::ROW_TEXT_BUDGET,
            "the fixture must actually overflow an ordinary one-line row"
        );
        assert!(
            expected.contains(&watch_url),
            "the displayed review is intact"
        );
        assert!(
            crate::a11y::a11y_lines(&app)
                .iter()
                .any(|line| line == &format!("button: {expected}")),
            "accessibility carries the same complete review"
        );

        let pending = app.pending_install.as_ref().expect("the install is staged");
        let review_text = crate::denizen::review_line(pending);
        let row_height = assert_install_review_layout(&app, &review_text);
        assert!(
            row_height > 44.0,
            "emergency breaking takes the URL beyond the two natural URL lines: {row_height}px"
        );
    }

    /// The caret split mirrors the omnibar state: before/caret/after and the
    /// preedit ride the retained DOM (the receipt-proven IME honesty, kept).
    #[test]
    fn the_caret_split_mirrors_omnibar_state() {
        let mut app = App::test_stub();
        app.update(Action::OmnibarOpen { command: false });
        for c in "abd".chars() {
            app.update(Action::OmnibarChar(c));
        }
        app.update(Action::OmnibarCaret(crate::action::CaretMove::Left));
        app.omnibar.preedit = Some("c".to_string());
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        let input = dom
            .all_with_class(dom.document(), "omni-input")
            .into_iter()
            .next()
            .expect("the input line is drawn");
        let texts: Vec<String> = dom
            .dom_children(input)
            .flat_map(|c| {
                let mut out = Vec::new();
                if let Some(t) = dom.text(c) {
                    out.push(t.to_string());
                }
                out.extend(
                    dom.dom_children(c)
                        .filter_map(|g| dom.text(g).map(str::to_string)),
                );
                out
            })
            .collect();
        assert_eq!(texts, ["ab", "c", "|", "d"],
            "committed prefix, IME preedit, font-safe caret, committed suffix");
    }

    /// The retention contract: a chrome that nothing has changed re-paints
    /// without re-cascading. This is the unit-level guard for the command
    /// palette open-lag receipt, which measured the unretained path at 36 ms
    /// of a 44 ms frame.
    #[test]
    fn an_unchanged_chrome_paints_without_cascading_again() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);

        assert!(!chrome.scene(0, 1024, 600).ops.is_empty());
        assert_eq!(chrome.layout_rebuilds(0), 1, "the first frame cascades");

        // Idle frames: sync still runs (the shell calls it every frame), but
        // it writes no mutations, so nothing is invalidated.
        for _ in 0..5 {
            chrome.sync(&app, &[(0, 1024.0, 600.0)]);
            assert!(!chrome.scene(0, 1024, 600).ops.is_empty());
        }
        assert_eq!(
            chrome.layout_rebuilds(0),
            1,
            "five idle frames must not re-cascade"
        );

        // A resize is a per-root fact the layout detects for itself.
        assert!(!chrome.scene(0, 800, 600).ops.is_empty());
        assert_eq!(chrome.layout_rebuilds(0), 2, "a resize re-cascades");
    }

    #[test]
    fn chrome_selectors_refuse_unpainted_menu_geometry_even_after_a_shared_drain() {
        let app = App::test_stub();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        let selector = taproot::Selector::role("button").with_attr("aria-label", "Panes");
        let (x, y) = chrome.selector_point(&selector).unwrap();
        assert_eq!(
            chrome.click(0, x, y, 1024, 600),
            vec![ChromeIntent::MenuChanged]
        );
        assert_eq!(chrome.runner.state().menu, Some(ChromeMenu::Panes));
        assert!(
            chrome.selector_point(&selector).is_none(),
            "a menu transition must be painted before resolving another click"
        );
        let mut consumed = Vec::new();
        chrome.dom.borrow_mut().drain_mutations(&mut consumed);
        assert!(!consumed.is_empty());
        assert!(
            chrome.selector_point(&selector).is_none(),
            "consuming the mutation batch does not make old fragments current"
        );
        chrome.scene(0, 1024, 600);
        let (x, y) = chrome.selector_point(&selector).unwrap();
        assert_eq!(chrome.layout_rebuilds(0), 2);
        assert_eq!(
            chrome.click(0, x, y, 1024, 600),
            vec![ChromeIntent::MenuChanged]
        );
        assert_eq!(chrome.runner.state().menu, None);
    }

    /// A state change reaches the layout. The retention must not be so eager
    /// that a changed chrome keeps painting the old cascade.
    #[test]
    fn a_changed_chrome_cascades_again() {
        let mut app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        assert_eq!(chrome.layout_rebuilds(0), 1);

        app.update(Action::OmnibarChar('e'));
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        assert_eq!(
            chrome.layout_rebuilds(0),
            2,
            "an edited omnibar re-cascades"
        );
    }

    /// The forest hazard, stated as a test: N window-roots share ONE DOM and
    /// therefore ONE mutation queue. A per-root drain would let whichever root
    /// rendered first swallow the batch, leaving every other window painting a
    /// tree that no longer exists — stale, not merely slow. The drain lives in
    /// `absorb_dom_mutations` for exactly this reason, and this asserts that a
    /// single change reaches BOTH roots.
    #[test]
    fn one_mutation_invalidates_every_window_root() {
        let mut app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        let sizes = [(0, 1024.0, 600.0), (1, 800.0, 500.0)];

        chrome.sync(&app, &sizes);
        chrome.scene(0, 1024, 600);
        chrome.scene(1, 800, 500);
        assert_eq!(chrome.layout_rebuilds(0), 1);
        assert_eq!(chrome.layout_rebuilds(1), 1);

        // The primary renders FIRST, which is precisely the ordering that
        // would hide a per-root drain bug.
        app.update(Action::OmnibarChar('e'));
        chrome.sync(&app, &sizes);
        chrome.scene(0, 1024, 600);
        chrome.scene(1, 800, 500);
        assert_eq!(chrome.layout_rebuilds(0), 2, "the primary re-cascaded");
        assert_eq!(
            chrome.layout_rebuilds(1),
            2,
            "the lens re-cascaded too, rather than losing the batch to the primary"
        );
    }

    /// A pointer press hit-tests the layout the frame painted, and pays no
    /// cascade to do it. Before retention this path rebuilt the whole subtree
    /// on every press, including presses that miss chrome entirely.
    #[test]
    fn a_press_hit_tests_without_cascading_again() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        assert_eq!(chrome.layout_rebuilds(0), 1);

        // A press well away from any chrome control: it still hit-tests, and
        // it must not provoke a cascade to answer "nothing here".
        let intents = chrome.click(0, 5.0, 590.0, 1024, 600);
        assert!(
            intents.is_empty(),
            "a press on empty chrome commits nothing"
        );
        assert_eq!(
            chrome.layout_rebuilds(0),
            1,
            "hit testing shares the frame's layout instead of building its own"
        );
    }
}
