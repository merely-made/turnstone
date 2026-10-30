// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn inspector_pane_view(state: &InspectorState) -> InspectorView {
    let sections: Vec<DetailSection> = state
        .sections
        .iter()
        .map(|s| {
            DetailSection::new(
                s.title.clone(),
                s.rows
                    .iter()
                    .map(|(k, v)| DetailRow::new(k.clone(), v.clone()))
                    .collect(),
            )
        })
        .collect();
    let clip_label = match (&state.clip_target, state.clip_offer.is_some()) {
        (Some(target), true) => format!(
            "Clip {} to {target}",
            if state.saved_passage {
                "saved passage"
            } else {
                "document"
            }
        ),
        (None, _) => "Clip passage".into(),
        (Some(_), false) => "Clip unavailable; see status below".into(),
    };
    let clip_status =
        el::<_, InspectorState, InspectorIntent>("div", format!("Passage: {}", state.clip_status))
            .attr("class", "detail-value");
    let offered_clip = state.clip_offer;
    let clip_button = button(
        clip_label,
        move |state: &mut InspectorState, _click: PointerClick| {
            state
                .clip_target
                .as_ref()
                .and(offered_clip)
                .map(|offer| InspectorIntent::ClipToKnot { offer })
        },
    )
    .attr(
        "class",
        if state.clip_target.is_some() && state.clip_offer.is_some() {
            "list-row action"
        } else {
            "list-row muted"
        },
    )
    .attr(
        "aria-disabled",
        if state.clip_target.is_some() && state.clip_offer.is_some() {
            "false"
        } else {
            "true"
        },
    )
    .attr("style", "white-space: normal; overflow-wrap: anywhere;");
    // Bind the offered source, rather than consulting the possibly retargeted
    // pane when a queued activation is finally delivered.
    let source_target = state.source_target.clone();
    let source_button = button(
        "Save source and keep",
        move |_state: &mut InspectorState, _click: PointerClick| {
            source_target
                .clone()
                .map(|target| InspectorIntent::CaptureSourceDocument { target })
        },
    )
    .attr(
        "class",
        if state.source_target.is_some() {
            "list-row action"
        } else {
            "list-row muted"
        },
    )
    .attr(
        "aria-disabled",
        if state.source_target.is_some() {
            "false"
        } else {
            "true"
        },
    );
    let source_status = el::<_, InspectorState, InspectorIntent>(
        "div",
        format!(
            "Source save: {}{}",
            state.source_status, state.source_unavailable
        ),
    )
    .attr("class", "detail-value")
    .attr("style", "white-space: normal; overflow-wrap: anywhere;");
    let viewer_options = state.viewer_options.clone();
    let viewer = map_message_result(
        lens(
            move |radio: &mut RadioGroup| {
                radio_options(radio, &viewer_radio_options(&viewer_options))
            },
            |state: &mut InspectorState| &mut state.radio,
        ),
        |_state, message| match message {
            cambium::MessageResult::Action(()) | cambium::MessageResult::Nop => {
                cambium::MessageResult::<InspectorIntent>::Nop
            },
            cambium::MessageResult::RequestRebuild => cambium::MessageResult::RequestRebuild,
            cambium::MessageResult::Stale => cambium::MessageResult::Stale,
        },
    );
    let current_viewer = state
        .viewer_options
        .get(state.radio.selected)
        .map(|option| option.label.as_str())
        .unwrap_or("Unavailable saved viewer");
    Box::new(
        el::<_, InspectorState, InspectorIntent>(
            "div",
            (
                crate::pane_binding::binding_view(
                    &state.binding,
                    |state: &mut InspectorState| &mut state.binding,
                    InspectorIntent::Binding,
                ),
                button(
                    format!("Viewer: {current_viewer}"),
                    |state: &mut InspectorState, _| {
                        state.viewer_expanded = !state.viewer_expanded;
                        None::<InspectorIntent>
                    },
                )
                .attr("aria-label", "Change viewer")
                .attr(
                    "aria-expanded",
                    if state.viewer_expanded {
                        "true"
                    } else {
                        "false"
                    },
                )
                .attr("class", "shell-control"),
                el("div", viewer).attr(
                    "style",
                    if state.viewer_expanded {
                        ""
                    } else {
                        "display:none;"
                    },
                ),
                detail_panel(&sections),
                source_button,
                source_status,
                button("Research tools", |state: &mut InspectorState, _| {
                    state.research_expanded = !state.research_expanded;
                    None::<InspectorIntent>
                })
                .attr(
                    "aria-expanded",
                    if state.research_expanded {
                        "true"
                    } else {
                        "false"
                    },
                )
                .attr("class", "shell-control"),
                el(
                    "div",
                    (
                        el(
                            "p",
                            state
                                .clip_target
                                .as_ref()
                                .map(|target| format!("Research note: {target}"))
                                .unwrap_or_else(|| {
                                    "Choose a research note to keep passages.".into()
                                }),
                        )
                        .attr("class", "detail-value"),
                        el(
                            "div",
                            (
                                button("New research note", |_: &mut InspectorState, _| {
                                    InspectorIntent::NewResearchNote
                                }),
                                button("Open research note", |state: &mut InspectorState, _| {
                                    state
                                        .clip_target
                                        .as_ref()
                                        .map(|_| InspectorIntent::OpenResearchNote)
                                })
                                .attr(
                                    "style",
                                    if state.clip_target.is_some() {
                                        ""
                                    } else {
                                        "display:none;"
                                    },
                                ),
                                {
                                    let choice = state.note_choice.clone();
                                    button(
                                        choice
                                            .as_ref()
                                            .map(|choice| choice.use_label())
                                            .unwrap_or_else(|| {
                                                "Use this document as research note".into()
                                            }),
                                        move |_: &mut InspectorState, _| {
                                            choice.clone().map(|choice| {
                                                InspectorIntent::ChooseResearchNote { choice }
                                            })
                                        },
                                    )
                                    .attr(
                                        "style",
                                        if state.note_choice.is_some() {
                                            ""
                                        } else {
                                            "display:none;"
                                        },
                                    )
                                },
                            ),
                        )
                        .attr("style", "display:flex;flex-wrap:wrap;gap:4px;"),
                        el("div", (clip_status, clip_button)).attr(
                            "style",
                            if state.clip_target.is_some() {
                                ""
                            } else {
                                "display:none;"
                            },
                        ),
                    ),
                )
                .attr(
                    "style",
                    if state.research_expanded {
                        ""
                    } else {
                        "display:none;"
                    },
                ),
            ),
        )
        .attr("class", "pane cambium-shell-panel")
        .attr(
            "style",
            format!(
                "width:{}px;height:{}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}
