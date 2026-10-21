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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerSourceV2 {
    pub session: uuid::Uuid,
    pub view: uuid::Uuid,
    pub chosen: Vec<crate::research::Target>,
}
pub fn answer_source(session: uuid::Uuid) -> PaneSource {
    answer_source_value(AnswerSourceV2 {
        session,
        view: uuid::Uuid::new_v4(),
        chosen: Vec::new(),
    })
}
pub(crate) fn answer_source_value(value: AnswerSourceV2) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 2,
            payload: serde_json::to_value(value).unwrap(),
        },
    })
}
#[derive(Clone, Default)]
pub(crate) struct AnswerConnections(Rc<RefCell<BTreeMap<String, Weak<InferenceService>>>>);
#[derive(Clone, Default)]
struct Draft {
    question: TextInput,
    active: Option<String>,
}
pub struct AnswerProvider {
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    owner: Arc<dyn RetainedSourceOwner>,
    wake: armillary::Wake,
    connections: AnswerConnections,
    bridge: crate::research::Bridge,
    drafts: RefCell<BTreeMap<String, std::rc::Weak<RefCell<Draft>>>>,
}
impl AnswerProvider {
    pub(crate) fn new(
        owner: Arc<dyn RetainedSourceOwner>,
        wake: armillary::Wake,
        bridge: crate::research::Bridge,
    ) -> Self {
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
            drafts: Default::default(),
            bridge,
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
        if schema != &self.schema || payload.version != 2 {
            return Err(invalid(
                "Open Ask saved sources to choose an explicit saved-source scope.".into(),
            ));
        }
        let source: AnswerSourceV2 =
            serde_json::from_value(payload.payload.clone()).map_err(|e| invalid(e.to_string()))?;
        let context = self.bridge.context().map_err(invalid)?;
        if *context.session.as_uuid() != source.session {
            return Err(invalid(
                "Answers belong to another browsing session.".into(),
            ));
        }
        let namespace = crate::research::namespace(context.session);
        let connection_key = format!("{namespace}/{}", context.generation);
        let existing = self
            .connections
            .0
            .borrow()
            .get(&connection_key)
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
                    .insert(connection_key, Arc::downgrade(&service));
                service
            },
        };
        let draft_key = format!("{namespace}/{}/{}", context.generation, source.view);
        let draft = self
            .drafts
            .borrow()
            .get(&draft_key)
            .and_then(std::rc::Weak::upgrade)
            .unwrap_or_else(|| Rc::new(RefCell::new(Draft::default())));
        self.drafts
            .borrow_mut()
            .insert(draft_key, Rc::downgrade(&draft));
        let previous = draft.borrow().clone();
        let state = AnswerState {
            snapshot: service.snapshot(),
            source,
            context,
            bridge: self.bridge.clone(),
            service,
            draft,
            question: previous.question,
            active: previous.active,
            local_status: String::new(),
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
    draft: Rc<RefCell<Draft>>,
    source: AnswerSourceV2,
    context: crate::research::Context,
    bridge: crate::research::Bridge,
    service: Arc<InferenceService>,
    snapshot: ClientSnapshot,
    question: TextInput,
    active: Option<String>,
    local_status: String,
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
    if state.source.chosen.is_empty() {
        state.local_status = "Choose at least one saved source before asking.".into();
        return;
    }
    match state.service.ask(
        state.question.text().to_owned(),
        state.source.chosen.clone(),
    ) {
        Ok(run) => {
            state.active = Some(run);
            state.question = TextInput::default();
            state.local_status = "Sending question…".into();
        },
        Err(error) => state.local_status = error,
    }
}
fn answer_view(state: &AnswerState) -> AnswerView {
    let mut body: Vec<AnswerView> = vec![
        Box::new(el::<_, AnswerState, ()>("h2", "Answers").attr("class", "answer-heading")),
        row("Ask only about the saved sources you choose below."),
        row(format!(
            "{} saved sources chosen",
            state.source.chosen.len()
        )),
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
    let mut choices = vec![row("Saved sources for this question")];
    for choice in &state.snapshot.choices {
        if let Some(target) = &choice.target {
            let target = target.clone();
            let chosen = state.source.chosen.contains(&target);
            choices.push(button(
                format!(
                    "{}: {}",
                    if chosen {
                        "Remove source"
                    } else {
                        "Choose source"
                    },
                    choice.label
                ),
                move |state, _| {
                    let mut chosen = state.source.chosen.clone();
                    if chosen.contains(&target) {
                        chosen.retain(|item| item != &target);
                    } else {
                        chosen.push(target.clone());
                    }
                    state.bridge.send(crate::research::Intent::Selection {
                        context: state.context.clone(),
                        expected: state.source.clone(),
                        chosen,
                    });
                },
            ));
        } else {
            choices.push(row(format!(
                "Unavailable: {} · {}",
                choice.label,
                choice.error.as_deref().unwrap_or("Unreadable saved source")
            )));
        }
    }
    for chosen in &state.source.chosen {
        if !state
            .snapshot
            .choices
            .iter()
            .any(|choice| choice.target.as_ref() == Some(chosen))
        {
            let missing = chosen.clone();
            choices.push(button(
                format!(
                    "Remove unavailable source: {}",
                    chosen.identity()
                ),
                move |state, _| {
                    let chosen = state
                        .source
                        .chosen
                        .iter()
                        .filter(|target| *target != &missing)
                        .cloned()
                        .collect();
                    state.bridge.send(crate::research::Intent::Selection {
                        context: state.context.clone(),
                        expected: state.source.clone(),
                        chosen,
                    });
                },
            ));
        }
    }
    body.push(Box::new(
        el::<_, AnswerState, ()>("div", choices).attr("class", "answer-scope"),
    ));
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
            if evidence.citations.is_empty() && run.state == RunStateV1::Finished {
                card.push(row("No validated citations in this answer."));
            }
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
                        format!(
                            "Source {handle}: {}",
                            state
                                .snapshot
                                .choices
                                .iter()
                                .find(|choice| choice.target.as_ref().is_some_and(|target| {
                                    crate::research::source_id(target).ok().as_ref()
                                        == Some(&supplied.passage.source)
                                }))
                                .map(|choice| choice.label.as_str())
                                .unwrap_or("Saved source")
                        ),
                        move |state, _| {
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
                                supplied
                                    .passage
                                    .version
                                    .strip_prefix("blake3:")
                                    .unwrap_or(&supplied.passage.version)
                                    .chars()
                                    .take(12)
                                    .collect::<String>(),
                                supplied.passage.selector.start,
                                supplied.passage.selector.end
                            ),
                        )
                        .attr("class", "answer-version"),
                    ));
                }
            }
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
        let state = self.runner.state();
        *state.draft.borrow_mut() = Draft {
            question: state.question.clone(),
            active: state.active.clone(),
        };
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
            if state.local_status == "Sending question…"
                && state
                    .active
                    .as_ref()
                    .is_some_and(|id| next.runs.iter().any(|run| &run.run == id))
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    struct NoIo;
    impl RetainedSourceOwner for NoIo {
        fn snapshot(
            &self,
            _: &str,
        ) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
            unreachable!()
        }
        fn resolve(
            &self,
            _: &eidetic::models::recall::RecallSourceId,
            _: &str,
        ) -> Result<String, String> {
            unreachable!()
        }
    }
    #[test]
    fn research_panes_share_one_publisher_and_reject_foreign_or_legacy_sources() {
        let bridge = crate::research::Bridge::default();
        let context = crate::research::Context {
            session: crate::panes::SessionId::new(),
            generation: uuid::Uuid::new_v4(),
            admissions: Vec::new(),
            clips: Vec::new(),
        };
        bridge.project(context.clone());
        let provider = AnswerProvider::new(Arc::new(NoIo), Arc::new(|| {}), bridge);
        let namespace = crate::research::namespace(context.session);
        let service = InferenceService::inert_for_surface_test(namespace.clone());
        provider.connections.0.borrow_mut().insert(
            format!("{namespace}/{}", context.generation),
            Arc::downgrade(&service),
        );
        let dom = || Rc::new(RefCell::new(genet_scripted_dom::ScriptedDom::new()));
        let first = provider
            .admit(&answer_source(*context.session.as_uuid()), dom())
            .unwrap();
        let second = provider
            .admit(&answer_source(*context.session.as_uuid()), dom())
            .unwrap();
        assert_eq!(
            Arc::strong_count(&service),
            3,
            "two admitted runners borrow exactly one publisher"
        );
        assert!(
            provider
                .admit(&answer_source(uuid::Uuid::new_v4()), dom())
                .is_err()
        );
        let legacy = PaneSource::Fixed(SourceRef::External {
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            payload: SerializedSource {
                version: 1,
                payload: serde_json::json!({ "session": context.session.as_uuid() }),
            },
        });
        assert!(
            provider.admit(&legacy, dom()).is_err(),
            "legacy PageText source cannot become a capture ask implicitly"
        );
        drop(first);
        drop(second);
        assert_eq!(Arc::strong_count(&service), 1);
    }
}
