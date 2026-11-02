// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn append_window_navigation(
    state: &ChromeState,
    win: &WindowChrome,
    slot: usize,
    children: &mut Vec<ChromeView>,
) {
    let zoom = state.zoom;

    let navigation_height = navigation_height(state.zoom);
    let status_height = status_height(state.zoom);
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
        controls.push(cambium::scope_navigation::scope_controls(
            state.scope_controls.clone(),
            scope_chrome_intent,
        ));
        if wide {
            let name = state
                .scope_controls
                .down
                .as_ref()
                .map(|label| format!("Peek below: {label}"))
                .unwrap_or_else(|| "Peek below".into());
            controls.push(browser_control(
                "Peek",
                &name,
                state
                    .peek_below
                    .clone()
                    .map(|t| ChromeIntent::FocusCard(crate::focus_card::Command::Peek(t))),
            ));
            controls.push(browser_control(
                "Layout",
                "Preview reading layout",
                state
                    .peek_forme
                    .clone()
                    .map(|t| ChromeIntent::FocusCard(crate::focus_card::Command::Peek(t))),
            ));
        }
        if !compact {
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
        }
        let field = controls.pop();
        // A regular-width native toolbar also carries Keep/Documents/Related.
        // Reserve its location slot for the current destination; the complete
        // ancestry returns with the wider toolbar and stays reachable via Up.
        let crumbs = state
            .scope_crumbs
            .iter()
            .filter(|crumb| wide || crumb.current)
            .cloned()
            .collect();
        let location =
            cambium::scope_navigation::scope_location(crumbs, field, scope_chrome_intent);
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
            for (id, label) in [
                (crate::panes::kind::INSPECTOR, "Inspect"),
                (crate::panes::kind::TRAIL, "Trail"),
            ] {
                let visible = state
                    .tools
                    .iter()
                    .any(|(kind, _, visible)| *kind == id && *visible);
                controls.push(selected_button(label, ChromeIntent::Tool(id), visible));
            }
        }
        controls.push(menu_button(
            state,
            if tiny { "⋯".into() } else { "Panes".into() },
            "Panes",
            ChromeMenu::Panes,
        ));
        let left_padding = leading + if merged && !tiny { 38.0 * zoom } else { 6.0 };
        children.push(Box::new(el::<_, ChromeState, ChromeIntent>("div", cambium::shell_toolbar("Document graph navigation", leading_controls, location, controls))
            .attr("class", if state.fetching { "browser-strip browser-loading" } else { "browser-strip" })
            .attr("style", format!("transform:translate({left_padding}px,{navigation_top}px);width:{}px;height:{height}px;padding-left:0px;--app-region:{};",(win.w-left_padding).max(0.0), if merged { "drag" } else { "no-drag" }))));

        let top_status = matches!(
            state.shellbar_placement,
            ChromePlacement::Docked(ChromeEdge::Top)
        );
        let rail_top = title_height + if top_status { status_height } else { 0.0 };
        let rail_bottom = if top_status { 0.0 } else { status_height }
            + if state.layout.toolbar_bottom {
                navigation_height
            } else {
                0.0
            };
        let rail_height = (win.h - rail_top - rail_bottom).max(0.0);
        let rail_width = if state.layout.activity_rail {
            cambium::ShellMetrics::ACTIVITY_RAIL_WIDTH * zoom
        } else {
            0.0
        };
        if state.layout.activity_rail {
            let projections = vec![
                activity_button(
                    "▤",
                    "Read",
                    "Read",
                    ChromeIntent::WorkspaceMap(false),
                    Some(!state.map_active),
                ),
                activity_button(
                    "◇",
                    "Map",
                    "Map",
                    ChromeIntent::WorkspaceMap(true),
                    Some(state.map_active),
                ),
            ];
            let mut tools = Vec::new();
            for (mark, label, id) in [
                ("ⓘ", "Inspect", crate::panes::kind::INSPECTOR),
                ("↶", "Trail", crate::panes::kind::TRAIL),
            ] {
                let visible = state
                    .tools
                    .iter()
                    .any(|(kind, _, visible)| *kind == id && *visible);
                tools.push(activity_button(
                    mark,
                    label,
                    label,
                    ChromeIntent::Tool(id),
                    Some(visible),
                ));
            }
            tools.push(activity_button(
                "⌕",
                "Search",
                "Search document graph",
                ChromeIntent::Address,
                None,
            ));
            tools.push(activity_button(
                "▥",
                "Tabs",
                "Reading tabs",
                ChromeIntent::Layout(crate::chrome_layout::ChromeLayout {
                    document_rail: !state.layout.document_rail,
                    ..state.layout
                }),
                Some(state.layout.document_rail),
            ));
            let utilities = vec![activity_button(
                "⌖",
                "Fit",
                "Fit graph",
                ChromeIntent::FitGraph,
                None,
            )];
            children.push(Box::new(el("div", cambium::shell_activity_rail(projections, tools, utilities))
            .attr("class", "turnstone-activity-rail")
            .attr("style", format!("position:absolute;left:0px;top:{rail_top}px;width:{rail_width}px;height:{rail_height}px;--shell-rail-width:{rail_width}px;--shell-activity-size:{}px;--shell-activity-mark-size:{}px;--shell-activity-caption-size:{}px;", 46.0*zoom,18.0*zoom,10.0*zoom))));
        }
        if state.layout.document_rail {
            let groups = if state.reading_stacks.is_empty() {
                vec![(
                    "Reading tiles".into(),
                    vec![browser_button("No arranged documents", None)],
                )]
            } else {
                state
                    .reading_stacks
                    .iter()
                    .enumerate()
                    .map(|(i, stack)| {
                        (
                            format!("Tile {}", i + 1),
                            stack
                                .iter()
                                .map(|(member, title, active)| {
                                    selected_button(
                                        title,
                                        ChromeIntent::ReadMember(*member),
                                        *active,
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect()
            };
            let width = cambium::ShellMetrics::DOCUMENT_NAV_WIDTH * zoom;
            children.push(Box::new(el("div", cambium::shell_document_rail(groups))
                .attr("class", "turnstone-reading-tabs")
                .attr("style", format!("position:absolute;left:{rail_width}px;top:{rail_top}px;width:{width}px;height:{rail_height}px;--shell-document-width:{width}px;"))));
        }

        append_navigation_menu(
            state,
            win,
            NavigationGeometry {
                zoom,
                title_height,
                navigation_height,
                navigation_top,
                height,
            },
            children,
        );

        // Menus and decisions own this overlay column while open. The deck's
        // bindings stay retained, but cannot paint over or intercept commands.
        if context_cards_visible(state) {
            let cards = state
                .focus_cards
                .iter()
                .map(|card| {
                    let related = card
                        .related
                        .iter()
                        .filter(|_| card.swatch.is_none() && card.forme.is_none())
                        .map(|(target, title, detail)| {
                            let target = target.clone();
                            cambium::shell_navigation_item(
                                title.clone(),
                                detail.clone(),
                                Default::default(),
                                move |_: &mut ChromeState, _| {
                                    ChromeIntent::FocusCard(crate::focus_card::Command::Peek(
                                        target.clone(),
                                    ))
                                },
                            )
                        })
                        .collect();
                    let target = card.target.clone();
                    let body: ChromeView = if let Some(swatch) = &card.forme {
                        let targets = card.related.clone();
                        cambium::forme_swatch(swatch, move |_: &mut ChromeState, member| {
                            targets
                                .iter()
                                .find(|(t, _, _)| t.member == Some(member))
                                .map(|(t, _, _)| {
                                    ChromeIntent::FocusCard(crate::focus_card::Command::Read(
                                        t.clone(),
                                    ))
                                })
                                .unwrap_or(ChromeIntent::MenuChanged)
                        })
                    } else if let Some(swatch) = &card.swatch {
                        let targets = card.related.clone();
                        Box::new(cambium::graph_canvas_swatch(
                            swatch,
                            move |_: &mut ChromeState, member| {
                                targets
                                    .iter()
                                    .find(|(t, _, _)| t.member == Some(member))
                                    .map(|(t, _, _)| {
                                        ChromeIntent::FocusCard(crate::focus_card::Command::Peek(
                                            t.clone(),
                                        ))
                                    })
                                    .unwrap_or(ChromeIntent::MenuChanged)
                            },
                            |_: &mut ChromeState, _| {},
                            |_: &mut ChromeState| ChromeIntent::MenuChanged,
                        ))
                    } else {
                        Box::new(el(
                            "div",
                            card.info
                                .excerpt
                                .clone()
                                .map(|text| el("p", text).attr("class", "focus-card-excerpt")),
                        ))
                    };
                    cambium::focus_card_with_body(
                        card.info.clone(),
                        card.kept,
                        body,
                        related,
                        move |_: &mut ChromeState, command| {
                            use crate::focus_card::Command as F;
                            use cambium::FocusCardCommand as C;
                            ChromeIntent::FocusCard(match command {
                                C::KeepOpen => F::Keep(target.clone()),
                                C::Close => F::Close(target.clone()),
                                C::Primary => F::Read(target.clone()),
                                C::Secondary => F::Map(target.clone()),
                            })
                        },
                    )
                })
                .collect::<Vec<_>>();
            let width = (360.0 * zoom).min((win.w - 24.0).max(0.0));
            children.push(Box::new(on_click(el::<_,ChromeState,ChromeIntent>("div",cards)
                .attr("class","focus-card-deck chrome-focus-cards")
                .attr("style",format!("position:absolute;transform:translate({}px,{}px);width:{width}px;max-height:{}px;overflow-y:auto;",(win.w-width-12.0).max(0.0),title_height+12.0,(win.h-title_height-status_height-24.0).max(0.0))), |_: &mut ChromeState,_|ChromeIntent::MenuChanged)));
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
}
