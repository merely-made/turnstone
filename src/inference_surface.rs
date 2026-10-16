// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained answer card. Generation and durable jobs remain in the resident.
use crate::{
    contributed_surface::{SurfaceAdmissionError, SurfaceProvider},
    inference_service::{ClientSnapshot, Control, InferenceService, RetainedSourceOwner},
    panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId},
};
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, PointerClick, ResolvedSurfaceEvent,
    RetainedSurfaceSession, SurfaceEffect, SurfaceViewport, TextInput, clickable, el, lens,
    text_field_typed,
};
use distillery::inference_protocol::{RunStateV1, RunV1};
use genet_scripted_dom::NodeId;
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Weak},
};

pub const PANE_KIND: &str = "turnstone.answers";
pub const SOURCE_SCHEMA: &str = "turnstone.answers.v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerSourceV1 {
    pub session: uuid::Uuid,
}
pub fn answer_source(session: uuid::Uuid) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 1,
            payload: serde_json::to_value(AnswerSourceV1 { session }).unwrap(),
        },
    })
}
#[derive(Clone, Default)]
pub(crate) struct AnswerConnections(Rc<RefCell<BTreeMap<String, Weak<InferenceService>>>>);
pub struct AnswerProvider {
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    owner: Arc<dyn RetainedSourceOwner>,
    wake: armillary::Wake,
    connections: AnswerConnections,
}
impl AnswerProvider {
    pub(crate) fn new(owner: Arc<dyn RetainedSourceOwner>, wake: armillary::Wake) -> Self {
        Self {
            kind: PaneKindId::new(PANE_KIND),
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SOURCE_SCHEMA),
                label: "Answers".into(),
                accepted_source: SurfaceSourceShape::One(SourceKindId::from(SOURCE_SCHEMA)),
            },
            owner,
            wake,
            connections: Default::default(),
        }
    }
}
const CSS: &str = r#"
.answer-pane { box-sizing: border-box; padding: 12px; color: rgb(230,235,245); font-size: 14px; }
.answer-pane button { color: rgb(230,235,245); background-color: rgb(36,44,62); }
.answer-heading { font-size: 18px; margin-bottom: 8px; }
.answer-controls { display: flex; flex-wrap: wrap; gap: 8px; margin: 8px 0; }
.answer-button { padding: 6px 10px; border: 1px solid rgb(105,115,135); border-radius: 5px; }
.answer-body { white-space: pre-wrap; overflow-wrap: anywhere; margin: 12px 0; }
.answer-status { margin: 8px 0; }
.answer-source { padding: 10px; border: 1px solid rgb(100,110,135); margin: 10px 0; }
.answer-version { font-size: 12px; overflow-wrap: anywhere; }
.answer-history { margin: 12px 0; }
.answer-question { box-sizing: border-box; min-height: 32px; min-width: 160px; width: 100%; padding: 6px 8px; border: 1px solid rgb(105,115,135); border-radius: 5px; color: rgb(230,235,245); background-color: rgb(25,32,48); }
"#;
impl SurfaceProvider for AnswerProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.kind
    }
    fn source_schema(&self) -> &SourceSchemaId {
        &self.schema
    }
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn stylesheet(&self) -> &str {
        CSS
    }
    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let invalid = |message: String| SurfaceAdmissionError::InvalidPayload {
            schema: self.schema.clone(),
            message,
        };
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(invalid("Answers need a browsing-session source.".into()));
        };
        if schema != &self.schema || payload.version != 1 {
            return Err(invalid("Unsupported answer source.".into()));
        }
        let source: AnswerSourceV1 =
            serde_json::from_value(payload.payload.clone()).map_err(|e| invalid(e.to_string()))?;
        let namespace = format!("turnstone/{}", source.session);
        let existing = self
            .connections
            .0
            .borrow()
            .get(&namespace)
            .and_then(Weak::upgrade);
        let service = match existing {
            Some(service) => service,
            None => {
                let service = InferenceService::start(
                    namespace.clone(),
                    self.owner.clone(),
                    self.wake.clone(),
                )
                .map_err(invalid)?;
                self.connections
                    .0
                    .borrow_mut()
                    .insert(namespace, Arc::downgrade(&service));
                service
            },
        };
        let state = AnswerState {
            snapshot: service.snapshot(),
            service,
            question: TextInput::default(),
            active: None,
            local_status: String::new(),
            source_open: false,
            width: 0.,
            height: 0.,
        };
        let runner = GenetAppRunner::new(dom, answer_view as fn(&AnswerState) -> AnswerView, state);
        Ok(Box::new(AnswerSession {
            descriptor: self.descriptor.clone(),
            runner,
        }))
    }
}
struct AnswerState {
    service: Arc<InferenceService>,
    snapshot: ClientSnapshot,
    question: TextInput,
    active: Option<String>,
    local_status: String,
    source_open: bool,
    width: f32,
    height: f32,
}
type AnswerView = Box<dyn AnyView<AnswerState, (), GenetCtx, GenetElement>>;
type AnswerRunner = GenetAppRunner<AnswerState, fn(&AnswerState) -> AnswerView, AnswerView, ()>;
fn row(text: impl Into<String>) -> AnswerView {
    Box::new(el::<_, AnswerState, ()>("div", text.into()))
}
fn button(
    label: impl Into<String>,
    callback: impl Fn(&mut AnswerState, PointerClick) + 'static,
) -> AnswerView {
    Box::new(clickable(
        el::<_, AnswerState, ()>("button", label.into()).attr("class", "answer-button"),
        callback,
    ))
}
fn state_name(state: RunStateV1) -> &'static str {
    match state {
        RunStateV1::Queued => "Queued",
        RunStateV1::Running => "Generating",
        RunStateV1::Paused => "Paused",
        RunStateV1::Finished => "Finished",
        RunStateV1::Cancelled => "Cancelled",
        RunStateV1::Failed => "Failed",
    }
}
fn selected(state: &AnswerState) -> Option<&RunV1> {
    state
        .active
        .as_ref()
        .and_then(|id| state.snapshot.runs.iter().find(|r| &r.run == id))
        .or_else(|| state.snapshot.runs.last())
}
fn ask(state: &mut AnswerState, _: PointerClick) {
    if !state.snapshot.ready {
        return;
    }
    match state.service.ask(state.question.text().to_owned()) {
        Ok(run) => {
            state.active = Some(run);
            state.source_open = false;
            state.local_status = "Sending question…".into();
        },
        Err(error) => state.local_status = error,
    }
}
fn answer_view(state: &AnswerState) -> AnswerView {
    let mut body: Vec<AnswerView> = vec![
        Box::new(el::<_, AnswerState, ()>("h2", "Answers").attr("class", "answer-heading")),
        row("Ask about retained text in this browsing session."),
        Box::new(
            el::<_, AnswerState, ()>("div", state.snapshot.status.clone())
                .attr("class", "answer-status")
                .attr(
                    "data-answer-connection",
                    if state.snapshot.ready {
                        "ready"
                    } else {
                        "waiting"
                    },
                ),
        ),
        row("Question"),
        Box::new(lens(
            |input: &mut TextInput| {
                text_field_typed(input)
                    .attr("aria-label", "Question")
                    .attr("data-answer-question", "true")
                    .attr("class", "answer-question")
            },
            |state: &mut AnswerState| &mut state.question,
        )),
    ];
    if !state.local_status.is_empty() {
        body.push(row(state.local_status.clone()));
    }
    let ask_button = if state.snapshot.ready {
        button("Ask", ask)
    } else {
        Box::new(el::<_, AnswerState, ()>("button", "Ask").attr("disabled", "disabled"))
            as AnswerView
    };
    body.push(Box::new(
        el::<_, AnswerState, ()>(
            "div",
            vec![
                ask_button,
                button("Refresh answers", |state, _| {
                    if let Err(e) = state.service.refresh() {
                        state.local_status = e;
                    }
                }),
            ],
        )
        .attr("class", "answer-controls"),
    ));
    if let Some(run) = selected(state) {
        let mut card: Vec<AnswerView> = vec![
            row(run.question.clone()),
            Box::new(
                el::<_, AnswerState, ()>(
                    "div",
                    format!("{} · {} tokens", state_name(run.state), run.tokens),
                )
                .attr("data-answer-state", state_name(run.state).to_lowercase())
                .attr("data-answer-tokens", run.tokens.to_string()),
            ),
            Box::new(
                el::<_, AnswerState, ()>("pre", run.text.clone())
                    .attr("class", "answer-body")
                    .attr("data-answer-text", "true"),
            ),
        ];
        if let Some(error) = &run.error {
            card.push(row(error.clone()));
        }
        let mut controls = Vec::new();
        if matches!(run.state, RunStateV1::Queued | RunStateV1::Running) {
            let id = run.run.clone();
            controls.push(button("Pause", move |state, _| {
                if let Err(e) = state.service.control(id.clone(), Control::Pause) {
                    state.local_status = e;
                }
            }));
        }
        if run.state == RunStateV1::Paused {
            let id = run.run.clone();
            controls.push(button("Resume", move |state, _| {
                if let Err(e) = state.service.control(id.clone(), Control::Resume) {
                    state.local_status = e;
                }
            }));
        }
        if !run.state.is_terminal() {
            let id = run.run.clone();
            controls.push(button("Cancel", move |state, _| {
                if let Err(e) = state.service.control(id.clone(), Control::Cancel) {
                    state.local_status = e;
                }
            }));
        }
        card.push(Box::new(
            el::<_, AnswerState, ()>("div", controls).attr("class", "answer-controls"),
        ));
        if let Some(evidence) = &run.recall {
            if !evidence.stale.is_empty() {
                card.push(row("Some retained sources changed or became unavailable. Their citations are withdrawn."));
            }
            if !evidence.unknown_citations.is_empty() {
                card.push(row("Some answer markers do not name supplied material."));
            }
            for citation in &evidence.citations {
                if let Some(supplied) = evidence
                    .passages
                    .iter()
                    .find(|p| p.handle == citation.handle && p.passage.id() == citation.passage)
                {
                    let id = run.run.clone();
                    let handle = citation.handle;
                    let basis = match citation.basis {
                        distillery::inference_protocol::recall::CitationBasis::ModelMarker => {
                            "Source marker"
                        },
                        distillery::inference_protocol::recall::CitationBasis::VerbatimPassage => {
                            "Exact quotation"
                        },
                    };
                    card.push(button(
                        format!("Source {handle}: {}", supplied.passage.source.source),
                        move |state, _| {
                            state.source_open = true;
                            if let Err(e) = state.service.source(id.clone(), handle) {
                                state.local_status = e;
                            }
                        },
                    ));
                    card.push(Box::new(
                        el::<_, AnswerState, ()>(
                            "div",
                            format!(
                                "{basis} · {} · characters {}..{}",
                                supplied.passage.version,
                                supplied.passage.selector.start,
                                supplied.passage.selector.end
                            ),
                        )
                        .attr("class", "answer-version"),
                    ));
                }
            }
        }
        if state.source_open
            && let Some(source) = state.snapshot.source.as_ref().filter(|s| s.run == run.run)
        {
            card.push(Box::new(
                el::<_, AnswerState, ()>(
                    "div",
                    vec![
                        row("Retained source version"),
                        row(source.retained.source.source.clone()),
                        Box::new(
                            el::<_, AnswerState, ()>(
                                "div",
                                format!(
                                    "{} · cited characters {}..{}",
                                    source.retained.version,
                                    source.selector.start,
                                    source.selector.end
                                ),
                            )
                            .attr("class", "answer-version"),
                        ),
                        Box::new(
                            el::<_, AnswerState, ()>("pre", source.retained.text.clone())
                                .attr("class", "answer-body")
                                .attr("data-answer-source", "true"),
                        ),
                        button("Close source", |state, _| state.source_open = false),
                    ],
                )
                .attr("class", "answer-source"),
            ));
        }
        body.push(Box::new(
            el::<_, AnswerState, ()>("section", card).attr("data-answer-run", run.run.clone()),
        ));
    }
    if !state.snapshot.runs.is_empty() {
        let mut history = vec![row("Past asks")];
        for run in state.snapshot.runs.iter().rev() {
            let id = run.run.clone();
            history.push(button(
                format!(
                    "Reopen: {} ({})",
                    run.question.chars().take(70).collect::<String>(),
                    state_name(run.state)
                ),
                move |state, _| {
                    state.active = Some(id.clone());
                    state.source_open = false;
                    state.local_status = String::new();
                },
            ));
        }
        body.push(Box::new(
            el::<_, AnswerState, ()>("div", history).attr("class", "answer-history"),
        ));
    }
    Box::new(
        el::<_, AnswerState, ()>("div", body)
            .attr("class", "answer-pane")
            .attr(
                "style",
                format!("width: {}px; min-height: {}px;", state.width, state.height),
            ),
    )
}
struct AnswerSession {
    descriptor: SurfaceDescriptor,
    runner: AnswerRunner,
}
impl AnswerSession {
    fn effects(&mut self) -> Vec<SurfaceEffect> {
        let mut effects = vec![SurfaceEffect::Redraw];
        if let Some(exit) = self.runner.take_focus_exit() {
            effects.push(SurfaceEffect::FocusExit(exit));
        }
        effects
    }
}
impl RetainedSurfaceSession for AnswerSession {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn availability(&self) -> SurfaceAvailability {
        SurfaceAvailability::Available
    }
    fn dom(&self) -> DomHandle {
        self.runner.dom()
    }
    fn root(&self) -> NodeId {
        self.runner.root()
    }
    fn focus(&self) -> Option<NodeId> {
        self.runner.focus()
    }
    fn set_focus(&mut self, node: Option<NodeId>) -> Vec<SurfaceEffect> {
        self.runner.set_focus(node);
        self.effects()
    }
    fn focus_traverse(&mut self, forward: bool) -> Vec<SurfaceEffect> {
        self.runner.focus_traverse(forward);
        self.effects()
    }
    fn focusables(&self) -> Vec<NodeId> {
        self.runner.focusables()
    }
    fn pointer_capture(&self) -> Option<NodeId> {
        self.runner.pointer_capture()
    }
    fn pointer_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.pointer_target(hit)
    }
    fn hover_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.hover_target(hit)
    }
    fn wheel_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.wheel_target(hit)
    }
    fn set_focus_exits(&mut self, exits: bool) -> bool {
        self.runner.set_focus_exits(exits);
        true
    }
    fn sync_viewport(&mut self, viewport: SurfaceViewport) -> Vec<SurfaceEffect> {
        let next = self.runner.state().service.snapshot();
        let state = self.runner.state();
        if state.snapshot.revision == next.revision
            && state.width == viewport.width
            && state.height == viewport.height
        {
            return Vec::new();
        }
        self.runner.update(|state| {
            if state.snapshot.revision != next.revision {
                state.local_status.clear();
            }
            state.snapshot = next;
            state.width = viewport.width;
            state.height = viewport.height;
        });
        self.effects()
    }
    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> Vec<SurfaceEffect> {
        match event {
            ResolvedSurfaceEvent::Click { target, event } => {
                self.runner.dispatch_click(target, event);
            },
            ResolvedSurfaceEvent::Key(event) => {
                self.runner.dispatch_key(event);
            },
            ResolvedSurfaceEvent::PointerDown { target, event } => {
                self.runner.dispatch_pointer_down(target, event);
            },
            ResolvedSurfaceEvent::PointerMove(event) => {
                self.runner.dispatch_pointer_move(event);
            },
            ResolvedSurfaceEvent::PointerUp(event) => {
                self.runner.dispatch_pointer_up(event);
            },
            ResolvedSurfaceEvent::Hover { target, event } => {
                self.runner.dispatch_hover(target, event);
            },
            ResolvedSurfaceEvent::Wheel { target, event } => {
                self.runner.dispatch_wheel(target, event);
            },
        }
        self.effects()
    }
}
