// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native Turnstone pane for an admitted resident mere.

use cambium::{
    DomHandle, GenetAppRunner, ResolvedSurfaceEvent, RetainedSurfaceSession, SurfaceEffect,
    SurfaceViewport, el,
};
use chirograph::PortableCardV1;
use genet_scripted_dom::NodeId;
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId,
    SurfaceSourceShape, SurfaceUnavailableReason,
};
use mere_view::{
    HostAction, MereView, MereViewElement, MereViewModel, MereViewRequest, MereViewState,
    NodeState, StatusNote, ViewStatus,
};
use ostiary::app_admission::AppRouteId;
use serde::Deserialize;
use sprigging::{ColorF, LeafRegistry};

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

mod worker;

pub(crate) const ADDRESS_PREFIX: &str = "mere://resident/";
pub(crate) const PANE_KIND: &str = "turnstone.resident-graph";
pub(crate) const LABEL: &str = "Resident graph";
pub(crate) const SOURCE_SCHEMA: &str = "turnstone.resident-mere.v1";
const GRAPH_LEAF: u64 = 0x6d65_7265;
const RECONNECT: &str = "reconnect";
const CSS: &str = "
.resident-graph { display: flex; flex-direction: column; color: var(--tabard-color-text, rgb(220, 226, 238)); }
.resident-graph .mere-view-bar { padding-inline: 8px; }
.resident-graph button:not(.graph-canvas-swatch-node) { color: inherit; background: var(--tabard-color-surface, rgb(28, 34, 50)); border: 1px solid var(--tabard-color-text-dim, rgb(120, 130, 150)); border-radius: 4px; padding: 3px 6px; }
.resident-graph button[aria-pressed=true]:not(.graph-canvas-swatch-node) { background: var(--tabard-color-surface-2, rgb(45, 58, 81)); border-color: var(--tabard-color-primary, rgb(116, 166, 223)); }
.resident-graph .mere-view-main > [role=status] { padding-inline: 8px; }
.resident-graph-detail { box-sizing: border-box; padding: 6px 8px; height: 104px; overflow: auto; }
.resident-graph-detail h3, .resident-graph-detail p { margin: 0 0 4px; overflow-wrap: anywhere; }
.resident-graph .mere-view-session, .resident-graph .mere-view-archive { padding: 4px; }
.resident-graph .mere-view-notice { color: inherit; }
";

#[derive(Deserialize)]
struct ResidentSource {
    domain: String,
}

pub(crate) fn source(domain: &str) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 1,
            payload: serde_json::json!({ "domain": domain }),
        },
    })
}

pub(crate) struct ResidentGraphProvider {
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    stylesheet: String,
    wake: armillary::Wake,
}

impl ResidentGraphProvider {
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
                "{} {} {CSS}",
                cambium::GRAPH_CANVAS_SWATCH_CSS,
                mere_view::MERE_VIEW_CSS,
            ),
            wake,
        }
    }
}

impl SurfaceProvider for ResidentGraphProvider {
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
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let invalid = |message| SurfaceAdmissionError::InvalidPayload {
            schema: self.schema.clone(),
            message,
        };
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(invalid("The pane needs a resident mere domain.".into()));
        };
        if schema != &self.schema || payload.version != 1 {
            return Err(invalid(
                "This resident graph source version is unsupported.".into(),
            ));
        }
        let source: ResidentSource = serde_json::from_value(payload.payload.clone())
            .map_err(|error| invalid(error.to_string()))?;
        let route = resident_route(&source.domain);
        let mut state = State {
            route,
            wake: self.wake.clone(),
            connection: None,
            model: MereViewModel {
                title: format!("Resident graph: {}", source.domain),
                ..Default::default()
            },
            view: MereViewState::default(),
            focused: None,
            record: None,
            requested: 0,
            completed: 0,
            size: (1, 1),
        };
        state.connect();
        Ok(Box::new(GraphSession {
            descriptor: self.descriptor.clone(),
            runner: GenetAppRunner::new(dom, view as fn(&State) -> GraphView, state),
            leaves: LeafRegistry::new(),
            leaf_dirty: true,
        }))
    }
}

fn resident_route(domain: &str) -> Result<AppRouteId, String> {
    if domain.is_empty()
        || !domain
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(
            "Enter a resident graph domain using letters, digits, hyphens or underscores.".into(),
        );
    }
    AppRouteId::new(format!("mere/{domain}")).map_err(|error| error.to_string())
}

struct State {
    route: Result<AppRouteId, String>,
    wake: armillary::Wake,
    connection: Option<worker::Connection>,
    model: MereViewModel,
    view: MereViewState,
    focused: Option<String>,
    record: Option<PortableCardV1>,
    requested: u64,
    completed: u64,
    size: (u32, u32),
}

impl State {
    fn connect(&mut self) {
        let result = self
            .route
            .clone()
            .and_then(|route| worker::Connection::start(route, self.wake.clone()));
        match result {
            Ok(connection) => {
                self.connection = Some(connection);
                self.model.status = ViewStatus::Building(StatusNote {
                    message: "Opening this resident graph…".into(),
                    action: None,
                });
                self.model.notice = None;
                self.requested = 0;
                self.completed = 0;
            },
            Err(reason) => self.fail(reason),
        }
    }

    fn fail(&mut self, reason: String) {
        self.connection = None;
        self.model.status = ViewStatus::Unavailable(StatusNote {
            message: reason,
            action: self
                .route
                .is_ok()
                .then(|| HostAction::new(RECONNECT, "Reconnect")),
        });
        self.model.notice = None;
        self.record = None;
        self.focused = None;
        self.view = MereViewState::default();
    }

    fn request(&mut self, request: MereViewRequest) {
        if matches!(&request, MereViewRequest::Host(key) if key == RECONNECT) {
            self.connect();
        } else if self.requested != self.completed
            || matches!(self.model.status, ViewStatus::Building(_))
        {
            self.model.notice = Some("Wait for the resident's current operation.".into());
        } else if let Some(connection) = &self.connection {
            if connection.requests.try_send(request).is_ok() {
                self.requested += 1;
                self.model.notice = None;
            } else {
                self.fail("The resident graph connection closed. Reconnect to continue.".into());
            }
        }
    }

    fn update_model(&mut self, update: worker::Update) {
        match update {
            worker::Update::Opening => {},
            worker::Update::Failed(reason) => self.fail(reason),
            worker::Update::Read(read) => {
                self.model = read.model;
                if self.focused != read.focused {
                    self.view.focused.clone_from(&read.focused);
                }
                self.focused = read.focused;
                self.record = read.record;
                self.completed = read.completed;
            },
        }
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
}

type GraphView = MereViewElement<State, ()>;
type Runner = GenetAppRunner<State, fn(&State) -> GraphView, GraphView, ()>;

fn view(state: &State) -> GraphView {
    let graph = mere_view::mere_view(
        state.mere_view(),
        |state: &mut State, event| {
            state
                .view
                .apply_and_lay_out(event, &state.model, state.size.0, state.size.1)
        },
        State::request,
    );
    let title = state
        .record
        .as_ref()
        .map_or("Select a record to see its details.", |card| {
            card.title.as_str()
        });
    let fields: Vec<GraphView> = state
        .record
        .iter()
        .flat_map(|card| &card.values)
        .map(|field| {
            Box::new(el::<_, State, ()>(
                "p",
                format!("{}: {}", field.label, field.value),
            )) as GraphView
        })
        .collect();
    let details: GraphView = if matches!(state.model.status, ViewStatus::Ready) {
        Box::new(
            el::<_, State, ()>("section", (el("h3", title.to_owned()), el("div", fields)))
                .attr("class", "resident-graph-detail")
                .attr("aria-label", "Record details")
                .attr("role", "region"),
        )
    } else {
        Box::new(el::<_, State, ()>("div", ()).attr("style", "display:none"))
    };
    Box::new(el::<_, State, ()>("div", (graph, details)).attr("class", "resident-graph"))
}

struct GraphSession {
    descriptor: SurfaceDescriptor,
    runner: Runner,
    leaves: LeafRegistry<u64>,
    leaf_dirty: bool,
}

impl GraphSession {
    fn effects(&mut self) -> Vec<SurfaceEffect> {
        let mut effects = vec![SurfaceEffect::Redraw];
        if let Some(exit) = self.runner.take_focus_exit() {
            effects.push(SurfaceEffect::FocusExit(exit));
        }
        effects
    }
}

impl RetainedSurfaceSession for GraphSession {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn availability(&self) -> SurfaceAvailability {
        match &self.runner.state().model.status {
            ViewStatus::Unavailable(note) => SurfaceAvailability::Unavailable(
                SurfaceUnavailableReason::Other(note.message.clone()),
            ),
            _ => SurfaceAvailability::Available,
        }
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
            (viewport.height as u32).saturating_sub(104).max(1),
        );
        let state = self.runner.state();
        let changed = state
            .connection
            .as_ref()
            .is_some_and(|connection| !matches!(connection.updates.has_changed(), Ok(false)));
        if !changed && state.size == size {
            return Vec::new();
        }
        self.runner.update(|state| {
            if changed && let Some(connection) = &mut state.connection {
                let update = connection.updates.borrow_and_update().clone();
                state.update_model(update);
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
        if !std::mem::take(&mut self.leaf_dirty) {
            return Some(&mut self.leaves);
        }
        let leaf = self.runner.state().mere_view().paint_leaf(|kind| {
            let (r, g, b) = match kind.state() {
                NodeState::Available => (0.41, 0.67, 0.86),
                NodeState::Unavailable => (0.58, 0.59, 0.63),
                NodeState::Open => (0.48, 0.78, 0.6),
                NodeState::Dirty => (0.9, 0.66, 0.34),
            };
            ColorF { r, g, b, a: 1.0 }
        });
        self.leaves.insert(GRAPH_LEAF, Box::new(leaf));
        Some(&mut self.leaves)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{action::Action, ring::Ring};

    #[test]
    fn resident_addresses_keep_one_domain_and_require_the_host_ring() {
        assert_eq!(
            resident_route("Field_notes-2").unwrap().as_str(),
            "mere/Field_notes-2"
        );
        for domain in ["", "../notes", "notes/other", "notes?foo", "two words"] {
            assert!(resident_route(domain).is_err(), "{domain}");
        }
        assert_eq!(
            crate::ring::ring_of(&Action::OpenAddress(format!("{ADDRESS_PREFIX}notes"))),
            Ring::HostOnly
        );
    }
}
