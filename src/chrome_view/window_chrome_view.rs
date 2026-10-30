// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn window_chrome_view(state: &ChromeState, slot: usize) -> ChromeView {
    let Some(win) = state.windows.get(slot).cloned() else {
        return Box::new(el::<_, ChromeState, ChromeIntent>("div", ()));
    };
    let zoom = state.zoom;
    let mut children: Vec<ChromeView> = Vec::new();
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
    children.push(window_title_bar(state, &win, slot));
    if win.primary && field_controls_visible(state) {
        for entry in state.forme_fields.get(&slot).into_iter().flatten() {
            let target = entry.target.clone();
            if let Some(control) =
                cambium::scope_field_controls(&entry.field, move |_: &mut ChromeState, command| {
                    ChromeIntent::FocusCard(match command {
                        cambium::ScopeFieldCommand::Peek => {
                            crate::focus_card::Command::Peek(target.clone())
                        },
                        cambium::ScopeFieldCommand::Enter => {
                            crate::focus_card::Command::EnterLayout(target.clone())
                        },
                    })
                })
            {
                children.push(control);
            }
        }
    }

    append_window_navigation(state, &win, slot, &mut children);

    if win.primary && state.find.open {
        // The shared find field with its shared key map; the shell dispatches
        // the window's key events into it (`ChromeSurfaces::find_key`).
        let find_children = vec![cambium::find_row(
            cambium::find_field::<ChromeState, ChromeIntent, _>(
                &state.find,
                std::rc::Rc::new(|state: &mut ChromeState, event: cambium::ReadEvent| {
                    // Keys that arrive before the next sync edit the input
                    // this one produced.
                    if let cambium::ReadEvent::Query(input) = &event {
                        state.find.input = input.clone();
                    }
                    Some(ChromeIntent::Find(event))
                }),
            ),
            std::rc::Rc::new(|_: &mut ChromeState, command| {
                Some(ChromeIntent::Find(cambium::ReadEvent::Command(command)))
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
            el::<_, ChromeState, ChromeIntent>("div", state.focused_url.clone().or_else(||state.fetch_status.clone()).unwrap_or_default())
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
            card_children.push(Box::new(status_note::<ChromeState, ChromeIntent>(
                "decision-status",
                "Applying decision",
            )));
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
