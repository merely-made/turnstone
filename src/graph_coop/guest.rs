// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native, local presentation of a graph disclosed through an invitation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cambium::{
    DomHandle, GenetAppRunner, ResolvedSurfaceEvent, RetainedSurfaceSession, SurfaceEffect,
    SurfaceViewport, el,
};
use chirograph::PortableCardV1;
use genet_scripted_dom::NodeId;
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};
use mere_view::{MereView, MereViewModel, MereViewRequest, MereViewState, StatusNote, ViewStatus};
use sprigging::LeafRegistry;

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

use super::{Child, controls, worker};

pub(crate) const PANE_KIND: &str = "turnstone.invited-graph";
pub(crate) const SOURCE_SCHEMA: &str = "turnstone.invited-graph.v1";
pub(crate) const LABEL: &str = "Invited graph";
const GRAPH_LEAF: u64 = 0x636f_6f70;
const CONTROL_HEIGHT: u32 = 232;
const DETAIL_HEIGHT: u32 = 104;
const CSS: &str = "
.graph-coop-guest-controls { box-sizing: border-box; height: 232px; flex-shrink: 0; overflow: auto; }
.graph-coop-guest-controls .coop-panel { display: flex; flex-wrap: wrap; column-gap: 8px; align-items: center; }
.graph-coop-guest-controls .coop-panel h2 { flex: 1; }
.graph-coop-guest-controls .coop-panel h2, .graph-coop-guest-controls #coop-reconnect { order: -1; }
.graph-coop-guest-controls .coop-panel p { flex-basis: 100%; }
.graph-coop-guest-controls .coop-panel label { flex: 1 1 calc(50% - 4px); min-width: 0; }
";

/// The files are selected on the command line and held only for this process.
/// A saved pane source carries neither their paths nor their contents.
#[derive(Clone)]
pub(crate) struct Visit {
    pub(crate) invitation: PathBuf,
    pub(crate) contact: PathBuf,
}

pub(crate) fn source() -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 1,
            payload: serde_json::json!({}),
        },
    })
}

pub(crate) struct Provider {
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    stylesheet: String,
    wake: armillary::Wake,
    visit: Option<Visit>,
}

impl Provider {
    pub(crate) fn new(wake: armillary::Wake) -> Self {
        Self {
            kind: PaneKindId::new(PANE_KIND),
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SOURCE_SCHEMA),
                label: LABEL.into(),
                accepted_source: SurfaceSourceShape::One(SourceKindId::from(SOURCE_SCHEMA)),
            },
            stylesheet: format!(
                "{} {} {} {} {CSS}",
                cambium::GRAPH_CANVAS_SWATCH_CSS,
                mere_view::MERE_VIEW_CSS,
                crate::resident_graph::CSS,
                controls::CSS,
            ),
            wake,
            visit: None,
        }
    }

    pub(crate) fn with_visit(mut self, visit: Option<Visit>) -> Self {
        self.visit = visit;
        self
    }
}

impl SurfaceProvider for Provider {
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
        &self.stylesheet
    }

    fn admit(
        &self,
        pane: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        if pane != &source() {
            return Err(SurfaceAdmissionError::InvalidPayload {
                schema: self.schema.clone(),
                message: "This invited graph source is unsupported.".into(),
            });
        }

        let dirty = Arc::new(AtomicBool::new(false));
        let panel = self.visit.as_ref().map(|visit| {
            let dirty = dirty.clone();
            let wake = self.wake.clone();
            worker::Panel::new(
                String::new(),
                Some((visit.invitation.clone(), visit.contact.clone())),
                Arc::new(move || {
                    dirty.store(true, Ordering::Release);
                    wake();
                }),
            )
        });
        let status = match &panel {
            Some(panel) => ViewStatus::Building(StatusNote {
                message: panel.update.status.clone(),
                action: None,
            }),
            None => ViewStatus::Empty(StatusNote {
                message: "Open your invitation and contact files to connect to this graph.".into(),
                action: None,
            }),
        };
        let state = State {
            panel,
            model: MereViewModel {
                title: LABEL.into(),
                status,
                ..Default::default()
            },
            view: MereViewState::default(),
            focused: None,
            cards: HashMap::new(),
            size: (1, 1),
        };
        Ok(Box::new(Session {
            descriptor: self.descriptor.clone(),
            runner: GenetAppRunner::new(dom, view as fn(&State) -> Child<State>, state),
            dirty,
            leaves: LeafRegistry::new(),
            leaf_dirty: true,
        }))
    }
}

struct State {
    panel: Option<worker::Panel>,
    model: MereViewModel,
    view: MereViewState,
    focused: Option<String>,
    cards: HashMap<String, PortableCardV1>,
    size: (u32, u32),
}

impl State {
    fn request(&mut self, request: MereViewRequest) {
        match request {
            MereViewRequest::Activate(key) if self.cards.contains_key(&key) => {
                self.focused = Some(key);
            },
            MereViewRequest::Layout(layout) => {
                self.model.layout = layout;
                self.view.lay_out(&self.model, self.size.0, self.size.1);
            },
            _ => {},
        }
    }

    fn refresh(&mut self) {
        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        if !panel.drain() {
            return;
        }

        let layout = self.model.layout.clone();
        if let Some(data) = &panel.update.data {
            let (model, cards) = worker::guest_reading(data);
            self.model = model;
            self.cards = cards;
            if self
                .focused
                .as_ref()
                .is_some_and(|key| !self.cards.contains_key(key))
            {
                self.focused = None;
            }
            if self
                .view
                .focused
                .as_ref()
                .is_some_and(|key| !self.cards.contains_key(key))
            {
                self.view.focused = None;
            }
            if self
                .view
                .hovered
                .as_ref()
                .is_some_and(|key| !self.cards.contains_key(key))
            {
                self.view.hovered = None;
            }
        } else {
            self.model = MereViewModel {
                title: LABEL.into(),
                status: ViewStatus::Unavailable(StatusNote {
                    message: panel.update.status.clone(),
                    action: None,
                }),
                ..Default::default()
            };
            self.cards.clear();
            self.focused = None;
            self.view = MereViewState::default();
        }
        self.model.layout = layout;
    }

    fn mere_view(&self) -> MereView<'_> {
        MereView {
            model: &self.model,
            state: &self.view,
            leaf_key: GRAPH_LEAF,
            width: self.size.0,
            height: self.size.1,
        }
    }

    fn record(&self) -> Option<&PortableCardV1> {
        self.cards.get(self.focused.as_ref()?)
    }

    fn control_context(&self) -> controls::Context {
        controls::Context {
            attached: None,
            focused: self.focused.as_deref().and_then(|key| key.parse().ok()),
            busy: false,
        }
    }
}

fn view(state: &State) -> Child<State> {
    let controls = state.panel.is_some().then(|| {
        el(
            "div",
            controls::view(
                state,
                |state: &State| state.panel.as_ref(),
                |state: &mut State| state.panel.as_mut(),
                State::control_context,
            ),
        )
        .attr("class", "graph-coop-guest-controls")
    });
    let graph = mere_view::mere_view(
        state.mere_view(),
        |state: &mut State, event| {
            state
                .view
                .apply_and_lay_out(event, &state.model, state.size.0, state.size.1)
        },
        State::request,
    );

    let record = state.record();
    let detail = matches!(state.model.status, ViewStatus::Ready).then(|| {
        let title = record.map_or("Select a record to see its details.", |card| &card.title);
        let fields: Vec<Child<State>> = record
            .into_iter()
            .flat_map(|card| &card.values)
            .map(|field| {
                Box::new(el("p", format!("{}: {}", field.label, field.value))) as Child<State>
            })
            .collect();
        el("section", (el("h3", title.to_owned()), fields))
            .attr("class", "resident-graph-detail")
            .attr("aria-label", "Record details")
            .attr("role", "region")
    });
    let observation = controls::observe(
        state.panel.as_ref(),
        &state.model,
        state.focused.as_deref(),
        record,
    );
    Box::new(
        el("div", (controls, graph, detail))
            .attr("class", "resident-graph")
            .attr("data-coop-observation", observation),
    )
}

type Runner = GenetAppRunner<State, fn(&State) -> Child<State>, Child<State>, ()>;

struct Session {
    descriptor: SurfaceDescriptor,
    runner: Runner,
    dirty: Arc<AtomicBool>,
    leaves: LeafRegistry<u64>,
    leaf_dirty: bool,
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
        // The transport's status lives in the view so Reconnect stays usable.
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
        let size = (
            viewport.width.max(1.0) as u32,
            (viewport.height as u32)
                .saturating_sub(DETAIL_HEIGHT + CONTROL_HEIGHT)
                .max(96),
        );
        let changed = self.dirty.swap(false, Ordering::AcqRel);
        if !changed && self.runner.state().size == size {
            return Vec::new();
        }
        self.runner.update(|state| {
            if changed {
                state.refresh();
            }
            state.size = size;
            state.view.lay_out(&state.model, size.0, size.1);
        });
        self.leaf_dirty = true;
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
        self.leaf_dirty = true;
        self.effects()
    }

    fn leaves(&mut self) -> Option<&mut LeafRegistry<u64>> {
        if std::mem::take(&mut self.leaf_dirty) {
            let leaf = self
                .runner
                .state()
                .mere_view()
                .paint_leaf(crate::resident_graph::node_color);
            self.leaves.insert(GRAPH_LEAF, Box::new(leaf));
        }
        Some(&mut self.leaves)
    }
}
