// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn authoring_view(state: &AuthoringState) -> AuthoringView {
    let mut styles = state
        .editor
        .highlights()
        .into_iter()
        .map(|span| StyleRange {
            range: span.range,
            class: format!("syntax-{:?}", span.kind).to_ascii_lowercase(),
        })
        .collect::<Vec<_>>();
    let selection = state.editor.selection();
    if selection.anchor.byte != selection.focus.byte {
        let range = selection.anchor.byte.min(selection.focus.byte)
            ..selection.anchor.byte.max(selection.focus.byte);
        // Styled fields give smaller ranges precedence. Cover syntax runs
        // inside the selection as well as the otherwise unstyled passage.
        let overlaps = styles
            .iter()
            .filter_map(|style| {
                let start = range.start.max(style.range.start);
                let end = range.end.min(style.range.end);
                (start < end).then_some(StyleRange {
                    range: start..end,
                    class: "knot-selection".into(),
                })
            })
            .collect::<Vec<_>>();
        styles.push(StyleRange {
            range,
            class: "knot-selection".into(),
        });
        styles.extend(overlaps);
    }
    let field = lens(
        move |input: &mut cambium::TextInput| styled_textarea(input, &styles),
        |state: &mut AuthoringState| state.editor.input_mut(),
    );
    let status = match state.status {
        AuthoringStatus::Unavailable => "unavailable".to_string(),
        AuthoringStatus::Revoked => "closed".to_string(),
        AuthoringStatus::Stale => "stale; reload or resolve".to_string(),
        AuthoringStatus::Saving => "saving".to_string(),
        AuthoringStatus::Resolving => "resolving".to_string(),
        AuthoringStatus::Running => "running".to_string(),
        AuthoringStatus::Reloading => "reloading".to_string(),
        AuthoringStatus::Rejected => state.detail.clone(),
        AuthoringStatus::Current if state.editor.is_dirty() => "unsaved".to_string(),
        AuthoringStatus::Current => "saved".to_string(),
    };
    let outline = state
        .editor
        .outline()
        .into_iter()
        .map(|item| {
            format!(
                "{}{}",
                "  ".repeat(item.level.saturating_sub(1) as usize),
                item.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let fold_count = state.editor.folds().len();
    let title = state
        .editor
        .source()
        .lines()
        .next()
        .unwrap_or("Knot document")
        .to_string();
    let preview = match &state.derived {
        Some(derived) => KnotEditor::scratch("knot:derived", derived.source.clone())
            .preview()
            .map(|document| format!("{}\n\n{}", derived_status(derived), document.to_markdown()))
            .unwrap_or_else(|error| format!("Derived preview unavailable: {error}")),
        None => state
            .editor
            .preview()
            .map(|document| document.to_markdown())
            .unwrap_or_else(|error| format!("Preview unavailable: {error}")),
    };
    let toolbar = el::<_, AuthoringState, ()>(
        "div",
        (
            el("div", title).attr("class", "knot-title"),
            el("div", status).attr("class", "knot-status"),
            button(
                "Save",
                |state: &mut AuthoringState, _click: PointerClick| {
                    state.requests.push(AuthoringRequest::Save);
                },
            )
            .attr("class", "knot-save")
            .attr(
                "data-projection-instance",
                state.projection_target.0.to_string(),
            )
            .attr("data-projection-intent", EDITABLE_TEXT_SAVE_INTENT)
            // Not merely disabled: an unreachable holder has no save to
            // offer, and a button that cannot work is worse than none.
            .attr(
                "style",
                if matches!(state.status, AuthoringStatus::Unavailable) {
                    "display: none;"
                } else {
                    ""
                },
            ),
            button(
                "Resolve",
                |state: &mut AuthoringState, _click: PointerClick| {
                    if state.resolve_available {
                        state.requests.push(AuthoringRequest::Resolve);
                    }
                },
            )
            .attr("class", "knot-effect knot-resolve")
            .attr(
                "data-projection-instance",
                state.projection_target.0.to_string(),
            )
            .attr("data-projection-intent", KNOT_TRANSCLUSION_RESOLVE_INTENT)
            .attr(
                "style",
                if state.resolve_available {
                    ""
                } else {
                    "display: none;"
                },
            ),
            button("Run", |state: &mut AuthoringState, _click: PointerClick| {
                if state.run_available {
                    state.requests.push(AuthoringRequest::Run);
                }
            })
            .attr("class", "knot-effect knot-run")
            .attr(
                "data-projection-instance",
                state.projection_target.0.to_string(),
            )
            .attr("data-projection-intent", KNOT_BLOCK_RUN_INTENT)
            .attr(
                "style",
                if state.run_available {
                    ""
                } else {
                    "display: none;"
                },
            ),
            button(
                "Reload",
                |state: &mut AuthoringState, _click: PointerClick| {
                    state.requests.push(AuthoringRequest::Reload);
                },
            )
            .attr("class", "knot-reload"),
        ),
    )
    .attr("class", "knot-toolbar");
    let intent_receipt = state.last_intent.as_ref().map(|receipt| {
        el::<_, AuthoringState, ()>("div", receipt.line())
            .attr("class", "knot-intent-receipt")
            .attr("data-projection-instance", receipt.target.0.to_string())
            .attr("data-projection-intent", receipt.intent.clone())
            .attr("data-intent-result", receipt.result_word())
    });
    let mut citations: Vec<AuthoringView> = Vec::new();
    match crate::knot_saved_citations::controls(state.editor.source()) {
        Ok(controls) => {
            let expected_source = blake3::hash(state.editor.source().as_bytes());
            for (label, address) in controls {
                citations.push(Box::new(
                    button(
                        label,
                        move |state: &mut AuthoringState, _click: PointerClick| {
                            state.requests.push(AuthoringRequest::OpenSavedCitation {
                                address: address.clone(),
                                expected_source,
                            });
                        },
                    )
                    .attr("class", "knot-saved-citation"),
                ));
            }
        },
        Err(error) => citations.push(Box::new(el::<_, AuthoringState, ()>(
            "div",
            format!("Saved citations unavailable: {error}"),
        ))),
    }
    let editor = el::<_, AuthoringState, ()>("div", field).attr("class", "knot-editor-wrap");
    let readout = el::<_, AuthoringState, ()>(
        "div",
        format!("Outline\n{outline}\n\nFolds: {fold_count}\n\nPreview\n{preview}"),
    )
    .attr("class", "knot-readout");
    Box::new(
        el::<_, AuthoringState, ()>(
            "div",
            (
                toolbar,
                intent_receipt,
                el("div", citations).attr("class", "knot-citations"),
                el("div", (editor, readout)).attr("class", "knot-body"),
            ),
        )
        .attr("class", "knot-root")
        .attr(
            "style",
            format!("width: {}px; height: {}px;", state.width, state.height),
        ),
    )
}
