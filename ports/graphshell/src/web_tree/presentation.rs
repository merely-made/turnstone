// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained authoring controls and decision preview; no renderer effect adapter.
use super::*;
use cambium::{TextInput, button, lens, text_field_typed};
use graphshell::presentation_editor::presentation::{Context, PrimitiveKind, Property, Resolution};
use graphshell::presentation_editor::{
    PresentationSession, RuleDraft, RuleSetDraft, SaveKind, parse,
};
use muniment::IndexedDbBackend;

const LABELS: [&str; 7] = [
    "Rule ID",
    "Rule label",
    "Rule priority",
    "Rule target JSON",
    "Rule condition JSON",
    "Rule effects JSON",
    "Preview context JSON",
];
pub(super) struct Pane {
    session: Option<PresentationSession>,
    backend: Option<IndexedDbBackend>,
    completed: Rc<RefCell<Option<Result<(), String>>>>,
    open: bool,
    selected: Option<usize>,
    inputs: Vec<TextInput>,
    enabled: bool,
    status: String,
    preview: Option<(String, Context, Resolution)>,
}
impl Pane {
    pub(super) async fn open(product: Option<&product::SavedProduct>) -> Self {
        let mut pane = Self {
            session: None,
            backend: None,
            completed: Rc::new(RefCell::new(None)),
            open: false,
            selected: None,
            inputs: (0..7).map(|_| TextInput::new("")).collect(),
            enabled: true,
            status: String::new(),
            preview: None,
        };
        let Some((backend, id)) = product.and_then(|p| p.forme_source()) else {
            return pane;
        };
        match PresentationSession::load(backend.clone(), id).await {
            Ok(session) => {
                pane.session = Some(session);
                pane.backend = Some(backend);
            },
            Err(error) => {
                pane.status =
                    format!("Rules could not be restored · {error}. Saved bytes retained.");
            },
        }
        pane.sample(PrimitiveKind::Node);
        pane
    }
    pub(super) fn ready(&self) -> bool {
        self.completed.borrow().is_some()
    }
    pub(super) fn busy(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.busy())
    }
    fn editing(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.editing())
    }
    fn rows(&self) -> RuleSetDraft {
        self.session
            .as_ref()
            .map_or_else(RuleSetDraft::default, |s| {
                s.draft()
                    .cloned()
                    .unwrap_or_else(|| RuleSetDraft::from(s.committed()))
            })
    }
    fn input(&self) -> RuleDraft {
        RuleDraft {
            id: self.inputs[0].text().into(),
            label: self.inputs[1].text().into(),
            enabled: self.enabled,
            priority: self.inputs[2].text().into(),
            target: self.inputs[3].text().into(),
            condition: self.inputs[4].text().into(),
            effects: self.inputs[5].text().into(),
        }
    }
    fn unstaged(&self) -> bool {
        self.editing()
            && self
                .selected
                .and_then(|i| self.rows().rules.get(i).cloned())
                .is_some_and(|r| r != self.input())
    }
    fn sync(&mut self) {
        let rows = self.rows();
        self.selected = self.selected.filter(|i| *i < rows.rules.len());
        if let Some(rule) = self.selected.and_then(|i| rows.rules.get(i)) {
            for (i, text) in [
                &rule.id,
                &rule.label,
                &rule.priority,
                &rule.target,
                &rule.condition,
                &rule.effects,
            ]
            .into_iter()
            .enumerate()
            {
                self.inputs[i] = TextInput::new(text);
            }
            self.enabled = rule.enabled;
        }
        self.preview = None;
    }
    fn stage(&mut self) -> Result<(), String> {
        if !self.editing() {
            return Ok(());
        }
        if let Some(index) = self.selected {
            let rule = self.input();
            self.session
                .as_mut()
                .ok_or("Rule workspace unavailable")?
                .edit(None, now_ms() as u64, move |draft| {
                    if let Some(row) = draft.rules.get_mut(index) {
                        *row = rule;
                    }
                })?;
        }
        self.preview = None;
        Ok(())
    }
    fn sample(&mut self, kind: PrimitiveKind) {
        let (id, facts, states) = match kind {
            PrimitiveKind::Node => (
                "sample:node",
                json!({"media":"image", "scene":"force"}),
                json!(["selected", "overview"]),
            ),
            PrimitiveKind::Relation => (
                "sample:link",
                json!({"family":"membership", "scene":"hierarchy", "derived":true}),
                json!(["inspected"]),
            ),
            PrimitiveKind::Field => (
                "sample:field",
                json!({"role":"forme"}),
                json!(["layout_unlocked"]),
            ),
        };
        self.inputs[6] = TextInput::new(json!({"id":id,"kind":kind,"facts":facts,"states":states,"screen_size":64,"previous_matches":[]}).to_string());
        self.preview = None;
        self.status =
            "Sample context loaded; edit it and preview. Samples are explicit inputs.".into();
    }
    fn preview(&mut self) -> Result<(), String> {
        self.stage()?;
        let source = self.inputs[6].text().to_string();
        let context = parse::<Context>(&source, "context")
            .map_err(|e| format!("Preview context refused: {e:?}"))?;
        let result = self
            .session
            .as_ref()
            .ok_or("Rule workspace unavailable")?
            .preview(&context)
            .map_err(|e| format!("Rule preview refused: {e:?}"))?;
        self.preview = Some((source, context, result));
        Ok(())
    }
    fn save(&mut self, kind: SaveKind) -> Result<(), String> {
        if kind == SaveKind::Apply {
            self.stage()?;
        }
        let backend = self.backend.clone().ok_or("Rule store unavailable")?;
        let proposal = self
            .session
            .as_mut()
            .ok_or("Rule workspace unavailable")?
            .prepare_save(kind)?;
        self.status = "Saving presentation rules…".into();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            *completed.borrow_mut() = Some(proposal.save(backend).await);
        });
        Ok(())
    }
    fn poll(&mut self) {
        let Some(result) = self.completed.borrow_mut().take() else {
            return;
        };
        match self
            .session
            .as_mut()
            .expect("pending workspace")
            .finish_save(result)
        {
            Ok(()) => {
                self.status = "Presentation rules saved".into();
                self.sync();
            },
            Err(error) => {
                self.status = format!("Save failed · {error} · Draft and saved version retained")
            },
        }
    }
}
pub(super) fn poll(page: &mut TreePage) {
    page.presentation.poll();
}
fn action(page: &mut TreePage, change: impl FnOnce(&mut Pane) -> Result<(), String>) {
    let pane = &mut page.presentation;
    if pane.busy() {
        return;
    }
    if let Err(error) = change(pane) {
        pane.status = error;
    }
}
pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    let pane = &runner.state().presentation;
    if !pane.open || pane.busy() {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let index = LABELS.into_iter().enumerate().find_map(|(i, label)| {
        ((i == 6 || (pane.editing() && pane.selected.is_some()))
            && taproot::matching(&dom, &Selector::role("textbox").containing(label))
                .contains(&node))
        .then_some(i)
    })?;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |page| &page.presentation.inputs[index]),
        get_mut: Box::new(move |page| &mut page.presentation.inputs[index]),
    })
}
fn input(index: usize) -> Child {
    Box::new(el(
        "label",
        (
            LABELS[index],
            lens(
                move |input: &mut TextInput| {
                    text_field_typed(input).attr("aria-label", LABELS[index])
                },
                move |page: &mut TreePage| &mut page.presentation.inputs[index],
            ),
        ),
    ))
}
fn controls(page: &TreePage) -> Vec<Child> {
    let pane = &page.presentation;
    let mut children: Vec<Child> = Vec::new();
    if pane.busy() {
        children.push(Box::new(el("p", "Saving presentation rules…")));
        return children;
    }
    if !pane.editing() {
        children.push(Box::new(button(
            "Begin rule draft",
            |page: &mut TreePage, _| {
                action(page, |pane| {
                    pane.session
                        .as_mut()
                        .ok_or("Rule workspace unavailable")?
                        .begin()?;
                    pane.sync();
                    pane.status = "Private rule draft opened".into();
                    Ok(())
                })
            },
        )));
    } else {
        for (kind, label) in [
            (PrimitiveKind::Node, "Add node rule"),
            (PrimitiveKind::Relation, "Add link rule"),
            (PrimitiveKind::Field, "Add field rule"),
        ] {
            children.push(Box::new(button(label, move |page: &mut TreePage, _| {
                action(page, |pane| {
                    pane.stage()?;
                    let mut selected = None;
                    pane.session
                        .as_mut()
                        .unwrap()
                        .edit(None, now_ms() as u64, |draft| {
                            selected = Some(draft.add(kind));
                        })?;
                    pane.selected = selected;
                    pane.sync();
                    Ok(())
                })
            })));
        }
        children.push(Box::new(button(
            "Discard rule draft",
            |page: &mut TreePage, _| {
                action(page, |pane| {
                    pane.session.as_mut().unwrap().discard()?;
                    pane.sync();
                    pane.status = "Rule draft discarded".into();
                    Ok(())
                })
            },
        )));
        children.push(Box::new(button("Apply rules", |page: &mut TreePage, _| {
            action(page, |pane| pane.save(SaveKind::Apply))
        })));
    }
    let session = pane.session.as_ref().unwrap();
    for (redo, available, label) in [
        (
            false,
            session.can_undo(),
            if pane.editing() {
                "Undo rule draft"
            } else {
                "Undo saved rules"
            },
        ),
        (
            true,
            session.can_redo(),
            if pane.editing() {
                "Redo rule draft"
            } else {
                "Redo saved rules"
            },
        ),
    ] {
        let control = button(label, move |page: &mut TreePage, _| {
            action(page, |pane| {
                if pane.editing() {
                    if redo && pane.unstaged() {
                        return Err("Update or undo pending input before Redo".into());
                    }
                    // Pending text belongs to this draft too; stage before Undo.
                    if !redo {
                        pane.stage()?;
                    }
                    pane.session.as_mut().unwrap().undo_draft(redo)?;
                    pane.selected = None;
                    pane.sync();
                    Ok(())
                } else {
                    pane.save(if redo { SaveKind::Redo } else { SaveKind::Undo })
                }
            })
        });
        let control = if (redo && pane.unstaged()) || (!available && !pane.unstaged()) {
            control.attr("disabled", "")
        } else {
            control
        };
        children.push(Box::new(control));
    }
    children
}
pub(super) fn section(page: &TreePage) -> Child {
    let pane = &page.presentation;
    if page.product.is_none() {
        return Box::new(el("div", ()));
    }
    let mut children: Vec<Child> = vec![Box::new(
        button(
            if pane.open {
                "Close rule editor"
            } else {
                "Presentation rules"
            },
            |page: &mut TreePage, _| {
                page.presentation.open = !page.presentation.open;
                page.tools_open = true;
            },
        )
        .attr("aria-expanded", pane.open.to_string()),
    )];
    if !pane.open {
        return Box::new(el("section", children).attr("class", "tools-section"));
    }
    children.push(Box::new(el("h2", "Presentation rules")));
    children.push(Box::new(el("p", "Target → condition → effect. Preview shows decisions and explanations; canvas effect adoption is pending.")));
    children.push(Box::new(
        el("p", pane.status.clone()).attr("role", "status"),
    ));
    if let Some(session) = &pane.session {
        children.push(Box::new(el(
            "p",
            format!(
                "Saved generation {} · {}",
                session.generation(),
                if pane.unstaged() {
                    "Unstaged input"
                } else if session.dirty() {
                    "Draft changes"
                } else {
                    "No draft changes"
                }
            ),
        )));
        children.extend(controls(page));
        for (index, row) in pane.rows().rules.iter().enumerate() {
            let label = format!("Edit rule {}: {}", index + 1, row.label);
            children.push(Box::new(
                button(label, move |page: &mut TreePage, _| {
                    action(page, |pane| {
                        pane.stage()?;
                        pane.selected = Some(index);
                        pane.sync();
                        Ok(())
                    })
                })
                .attr("aria-pressed", (pane.selected == Some(index)).to_string()),
            ));
        }
        if pane.selected.is_some() && pane.editing() && !pane.busy() {
            children.extend((0..6).map(input));
            children.push(Box::new(button(
                if pane.enabled {
                    "Disable rule"
                } else {
                    "Enable rule"
                },
                |page: &mut TreePage, _| {
                    action(page, |pane| {
                        pane.enabled = !pane.enabled;
                        pane.stage()
                    })
                },
            )));
            children.push(Box::new(button(
                "Update rule draft",
                |page: &mut TreePage, _| {
                    action(page, |pane| {
                        pane.stage()?;
                        pane.status = "Rule input recorded in draft".into();
                        Ok(())
                    })
                },
            )));
            for (delta, label) in [
                (-1isize, "Move rule earlier"),
                (1, "Move rule later"),
                (0, "Remove rule"),
            ] {
                children.push(Box::new(button(label, move |page: &mut TreePage, _| {
                    action(page, |pane| {
                        pane.stage()?;
                        let index = pane.selected.ok_or("Choose a rule")?;
                        let next = index.checked_add_signed(delta);
                        pane.session
                            .as_mut()
                            .unwrap()
                            .edit(None, now_ms() as u64, |draft| {
                                if delta == 0 {
                                    draft.rules.remove(index);
                                } else if let Some(next) = next.filter(|i| *i < draft.rules.len()) {
                                    draft.rules.swap(index, next);
                                }
                            })?;
                        if delta == 0 {
                            pane.selected = None;
                        } else if next.is_some_and(|i| i < pane.rows().rules.len()) {
                            pane.selected = next;
                        }
                        pane.sync();
                        Ok(())
                    })
                })));
            }
        }
        if !pane.busy() {
            children.push(Box::new(el("h3", "Decision preview")));
            for (kind, label) in [
                (PrimitiveKind::Node, "Node context sample"),
                (PrimitiveKind::Relation, "Link context sample"),
                (PrimitiveKind::Field, "Field context sample"),
            ] {
                children.push(Box::new(button(label, move |page: &mut TreePage, _| {
                    action(page, |pane| {
                        pane.sample(kind);
                        Ok(())
                    })
                })));
            }
            children.push(input(6));
            children.push(Box::new(el("p", "Facts, states, screen size and prior matches are explicit preview inputs. They are not inferred from the current scene.")));
            children.push(Box::new(button(
                "Preview rule decisions",
                |page: &mut TreePage, _| {
                    action(page, |pane| {
                        pane.preview()?;
                        pane.status = "Decision preview resolved".into();
                        Ok(())
                    })
                },
            )));
        }
        if let Some((source, context, preview)) = &pane.preview {
            if pane.unstaged() || source != pane.inputs[6].text() {
                children.push(Box::new(el(
                    "p",
                    "Inputs changed; preview again to refresh the decisions.",
                )));
            } else {
                children.push(Box::new(el(
                    "p",
                    format!("Occurrence {} · {:?}", context.id, context.kind),
                )));
                if preview.winners.is_empty() {
                    children.push(Box::new(el(
                        "p",
                        "No winning effect; host base presentation applies.",
                    )));
                }
                for (property, decision) in &preview.winners {
                    children.push(Box::new(el(
                        "p",
                        format!(
                            "{property:?}: {} · rule {} · priority {}",
                            serde_json::to_string(&decision.effect).unwrap(),
                            decision.rule_id,
                            decision.priority
                        ),
                    )));
                }
                for trace in &preview.traces {
                    children.push(Box::new(el(
                        "p",
                        format!(
                            "{} · matched {} · {} · winning {:?}",
                            trace.rule_id, trace.matched, trace.reason, trace.winning_properties
                        ),
                    )));
                }
            }
        }
    }
    Box::new(
        el("section", children)
            .attr("class", "tools-section tree-inspection tree-rules")
            .attr("aria-label", "Presentation rule editor"),
    )
}
pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.presentation;
    let session = pane.session.as_ref();
    let rules = session.and_then(|s| s.rules().ok());
    snapshot
        .with_field("rules-open", pane.open.to_string())
        .with_field("rules-selected", pane.selected.is_some().to_string())
        .with_field(
            "rules-preview-ready",
            pane.preview
                .as_ref()
                .is_some_and(|(source, _, _)| !pane.unstaged() && source == pane.inputs[6].text())
                .to_string(),
        )
        .with_field("rules-ready", session.is_some().to_string())
        .with_field("rules-editing", pane.editing().to_string())
        .with_field("rules-saving", pane.busy().to_string())
        .with_field(
            "rules-dirty",
            session.is_some_and(|s| s.dirty()).to_string(),
        )
        .with_field("rules-unstaged", pane.unstaged().to_string())
        .with_field("rules-valid", rules.is_some().to_string())
        .with_field("rules-count", pane.rows().rules.len().to_string())
        .with_field(
            "rules-generation",
            session.map_or(0, |s| s.generation()).to_string(),
        )
        .with_field(
            "rules-json",
            rules
                .as_ref()
                .map(|r| serde_json::to_string(r).unwrap())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-committed",
            session
                .map(|s| serde_json::to_string(s.committed()).unwrap())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-preview",
            pane.preview
                .as_ref()
                .filter(|(source, _, _)| !pane.unstaged() && source == pane.inputs[6].text())
                .map(|(_, _, r)| serde_json::to_string(r).unwrap())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-selected-id",
            pane.selected
                .and_then(|i| pane.rows().rules.get(i).map(|r| r.id.clone()))
                .unwrap_or_default(),
        )
        .with_field(
            "rules-preview-details",
            pane.preview
                .as_ref()
                .filter(|(source, _, _)| !pane.unstaged() && source == pane.inputs[6].text())
                .and_then(|(_, _, r)| r.winners.get(&Property::Details))
                .map(|d| d.rule_id.clone())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-preview-provenance",
            pane.preview
                .as_ref()
                .filter(|(source, _, _)| !pane.unstaged() && source == pane.inputs[6].text())
                .and_then(|(_, _, r)| r.winners.get(&Property::Provenance))
                .map(|d| d.rule_id.clone())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-preview-guides",
            pane.preview
                .as_ref()
                .filter(|(source, _, _)| !pane.unstaged() && source == pane.inputs[6].text())
                .and_then(|(_, _, r)| r.winners.get(&Property::RegionGuides))
                .map(|d| d.rule_id.clone())
                .unwrap_or_default(),
        )
        .with_field("rules-status", pane.status.clone())
}
