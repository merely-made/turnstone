// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A private strength draft bound to one coupling, with acknowledged saved history.
use super::product::CouplingOperation;
use super::*;
use cambium::{TextInput, button, lens, text_field_typed};
use mere::coupling_edit::StrengthDraft;

pub(super) struct Pane {
    draft: Option<StrengthDraft>,
    input: TextInput,
    busy: bool,
    reveal: bool,
    preview: Option<String>,
    status: String,
}
impl Default for Pane {
    fn default() -> Self {
        Self {
            draft: None,
            input: TextInput::new(""),
            busy: false,
            reveal: false,
            preview: None,
            status: String::new(),
        }
    }
}
impl Pane {
    fn stage(&mut self) -> Result<(), String> {
        self.draft
            .as_mut()
            .ok_or("Begin a strength draft first")?
            .set_text(self.input.text().into())
    }
}

pub(super) fn poll(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    if !ctx
        .runner
        .state()
        .product
        .as_ref()
        .is_some_and(|p| p.coupling_result.is_some())
    {
        return;
    }
    ctx.runner.update(|page| {
        let result = page
            .product
            .as_mut()
            .expect("local product")
            .coupling_result
            .take()
            .unwrap();
        let pane = &mut page.field_inspector.editor;
        pane.busy = false;
        pane.status = match result {
            Ok(warning) => {
                pane.draft = None;
                pane.preview = None;
                warning.map_or_else(
                    || "Coupling change saved".into(),
                    |warning| format!("Coupling change saved · {warning}"),
                )
            },
            Err(error) => {
                if pane.draft.is_some() {
                    format!("Coupling change refused · {error} · draft retained")
                } else {
                    format!("Coupling history · {error}")
                }
            },
        };
    });
}

pub(super) fn reveal(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    if !ctx.runner.state().field_inspector.editor.reveal {
        return;
    }
    if let Some(node) = ctx.runner.focus() {
        ctx.scroll_into_view(node, cambium_rootstock::ScrollAlign::Start);
    }
    ctx.runner
        .update(|page| page.field_inspector.editor.reveal = false);
}

fn begin(page: &mut TreePage) {
    let result = (|| {
        if page.field_inspector.editor.busy || page.field_inspector.editor.draft.is_some() {
            return Err("Apply or discard the current strength draft first".into());
        }
        let id = page
            .field_inspector
            .coupling
            .ok_or("Inspect one exact coupling first")?;
        StrengthDraft::begin(
            field_inspection::graph(page)?,
            page.product
                .as_ref()
                .ok_or("Local source is unavailable")?
                .source_session_id()?,
            id,
        )
    })();
    let pane = &mut page.field_inspector.editor;
    match result {
        Ok(draft) => {
            pane.input = TextInput::new(&draft.text);
            pane.draft = Some(draft);
            pane.preview = None;
            pane.status = "Strength draft opened".into();
            pane.reveal = true;
        },
        Err(error) => pane.status = error,
    }
}

fn preview(page: &mut TreePage) {
    let result = (|| {
        page.field_inspector.editor.stage()?;
        let draft = page.field_inspector.editor.draft.as_ref().unwrap();
        let candidate = draft.candidate(field_inspection::graph(page)?)?;
        Ok(format!(
            "{} to {} · exact coupling {}. Authored value preview; no force simulation.",
            draft.baseline.strength,
            candidate.strength,
            candidate.id.as_uuid()
        ))
    })();
    let pane = &mut page.field_inspector.editor;
    match result {
        Ok(text) => {
            pane.preview = Some(pane.input.text().into());
            pane.status = text;
        },
        Err(error) => {
            pane.preview = None;
            pane.status = error;
        },
    }
}

fn save(page: &mut TreePage, history: Option<bool>) {
    if page.field_inspector.editor.busy {
        return;
    }
    let result = (|| {
        let operation = if let Some(redo) = history {
            if page.field_inspector.editor.draft.is_some() {
                return Err("Apply or discard the draft before saved undo/redo".into());
            }
            CouplingOperation::History(
                page.field_inspector
                    .coupling
                    .ok_or("Inspect one exact coupling first")?,
                redo,
            )
        } else {
            page.field_inspector.editor.stage()?;
            let draft = page.field_inspector.editor.draft.as_ref().unwrap().clone();
            draft.candidate(field_inspection::graph(page)?)?;
            CouplingOperation::Apply(draft)
        };
        page.product
            .as_mut()
            .ok_or("Local source is unavailable")?
            .save_coupling(operation)
    })();
    let pane = &mut page.field_inspector.editor;
    match result {
        Ok(()) => {
            pane.busy = true;
            pane.status = "Saving coupling change".into();
        },
        Err(error) => pane.status = error,
    }
}

pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    let page = runner.state();
    let pane = &page.field_inspector.editor;
    if !page.field_inspector.open || pane.busy || pane.draft.is_none() {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    if !focused_field_has_label(&dom.borrow(), node, "Coupling strength") {
        return None;
    }
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(|page| &page.field_inspector.editor.input),
        get_mut: Box::new(|page| &mut page.field_inspector.editor.input),
    })
}

pub(super) fn section(page: &TreePage) -> Child {
    let pane = &page.field_inspector.editor;
    let reveal = pane.reveal;
    let mut children: Vec<Child> = Vec::new();
    if !pane.busy {
        if let Some(draft) = &pane.draft {
            children.push(Box::new(el("h3", "Coupling strength draft")));
            children.push(Box::new(el(
                "p",
                format!(
                    "Editing exact coupling {} · field {}",
                    draft.baseline.id.as_uuid(),
                    draft.baseline.field.as_uuid()
                ),
            )));
            children.push(Box::new(el(
                "label",
                (
                    "Coupling strength",
                    lens(
                        move |input: &mut TextInput| {
                            cambium::request_focus(
                                text_field_typed(input).attr("aria-label", "Coupling strength"),
                                reveal,
                            )
                        },
                        |page: &mut TreePage| &mut page.field_inspector.editor.input,
                    ),
                ),
            )));
            children.push(Box::new(button(
                "Preview coupling strength",
                |page: &mut TreePage, _| preview(page),
            )));
            children.push(Box::new(button(
                "Apply coupling strength",
                |page: &mut TreePage, _| save(page, None),
            )));
            children.push(Box::new(button(
                "Discard strength draft",
                |page: &mut TreePage, _| {
                    let pane = &mut page.field_inspector.editor;
                    pane.draft = None;
                    pane.preview = None;
                    pane.status = "Strength draft discarded".into();
                },
            )));
            for (label, redo) in [
                ("Undo strength draft", false),
                ("Redo strength draft", true),
            ] {
                children.push(Box::new(button(label, move |page: &mut TreePage, _| {
                    let pane = &mut page.field_inspector.editor;
                    if !redo {
                        if let Err(error) = pane.stage() {
                            pane.status = error;
                            return;
                        }
                    } else if pane
                        .draft
                        .as_ref()
                        .is_some_and(|d| d.text != pane.input.text())
                    {
                        pane.status = "Update or undo pending input before Redo".into();
                        return;
                    }
                    let draft = pane.draft.as_mut().unwrap();
                    draft.undo(redo);
                    pane.input = TextInput::new(&draft.text);
                    pane.preview = None;
                })));
            }
            if pane.preview.as_deref() != Some(pane.input.text()) {
                children.push(Box::new(el("p", "Preview is not current")));
            }
        } else if page.field_inspector.coupling.is_some() {
            children.push(Box::new(button(
                "Edit coupling strength",
                |page: &mut TreePage, _| begin(page),
            )));
            for (label, redo) in [
                ("Undo saved coupling change", false),
                ("Redo saved coupling change", true),
            ] {
                children.push(Box::new(button(label, move |page: &mut TreePage, _| {
                    save(page, Some(redo))
                })));
            }
        }
    }
    if !pane.status.is_empty() {
        children.push(Box::new(
            el("p", pane.status.clone()).attr("role", "status"),
        ));
    }
    Box::new(el("div", children).attr("class", "tree-coupling"))
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.field_inspector.editor;
    snapshot
        .with_field("coupling-editing", pane.draft.is_some().to_string())
        .with_field("coupling-saving", pane.busy.to_string())
        .with_field(
            "coupling-draft-id",
            pane.draft
                .as_ref()
                .map(|d| d.baseline.id.as_uuid().to_string())
                .unwrap_or_default(),
        )
        .with_field("coupling-draft-strength", pane.input.text())
        .with_field(
            "coupling-preview-current",
            (pane.preview.as_deref() == Some(pane.input.text())).to_string(),
        )
}
