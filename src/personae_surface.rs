// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's mount for the shared Dramatis pane. Djinn owns the prompts.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use cambium::{
    DomHandle, GenetAppRunner, ResolvedSurfaceEvent, RetainedSurfaceSession, SurfaceEffect,
    SurfaceViewport, el,
};
use dramatis_surface::Worker;
use genet_scripted_dom::NodeId;
use graphshell_dramatis_pane::{Child, Pane};
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

pub(crate) const PANE_KIND: &str = "turnstone.personae";
pub(crate) const LABEL: &str = "Personae";
pub(crate) const SOURCE_SCHEMA: &str = "turnstone.personae.v1";

const CSS: &str = "
.personae-surface { color: var(--tabard-color-text, #e0e6ee); }
.personae-surface .dramatis-pane .card { border-color: var(--tabard-color-text-dim, #6d7783); }
.personae-surface .dramatis-pane button { color: inherit; background: var(--tabard-color-surface, #22262c); }
.personae-surface .dramatis-pane .card button[aria-pressed=true] { background: var(--tabard-color-surface-2, #303b50); }
";

pub(crate) fn source() -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 1,
            payload: serde_json::Value::Null,
        },
    })
}

pub(crate) struct PersonaeProvider {
    kind: PaneKindId,
    source_schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    stylesheet: String,
    wake: armillary::Wake,
}

impl PersonaeProvider {
    pub(crate) fn new(wake: armillary::Wake) -> Self {
        Self {
            kind: PaneKindId::new(PANE_KIND),
            source_schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SOURCE_SCHEMA),
                label: LABEL.into(),
                accepted_source: SurfaceSourceShape::One(SourceKindId::from(SOURCE_SCHEMA)),
            },
            stylesheet: format!("{} {CSS}", graphshell_dramatis_pane::SHEET),
            wake,
        }
    }
}

impl SurfaceProvider for PersonaeProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.kind
    }
    fn source_schema(&self) -> &SourceSchemaId {
        &self.source_schema
    }
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn stylesheet(&self) -> &str {
        &self.stylesheet
    }

    fn admit(
        &self,
        requested: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        if requested != &source() {
            return Err(SurfaceAdmissionError::InvalidPayload {
                schema: self.source_schema.clone(),
                message: "Personae opens the configured Djinn profile.".into(),
            });
        }
        let dirty = Arc::new(AtomicBool::new(false));
        let wake = self.wake.clone();
        let changed = Arc::clone(&dirty);
        let worker = Worker::spawn(
            dramatis_surface::endpoint(),
            Arc::new(move || {
                changed.store(true, Ordering::Release);
                wake();
            }),
        );
        let state = Pane::new(worker);
        Ok(Box::new(Session {
            descriptor: self.descriptor.clone(),
            runner: GenetAppRunner::new(dom, view as fn(&State) -> Child<State>, state),
            dirty,
        }))
    }
}

type State = Pane<Worker>;

fn view(pane: &State) -> Child<State> {
    Box::new(
        el(
            "div",
            graphshell_dramatis_pane::view(pane, |pane: &mut State| pane),
        )
        .attr("class", "personae-surface"),
    )
}

struct Session {
    descriptor: SurfaceDescriptor,
    runner: GenetAppRunner<State, fn(&State) -> Child<State>, Child<State>, ()>,
    dirty: Arc<AtomicBool>,
}

impl Session {
    fn effects(&mut self) -> Vec<SurfaceEffect> {
        let mut effects = vec![SurfaceEffect::Redraw];
        if let Some(exit) = self.runner.take_focus_exit() {
            effects.push(SurfaceEffect::FocusExit(exit));
        }
        effects
    }
}

impl RetainedSurfaceSession for Session {
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
    fn sync_viewport(&mut self, _: SurfaceViewport) -> Vec<SurfaceEffect> {
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return Vec::new();
        }
        self.runner.update(State::refresh);
        self.effects()
    }
    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> Vec<SurfaceEffect> {
        match event {
            ResolvedSurfaceEvent::Click { target, event } => {
                self.runner.dispatch_click(target, event)
            },
            ResolvedSurfaceEvent::Key(event) => self.runner.dispatch_key(event),
            ResolvedSurfaceEvent::PointerDown { target, event } => {
                self.runner.dispatch_pointer_down(target, event)
            },
            ResolvedSurfaceEvent::PointerMove(event) => self.runner.dispatch_pointer_move(event),
            ResolvedSurfaceEvent::PointerUp(event) => self.runner.dispatch_pointer_up(event),
            ResolvedSurfaceEvent::Hover { target, event } => {
                self.runner.dispatch_hover(target, event)
            },
            ResolvedSurfaceEvent::Wheel { target, event } => {
                self.runner.dispatch_wheel(target, event)
            },
        };
        self.effects()
    }
}
