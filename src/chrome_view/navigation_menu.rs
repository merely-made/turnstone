// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn append_navigation_menu(
    state: &ChromeState,
    win: &WindowChrome,
    geometry: NavigationGeometry,
    children: &mut Vec<ChromeView>,
) {
    let NavigationGeometry {
        zoom,
        title_height,
        navigation_height,
        navigation_top,
        height,
    } = geometry;
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
                rows.push(selected_button(
                    "Territory",
                    ChromeIntent::Territory,
                    state.territory,
                ));
                rows.push(selected_button(
                    "Activity rail",
                    ChromeIntent::Layout(crate::chrome_layout::ChromeLayout {
                        activity_rail: !state.layout.activity_rail,
                        ..state.layout
                    }),
                    state.layout.activity_rail,
                ));
                rows.push(selected_button(
                    "Reading tabs",
                    ChromeIntent::Layout(crate::chrome_layout::ChromeLayout {
                        document_rail: !state.layout.document_rail,
                        ..state.layout
                    }),
                    state.layout.document_rail,
                ));
                rows.push(browser_button("Fit graph", Some(ChromeIntent::FitGraph)));
                rows.push(browser_control(
                    "Preview lower scope",
                    "Preview lower scope",
                    state
                        .peek_below
                        .clone()
                        .map(|t| ChromeIntent::FocusCard(crate::focus_card::Command::Peek(t))),
                ));
                rows.push(browser_control(
                    "Preview reading layout",
                    "Preview reading layout",
                    state
                        .peek_forme
                        .clone()
                        .map(|t| ChromeIntent::FocusCard(crate::focus_card::Command::Peek(t))),
                ));
                rows.push(browser_control(
                    "Peek selection",
                    "Peek selection",
                    state
                        .peek_selected
                        .clone()
                        .map(|t| ChromeIntent::FocusCard(crate::focus_card::Command::Peek(t))),
                ));
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
                for (label, bottom) in [("Toolbar at top", false), ("Toolbar at bottom", true)] {
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
                        let target = state
                            .relation_targets
                            .iter()
                            .find(|(key, _)| *key == row.target)
                            .map(|(_, t)| t.clone());
                        cambium::shell_navigation_item(
                            row.title.clone(),
                            mere::navigation::relationship_detail(row),
                            Default::default(),
                            move |_: &mut ChromeState, _| {
                                target
                                    .clone()
                                    .map(|t| {
                                        ChromeIntent::FocusCard(crate::focus_card::Command::Peek(t))
                                    })
                                    .unwrap_or(ChromeIntent::MenuChanged)
                            },
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
                420.0
            } else {
                360.0
            },
        );
        let row_count = if menu == ChromeMenu::Panes {
            rows.len().div_ceil(2)
        } else {
            rows.len()
        };
        let menu_height = if menu == ChromeMenu::Connections {
            row_count as f32 * (38.0 + 6.0 * zoom) + 16.0
        } else {
            row_count as f32 * (22.0 + 6.0 * zoom) + 16.0
        };
        let menu_height =
            menu_height.min((win.h - title_height - navigation_height - 12.0).max(0.0));
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
}
