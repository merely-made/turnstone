// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained authoring, exact occurrence previews and opt-in saved Forme guides.
use super::*;
use cambium::{TextInput, button, lens, text_field_typed};
use graphshell::field_inspection::FieldTarget;
use graphshell::presentation_binding::{
    BoundContext, FormeFacts, Occurrence, ViewInputs, bind_with_source,
};
use graphshell::presentation_store;
use graphshell::presentation_store::{Loaded, OwnerObservation, SaveFailure, binding_changed};
use mere::canvas::Role;
use mere_view::presentation_editor::{
    self as controls, Effect as ControlEffect, PresentationEditor, Snapshot,
};
use muniment::IndexedDbBackend;
use scenograph::presentation::editor::{
    OperationId, RuleDraft, RuleSetDraft, RuleWorkspace, SaveKind, parse,
};
use scenograph::presentation::{Context, PrimitiveKind, Property, Resolution, RuleSet};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Inputs {
    bound: Option<Result<BoundContext, String>>,
    live: Option<Result<BoundContext, String>>,
    generation: u64,
}

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
    workspace: Option<RuleWorkspace>,
    backend: Option<IndexedDbBackend>,
    graph_id: Option<uuid::Uuid>,
    baseline: Option<Vec<u8>>,
    binding_generation: u64,
    owner_suspended: bool,
    reloading: bool,
    conflict: bool,
    controls: PresentationEditor,
    completed: Rc<RefCell<VecDeque<Completion>>>,
    open: bool,
    selected: Option<usize>,
    inputs: Vec<TextInput>,
    enabled: bool,
    status: String,
    preview: Option<(String, Context, Resolution)>,
    binding: Option<Occurrence>,
    inputs_seen: Option<Inputs>,
    live_enabled: bool,
    live_result: Option<Result<(BoundContext, Resolution), String>>,
}
enum Completion {
    Saved {
        binding: u64,
        graph_id: uuid::Uuid,
        operation: OperationId,
        result: Result<Vec<u8>, SaveFailure>,
    },
    Loaded {
        binding: u64,
        graph_id: uuid::Uuid,
        result: Result<Loaded, String>,
    },
}
impl Pane {
    pub(super) async fn open(product: Option<&product::SavedProduct>) -> Self {
        let mut pane = Self {
            workspace: None,
            backend: None,
            graph_id: None,
            baseline: None,
            binding_generation: 1,
            owner_suspended: false,
            reloading: false,
            conflict: false,
            controls: PresentationEditor::default(),
            completed: Rc::new(RefCell::new(VecDeque::new())),
            open: false,
            selected: None,
            inputs: (0..7).map(|_| TextInput::new("")).collect(),
            enabled: true,
            status: String::new(),
            preview: None,
            binding: None,
            inputs_seen: None,
            live_enabled: false,
            live_result: None,
        };
        let Some((backend, id)) = product.and_then(|p| p.forme_source()) else {
            return pane;
        };
        pane.sample(PrimitiveKind::Node);
        match presentation_store::load(backend.clone(), id).await {
            Ok(loaded) => {
                pane.backend = Some(backend);
                pane.install_loaded(loaded);
            },
            Err(error) => {
                pane.status =
                    format!("Rules could not be restored · {error}. Saved bytes retained.");
            },
        }
        pane
    }
    pub(super) fn ready(&self) -> bool {
        !self.completed.borrow().is_empty() || self.controls.has_effects()
    }
    pub(super) fn owner_changed(&self, product: Option<&product::SavedProduct>) -> bool {
        let observed = match product {
            None => OwnerObservation::NoProduct,
            Some(product) => match product.forme_source() {
                Some((_, graph_id)) => OwnerObservation::Graph(graph_id),
                None => OwnerObservation::TemporarilyUnavailable,
            },
        };
        binding_changed(self.graph_id, observed)
            || (!self.owner_suspended
                && matches!(observed, OwnerObservation::TemporarilyUnavailable))
            || (self.owner_suspended && matches!(observed, OwnerObservation::Graph(_)))
    }
    pub(super) fn live_enabled(&self) -> bool {
        self.live_enabled
    }
    pub(super) fn saved_rules(&self) -> Option<(&RuleSet, u64)> {
        self.workspace
            .as_ref()
            .map(|workspace| (workspace.committed(), workspace.generation()))
    }
    pub(super) fn busy(&self) -> bool {
        self.owner_suspended
            || self.reloading
            || self.workspace.as_ref().is_some_and(RuleWorkspace::busy)
    }
    fn editing(&self) -> bool {
        self.workspace.as_ref().is_some_and(RuleWorkspace::editing)
    }
    fn rows(&self) -> RuleSetDraft {
        self.workspace
            .as_ref()
            .map_or_else(RuleSetDraft::default, |s| {
                s.draft()
                    .cloned()
                    .unwrap_or_else(|| RuleSetDraft::from(s.committed()))
            })
    }
    fn install_loaded(&mut self, loaded: Loaded) {
        self.graph_id = Some(loaded.graph_id);
        self.baseline = loaded.baseline;
        self.workspace = Some(loaded.workspace);
        self.conflict = false;
        self.reloading = false;
        self.sync();
        self.controls.set_snapshot(self.control_snapshot());
    }
    fn control_snapshot(&self) -> Snapshot {
        let workspace = self.workspace.as_ref();
        Snapshot {
            available: self.backend.is_some() && self.graph_id.is_some() && !self.owner_suspended,
            editing: workspace.is_some_and(RuleWorkspace::editing),
            dirty: workspace.is_some_and(RuleWorkspace::dirty) || self.unstaged(),
            busy: self.busy(),
            can_undo: workspace.is_some_and(RuleWorkspace::can_undo),
            can_redo: workspace.is_some_and(RuleWorkspace::can_redo),
            can_reload: (self.conflict || workspace.is_none()) && !self.busy(),
            notice: self.status.clone(),
        }
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
        let rows = self.rows();
        self.editing()
            && self
                .selected
                .and_then(|i| rows.rules.get(i))
                .is_some_and(|row| row != &self.input())
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
            self.workspace
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
        self.binding = None;
        self.inputs_seen = None;
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
        let session = self
            .workspace
            .as_ref()
            .ok_or("Rule workspace unavailable")?;
        let result = if self.binding.is_some() {
            let bound = self
                .inputs_seen
                .as_ref()
                .and_then(|inputs| inputs.bound.as_ref())
                .ok_or("Bound preview inputs have not been read")?
                .as_ref()
                .map_err(Clone::clone)?;
            session
                .rules()
                .map_err(|e| format!("Rule preview refused: {e:?}"))?
                .resolve_with_availability(&bound.context, &bound.availability())
        } else {
            session.preview(&context)
        }
        .map_err(|e| format!("Rule preview refused: {e:?}"))?;
        self.preview = Some((source, context, result));
        Ok(())
    }
    fn save(&mut self, kind: SaveKind) -> Result<(), String> {
        if kind == SaveKind::Apply {
            self.stage()?;
        }
        let backend = self.backend.clone().ok_or("Rule store unavailable")?;
        let graph_id = self.graph_id.ok_or("Rule workspace graph unavailable")?;
        let proposal = presentation_store::prepare_save(
            self.workspace
                .as_mut()
                .ok_or("Rule workspace unavailable")?,
            graph_id,
            self.baseline.clone(),
            kind,
        )?;
        self.status = "Saving presentation rules…".into();
        let completed = self.completed.clone();
        let binding = self.binding_generation;
        let operation = proposal.id();
        wasm_bindgen_futures::spawn_local(async move {
            let result = proposal.save(backend).await;
            completed.borrow_mut().push_back(Completion::Saved {
                binding,
                graph_id,
                operation,
                result,
            });
        });
        Ok(())
    }
    fn poll(&mut self) {
        self.controls.set_snapshot(self.control_snapshot());
        while let Some(effect) = self.controls.take_effect() {
            let result = (|| -> Result<(), String> {
                match effect {
                    ControlEffect::Begin => self
                        .workspace
                        .as_mut()
                        .ok_or_else(|| "Rule workspace unavailable".to_string())
                        .and_then(RuleWorkspace::begin),
                    ControlEffect::AddNode
                    | ControlEffect::AddRelation
                    | ControlEffect::AddField => {
                        self.stage()?;
                        let kind = match effect {
                            ControlEffect::AddNode => PrimitiveKind::Node,
                            ControlEffect::AddRelation => PrimitiveKind::Relation,
                            _ => PrimitiveKind::Field,
                        };
                        let mut selected = None;
                        self.workspace
                            .as_mut()
                            .ok_or("Rule workspace unavailable")?
                            .edit(None, now_ms() as u64, |draft| {
                                selected = Some(draft.add(kind))
                            })?;
                        self.selected = selected;
                        self.sync();
                        Ok(())
                    },
                    ControlEffect::Apply => self.save(SaveKind::Apply),
                    ControlEffect::Discard => {
                        self.workspace
                            .as_mut()
                            .ok_or("Rule workspace unavailable")?
                            .discard()?;
                        self.selected = None;
                        self.sync();
                        Ok(())
                    },
                    ControlEffect::Undo | ControlEffect::Redo => {
                        let redo = effect == ControlEffect::Redo;
                        if self.editing() {
                            self.stage()?;
                            self.workspace.as_mut().unwrap().undo_draft(redo)?;
                            self.selected = None;
                            self.sync();
                            Ok(())
                        } else {
                            self.save(if redo { SaveKind::Redo } else { SaveKind::Undo })
                        }
                    },
                    ControlEffect::Reload => {
                        self.reload_latest();
                        Ok(())
                    },
                }
            })();
            if let Err(error) = result {
                self.status = error;
            }
            self.controls.set_snapshot(self.control_snapshot());
        }
        if self.owner_suspended {
            return;
        }
        let Some(completion) = self.completed.borrow_mut().pop_front() else {
            return;
        };
        match completion {
            Completion::Saved {
                binding,
                graph_id,
                operation,
                result,
            } => {
                if binding != self.binding_generation || Some(graph_id) != self.graph_id {
                    return;
                }
                let finish = match &result {
                    Ok(_) => Ok(()),
                    Err(SaveFailure::Conflict(reason) | SaveFailure::Failed(reason)) => {
                        Err(reason.clone())
                    },
                };
                match self
                    .workspace
                    .as_mut()
                    .expect("pending workspace")
                    .finish(operation, finish)
                {
                    Ok(true) => {
                        if let Ok(bytes) = result {
                            self.baseline = Some(bytes);
                        }
                        self.conflict = false;
                        self.status = "Presentation rules saved".into();
                        self.sync();
                    },
                    Ok(false) => {},
                    Err(error) => {
                        self.conflict = matches!(result, Err(SaveFailure::Conflict(_)));
                        self.status =
                            format!("Save failed · {error} · Draft and saved version retained");
                    },
                }
            },
            Completion::Loaded {
                binding,
                graph_id,
                result,
            } => {
                if binding != self.binding_generation || Some(graph_id) != self.graph_id {
                    return;
                }
                match result {
                    Ok(loaded) => {
                        if let Some(workspace) = self.workspace.as_mut() {
                            workspace
                                .reconcile(loaded.workspace.saved_state().clone())
                                .expect("loaded state validated");
                            self.baseline = loaded.baseline;
                            self.conflict = false;
                            self.reloading = false;
                            self.preview = None;
                            self.inputs_seen = None;
                            self.live_result = None;
                            self.status =
                                "Latest saved rules loaded; private draft retained for review"
                                    .into();
                        } else {
                            self.install_loaded(loaded);
                        }
                    },
                    Err(error) => {
                        self.reloading = false;
                        self.status = format!("Reload failed · {error} · Draft retained");
                    },
                }
            },
        }
        self.controls.set_snapshot(self.control_snapshot());
    }
    fn reload_latest(&mut self) {
        let (Some(backend), Some(graph_id)) = (self.backend.clone(), self.graph_id) else {
            return;
        };
        self.binding_generation = self
            .binding_generation
            .checked_add(1)
            .expect("presentation binding generation exhausted");
        let binding = self.binding_generation;
        self.reloading = true;
        self.status = "Loading latest saved rules; private draft retained".into();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = presentation_store::load(backend, graph_id).await;
            completed.borrow_mut().push_back(Completion::Loaded {
                binding,
                graph_id,
                result,
            });
        });
    }
    fn observe_owner(&mut self, product: Option<&product::SavedProduct>) {
        let Some(product) = product else {
            if self.graph_id.is_some() {
                self.binding_generation = self
                    .binding_generation
                    .checked_add(1)
                    .expect("presentation binding generation exhausted");
                self.graph_id = None;
                self.backend = None;
                self.baseline = None;
                self.workspace = None;
                self.reloading = false;
                self.owner_suspended = false;
                self.conflict = false;
                self.selected = None;
                self.controls = PresentationEditor::default();
                self.status = "Presentation workspace retired after graph owner change".into();
            }
            return;
        };
        let Some((backend, graph_id)) = product.forme_source() else {
            // Product save temporarily takes the app out of SavedProduct. Keep
            // the current graph workspace and its receipts until the owner is
            // observable again; the missing value does not mean the product vanished.
            self.owner_suspended = true;
            self.status = "Presentation controls paused while the graph owner is saving".into();
            self.controls.set_snapshot(self.control_snapshot());
            return;
        };
        self.owner_suspended = false;
        if self.graph_id == Some(graph_id) {
            return;
        }
        self.binding_generation = self
            .binding_generation
            .checked_add(1)
            .expect("presentation binding generation exhausted");
        let binding = self.binding_generation;
        self.graph_id = Some(graph_id);
        self.backend = Some(backend.clone());
        self.baseline = None;
        self.workspace = None;
        self.reloading = true;
        self.owner_suspended = false;
        self.conflict = false;
        self.selected = None;
        self.preview = None;
        self.binding = None;
        self.inputs_seen = None;
        self.live_result = None;
        self.controls = PresentationEditor::default();
        self.status = "Loading presentation rules for the current graph".into();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = presentation_store::load(backend, graph_id).await;
            completed.borrow_mut().push_back(Completion::Loaded {
                binding,
                graph_id,
                result,
            });
        });
    }
}
pub(super) fn poll(page: &mut TreePage) {
    page.presentation.observe_owner(page.product.as_ref());
    page.presentation.poll();
}
fn bound(page: &TreePage, occurrence: &Occurrence) -> Result<BoundContext, String> {
    let graph = field_inspection::graph(page)?;
    let canvas = page.shared.canvas.borrow();
    let mut view = ViewInputs::default();
    let mut forme = None;
    match occurrence {
        Occurrence::Access { member } => {
            let key = canvas
                .graph()
                .get_node_key_by_id(*member)
                .ok_or("Access is absent from this view")?;
            view.selected = Some(canvas.selected_members().contains(member));
            view.position_pinned = Some(canvas.arrangement_role_of(key) == Role::Pinned);
            view.inspected = super::source_binding::member(page) == Some(*member);
            view.screen_size = canvas.node_presentation_screen_size(*member);
            view.set_attention(
                canvas.member_attention(*member),
                canvas.member_foreground_pinned(*member),
            );
        },
        Occurrence::Assertion { pair, statement } => {
            view.inspected = page.inspected_link == Some(*pair)
                && page.inspected_statement.as_ref() == Some(statement);
        },
        Occurrence::Numen { id } => {
            view.inspected = page.field_inspector.target == Some(FieldTarget::Numen(*id));
            view.visible = Some(canvas.field_visible(*id));
        },
        Occurrence::Forme { id } => {
            let owner = page
                .forme
                .as_ref()
                .ok_or("No Forme workspace is presented here")?
                .current();
            if owner.document.id.as_uuid() != *id {
                return Err(format!("Forme {id} is no longer presented here"));
            }
            forme = Some(FormeFacts {
                id: *id,
                label: owner.document.label.clone(),
                locked: owner.locked,
                visible: owner.visible,
                members: owner.members().len(),
            });
            view.inspected = page.field_inspector.target == Some(FieldTarget::Forme(*id));
            view.hovered = canvas
                .forme_region()
                .filter(|region| region.id == *id)
                .map(|_| canvas.forme_region_hovered());
        },
    }
    let source = page
        .product
        .as_ref()
        .ok_or("Presentation binding requires the local graph")?
        .link_source()?;
    bind_with_source(graph, occurrence, &view, forme.as_ref(), source)
}
/// Re-resolve exact owners on view changes; preview is invalidated, never retargeted.
pub(super) fn refresh(page: &TreePage) -> Option<Inputs> {
    let pane = &page.presentation;
    if pane.binding.is_none() && !pane.live_enabled && pane.inputs_seen.is_none() {
        return None;
    }
    let inputs = Inputs {
        bound: if pane.open {
            pane.binding.as_ref().map(|target| bound(page, target))
        } else {
            pane.inputs_seen
                .as_ref()
                .and_then(|inputs| inputs.bound.clone())
        },
        live: if pane.live_enabled {
            Some(
                page.forme
                    .as_ref()
                    .ok_or_else(|| "No Forme workspace is presented here".into())
                    .and_then(|forme| {
                        bound(
                            page,
                            &Occurrence::Forme {
                                id: forme.current().document.id.as_uuid(),
                            },
                        )
                    }),
            )
        } else {
            None
        },
        generation: pane
            .workspace
            .as_ref()
            .map_or(0, |workspace| workspace.generation()),
    };
    (pane.inputs_seen.as_ref() != Some(&inputs)).then_some(inputs)
}
pub(super) fn update(page: &mut TreePage, inputs: Inputs) {
    let pane = &mut page.presentation;
    if pane.inputs_seen.as_ref().and_then(|old| old.bound.as_ref()) != inputs.bound.as_ref() {
        pane.preview = None;
        if let Some(Ok(bound)) = &inputs.bound {
            pane.inputs[6] = TextInput::new(serde_json::to_string(&bound.context).unwrap());
        }
    }
    pane.live_result = inputs.live.as_ref().map(|result| {
        let bound = result.as_ref().map_err(Clone::clone)?;
        let session = pane
            .workspace
            .as_ref()
            .ok_or("Saved rule workspace unavailable")?;
        let resolution = session
            .committed()
            .resolve_with_availability(&bound.context, &bound.availability())
            .map_err(|issues| format!("Saved rules refused: {issues:?}"))?;
        Ok((bound.clone(), resolution))
    });
    pane.inputs_seen = Some(inputs);
}
pub(super) fn install(page: &TreePage) {
    let effect = page
        .presentation
        .live_result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|(bound, result)| bound.guides(result));
    if page.shared.canvas.borrow_mut().set_forme_guides(effect) {
        page.shared.dirty.set(true);
    }
}
pub(super) fn choose(page: &mut TreePage, target: Occurrence) {
    page.presentation.open = true;
    page.presentation.binding = Some(target);
    page.presentation.preview = None;
    page.presentation.inputs_seen = None;
    if let Some(inputs) = refresh(page) {
        update(page, inputs);
    }
    page.presentation.status =
        "Bound to the exact occurrence; preview reads current held facts and disclosed view state"
            .into();
}
fn action(page: &mut TreePage, change: impl FnOnce(&mut Pane) -> Result<(), String>) {
    let pane = &mut page.presentation;
    if pane.busy() {
        return;
    }
    if let Err(error) = change(pane) {
        pane.status = error;
    }
    pane.controls.set_snapshot(pane.control_snapshot());
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
        (((i == 6 && pane.binding.is_none())
            || (i < 6 && pane.editing() && pane.selected.is_some()))
            && focused_field_has_label(&dom, node, label))
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
    let mut children: Vec<Child> = vec![Box::new(lens(
        |editor: &mut PresentationEditor| controls::view(editor),
        controls_editor,
    ))];
    if pane.editing() {
        children.push(Box::new(button("Add node ladder defaults", |page: &mut TreePage, _| {
            action(page, |pane| {
                pane.stage()?;
                let mut selected = None;
                pane.workspace.as_mut().unwrap().edit(None, now_ms() as u64, |draft| selected = Some(graphshell::node_presentation::add_defaults(draft)))?;
                pane.selected = selected;
                pane.sync();
                pane.status = "Node ladder defaults added to the private draft; edit priorities and thresholds before applying".into();
                Ok(())
            })
        })));
    }
    children
}
fn controls_editor(page: &mut TreePage) -> &mut PresentationEditor {
    let snapshot = page.presentation.control_snapshot();
    page.presentation.controls.set_snapshot(snapshot);
    &mut page.presentation.controls
}
pub(super) fn section(page: &TreePage) -> Child {
    let pane = &page.presentation;
    if page.product.is_none() {
        return Box::new(el("div", ()));
    }
    let mut children: Vec<Child> = vec![Box::new(disclosure_trigger(
        if pane.open {
            "Close rule editor"
        } else {
            "Presentation rules"
        },
        pane.open,
        |page: &mut TreePage| {
            page.presentation.open = !page.presentation.open;
            page.tools_open = true;
        },
    ))];
    if !pane.open {
        return Box::new(el("section", children).attr("class", "tools-section"));
    }
    children.push(Box::new(el("h2", "Presentation rules")));
    children.push(Box::new(el("p", "Target → condition → effect. Exact occurrence previews disclose their inputs. Saved rules can control Forme subdivisions in this view.")));
    children.push(Box::new(status_note("", pane.status.clone())));
    if let Some(session) = &pane.workspace {
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
        children.push(Box::new(
            button(
                if pane.live_enabled {
                    "Disable saved rules in this view"
                } else {
                    "Enable saved rules in this view"
                },
                |page: &mut TreePage, _| {
                    page.presentation.live_enabled = !page.presentation.live_enabled;
                    if let Some(inputs) = refresh(page) {
                        update(page, inputs);
                    }
                    install(page);
                },
            )
            .attr("aria-pressed", pane.live_enabled.to_string()),
        ));
        children.push(Box::new(el("p", "Live adoption uses acknowledged saved rules, including saved Undo/Redo. Draft previews stay private. Other effects keep the host presentation until a renderer supports them.")));
        if let Some(result) = &pane.live_result {
            match result {
                Ok((bound, result)) => {
                    children.push(Box::new(el(
                        "p",
                        format!(
                            "Live Forme {} · {}",
                            bound.context.id,
                            bound
                                .guides(result)
                                .map_or("base guides", |(_, visible)| if visible {
                                    "show subdivisions"
                                } else {
                                    "hide subdivisions"
                                })
                        ),
                    )));
                    for (property, decision) in &result.winners {
                        if !bound.supports(*property) {
                            children.push(Box::new(el(
                                "p",
                                format!(
                                    "{property:?} from {} has no renderer here; host base remains.",
                                    decision.rule_id
                                ),
                            )));
                        }
                    }
                    for trace in result
                        .traces
                        .iter()
                        .filter(|trace| trace.reason.starts_with("Host input unavailable"))
                    {
                        children.push(Box::new(el(
                            "p",
                            format!("{} · {}", trace.rule_id, trace.reason),
                        )));
                    }
                },
                Err(error) => children.push(Box::new(el(
                    "p",
                    format!("Live inputs unavailable · {error} · base guides restored"),
                ))),
            }
        }
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
                        pane.workspace
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
            if let Some(member) = super::source_binding::member(page) {
                children.push(Box::new(button(
                    "Use inspected access",
                    move |page: &mut TreePage, _| choose(page, Occurrence::Access { member }),
                )));
            }
            if let (Some(pair), Some(statement)) =
                (page.inspected_link, page.inspected_statement.clone())
            {
                children.push(Box::new(button(
                    "Use inspected assertion",
                    move |page: &mut TreePage, _| {
                        choose(
                            page,
                            Occurrence::Assertion {
                                pair,
                                statement: statement.clone(),
                            },
                        )
                    },
                )));
            }
            if let Some(target) = page.field_inspector.target {
                children.push(Box::new(button(
                    "Use inspected field",
                    move |page: &mut TreePage, _| {
                        choose(
                            page,
                            match target {
                                FieldTarget::Numen(id) => Occurrence::Numen { id },
                                FieldTarget::Forme(id) => Occurrence::Forme { id },
                            },
                        )
                    },
                )));
            }
            if let Some(target) = &pane.binding {
                children.push(Box::new(el(
                    "p",
                    format!("Exact occurrence {}", target.id()),
                )));
                match pane
                    .inputs_seen
                    .as_ref()
                    .and_then(|inputs| inputs.bound.as_ref())
                {
                    Some(Ok(bound)) => {
                        children.push(Box::new(el(
                            "pre",
                            serde_json::to_string_pretty(bound).unwrap(),
                        )));
                        for note in &bound.notes {
                            children.push(Box::new(el("p", note.clone())));
                        }
                    },
                    Some(Err(error)) => children.push(Box::new(el(
                        "p",
                        format!("Bound inputs unavailable · {error}"),
                    ))),
                    None => {},
                }
                children.push(Box::new(button("Edit context manually", |page: &mut TreePage, _| {
                    page.presentation.binding = None;
                    page.presentation.preview = None;
                    page.presentation.status = "Manual context copied from the last bound inputs; it no longer tracks the graph".into();
                })));
            } else {
                children.push(input(6));
                children.push(Box::new(el("p", "Manual facts, states, screen size and prior matches are explicit preview inputs. Samples do not change the canvas.")));
            }
            children.push(Box::new(button(
                "Preview rule decisions",
                |page: &mut TreePage, _| {
                    if let Some(inputs) = refresh(page) {
                        update(page, inputs);
                    }
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
                            "{property:?}: {} · rule {} · priority {} · {}",
                            serde_json::to_string(&decision.effect).unwrap(),
                            decision.rule_id,
                            decision.priority,
                            if pane
                                .inputs_seen
                                .as_ref()
                                .and_then(|inputs| inputs.bound.as_ref())
                                .and_then(|result| result.as_ref().ok())
                                .is_some_and(|bound| bound.supports_effect(&decision.effect))
                            {
                                "Renderer available for saved rules"
                            } else {
                                "decision only; no live renderer for this preview"
                            }
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
    let session = pane.workspace.as_ref();
    let rules = session.and_then(|s| s.rules().ok());
    snapshot
        .with_field("rules-open", pane.open.to_string())
        .with_field("rules-bound", pane.binding.is_some().to_string())
        .with_field(
            "rules-bound-context",
            pane.inputs_seen
                .as_ref()
                .and_then(|inputs| inputs.bound.as_ref())
                .and_then(|result| result.as_ref().ok())
                .map(|bound| serde_json::to_string(&bound.context).unwrap())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-live-guides-rule",
            pane.live_result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(|(_, result)| result.winners.get(&Property::RegionGuides))
                .map(|decision| decision.rule_id.clone())
                .unwrap_or_default(),
        )
        .with_field(
            "rules-bound-id",
            pane.binding
                .as_ref()
                .map(Occurrence::id)
                .unwrap_or_default(),
        )
        .with_field(
            "rules-bound-ready",
            pane.inputs_seen
                .as_ref()
                .and_then(|inputs| inputs.bound.as_ref())
                .is_some_and(|result| result.is_ok())
                .to_string(),
        )
        .with_field("rules-live-enabled", pane.live_enabled.to_string())
        .with_field(
            "rules-live-ready",
            pane.live_result
                .as_ref()
                .is_some_and(|result| result.is_ok())
                .to_string(),
        )
        .with_field(
            "rules-live",
            pane.live_result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .map(|(_, result)| serde_json::to_string(result).unwrap())
                .unwrap_or_default(),
        )
        .with_field(
            "forme-guides-visible",
            page.shared
                .canvas
                .borrow()
                .forme_guides_visible()
                .to_string(),
        )
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
