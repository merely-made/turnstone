//! Provider-neutral retained surfaces contributed to Turnstone panes.
//!
//! Products own source decoding and the concrete Cambium runner. Turnstone
//! owns admission, retained layout, viewport scrolling, hit testing, and the
//! small amount of event routing needed to get from a pane coordinate to a
//! retained session.

use std::collections::HashMap;
use std::fmt;

use cambium::{
    DomHandle, GenetAppRunner, GenetCtx, GenetElement, HoverEvent, HoverPhase, KeyEvent,
    PointerClick, PointerEvent, PointerPhase, ResolvedSurfaceEvent, RetainedSurfaceSession,
    RunnerSurfaceSession, SurfaceEffect, SurfaceViewport, View, WheelEvent, el,
};
use genet_host_api::{
    SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape, SurfaceUnavailableReason,
};
use genet_scripted_dom::{NodeId, ScriptedDom};

use crate::panes::{PaneId, PaneKindId, PaneSource, PaneSpec, SourceRef, SourceSchemaId};
use crate::ui::{PaneScroll, RetainedLayout};

const UNAVAILABLE_SURFACE_CSS: &str = ".contributed-surface-unavailable { padding: 12px; }";

/// The result of applying effects requested by a retained session.
///
/// `Redraw` is the current retained-surface effect and changes the host's
/// immediate frame request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceRequest {
    #[default]
    None,
    Redraw,
}

/// Why a provider could not be admitted for a pane/source pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceAdmissionError {
    /// No registered provider accepts the requested pane kind and source.
    ProviderNotFound {
        pane_kind: PaneKindId,
        source_schema: Option<SourceSchemaId>,
    },
    /// The source is not of the schema published by the provider.
    InvalidSource {
        expected: SourceSchemaId,
        actual: Option<SourceSchemaId>,
    },
    /// The source names the right schema but its versioned payload is invalid.
    InvalidPayload {
        schema: SourceSchemaId,
        message: String,
    },
    /// The product had the right identity but cannot currently supply a
    /// session. Availability stays typed and inspectable on admitted sessions.
    Unavailable { reason: SurfaceUnavailableReason },
}

impl fmt::Display for SurfaceAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderNotFound {
                pane_kind,
                source_schema,
            } => write!(
                f,
                "no surface provider for pane '{}' and source schema {:?}",
                pane_kind.as_str(),
                source_schema.as_ref().map(SourceSchemaId::as_str)
            ),
            Self::InvalidSource { expected, actual } => write!(
                f,
                "surface source schema {:?} does not match expected {:?}",
                actual.as_ref().map(SourceSchemaId::as_str),
                expected.as_str()
            ),
            Self::InvalidPayload { schema, message } => write!(
                f,
                "surface source payload for {:?} is invalid: {message}",
                schema.as_str()
            ),
            Self::Unavailable { reason } => write!(f, "surface unavailable: {reason:?}"),
        }
    }
}

impl std::error::Error for SurfaceAdmissionError {}

/// Why a provider registration was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceRegistrationError {
    /// A duplicate provider identity.
    Duplicate {
        pane_kind: PaneKindId,
        surface_id: SurfaceId,
    },
    /// The descriptor's declared source kind and the provider's admission
    /// schema disagree. The descriptor is the stated admission truth, so a
    /// divergence is a provider bug surfaced at registration rather than a
    /// silent second truth consulted at admission.
    SourceShapeMismatch {
        surface_id: SurfaceId,
        declared: Option<String>,
        schema: SourceSchemaId,
    },
}

impl fmt::Display for SurfaceRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate {
                pane_kind,
                surface_id,
            } => write!(
                f,
                "duplicate surface '{}' for pane '{}'",
                surface_id.as_str(),
                pane_kind.as_str()
            ),
            Self::SourceShapeMismatch {
                surface_id,
                declared,
                schema,
            } => write!(
                f,
                "surface '{}' declares accepted source {:?} but admits schema '{}'",
                surface_id.as_str(),
                declared,
                schema.as_str()
            ),
        }
    }
}

impl std::error::Error for SurfaceRegistrationError {}

/// A product-owned source-to-session factory.
///
/// The trait is object-safe on purpose: Turnstone keeps providers erased and
/// does not need a concrete product map or runner state type.
pub trait SurfaceProvider {
    fn pane_kind(&self) -> &PaneKindId;

    fn source_schema(&self) -> &SourceSchemaId;

    fn descriptor(&self) -> &SurfaceDescriptor;

    fn stylesheet(&self) -> &str;

    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError>;
}

/// Registry of product contributions. The registry has no concrete session
/// knowledge. A pane kind names exactly one route; descriptor identity is the
/// provider/surface pair published outside Turnstone.
#[derive(Default)]
pub struct SurfaceProviderRegistry {
    providers: Vec<Box<dyn SurfaceProvider>>,
}

impl SurfaceProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        provider: Box<dyn SurfaceProvider>,
    ) -> Result<(), SurfaceRegistrationError> {
        let declared = match &provider.descriptor().accepted_source {
            SurfaceSourceShape::One(kind) | SurfaceSourceShape::Many(kind) => {
                Some(kind.as_str().to_owned())
            }
            SurfaceSourceShape::None => None,
        };
        if declared.as_deref() != Some(provider.source_schema().as_str()) {
            return Err(SurfaceRegistrationError::SourceShapeMismatch {
                surface_id: provider.descriptor().surface_id.clone(),
                declared,
                schema: provider.source_schema().clone(),
            });
        }
        if self.providers.iter().any(|existing| {
            existing.pane_kind() == provider.pane_kind()
                || (existing.descriptor().provider_id == provider.descriptor().provider_id
                    && existing.descriptor().surface_id == provider.descriptor().surface_id)
        }) {
            return Err(SurfaceRegistrationError::Duplicate {
                pane_kind: provider.pane_kind().clone(),
                surface_id: provider.descriptor().surface_id.clone(),
            });
        }
        self.providers.push(provider);
        Ok(())
    }

    pub fn register_provider<P: SurfaceProvider + 'static>(
        &mut self,
        provider: P,
    ) -> Result<(), SurfaceRegistrationError> {
        self.register(Box::new(provider))
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn contains_pane_kind(&self, pane_kind: &PaneKindId) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.pane_kind() == pane_kind)
    }

    pub fn find(
        &self,
        pane_kind: &PaneKindId,
        source: &PaneSource,
    ) -> Option<&dyn SurfaceProvider> {
        self.providers
            .iter()
            .find(|provider| {
                provider.pane_kind() == pane_kind
                    && source_matches(provider.source_schema(), source)
            })
            .map(|provider| provider.as_ref())
    }

    /// Admit a provider with a fresh host-created `ScriptedDom` handle.
    pub fn admit(
        &self,
        pane_kind: &PaneKindId,
        source: &PaneSource,
    ) -> Result<ContributedSurfacePane, SurfaceAdmissionError> {
        let provider = self
            .providers
            .iter()
            .find(|provider| provider.pane_kind() == pane_kind)
            .ok_or_else(|| SurfaceAdmissionError::ProviderNotFound {
                pane_kind: pane_kind.clone(),
                source_schema: source_schema(source).cloned(),
            })?;
        if !source_matches(provider.source_schema(), source) {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: provider.source_schema().clone(),
                actual: source_schema(source).cloned(),
            });
        }
        let dom = fresh_dom();
        let session = match provider.admit(source, dom.clone()) {
            Ok(session) => session,
            Err(SurfaceAdmissionError::Unavailable { reason }) => {
                unavailable_surface_session(provider.descriptor().clone(), dom, reason)
            }
            Err(error) => return Err(error),
        };
        Ok(ContributedSurfacePane::new(
            pane_kind.clone(),
            source.clone(),
            session,
            provider.stylesheet(),
        ))
    }
}

fn fresh_dom() -> DomHandle {
    std::rc::Rc::new(std::cell::RefCell::new(ScriptedDom::new()))
}

fn source_schema(source: &PaneSource) -> Option<&SourceSchemaId> {
    match source {
        PaneSource::Fixed(SourceRef::External { schema, .. }) => Some(schema),
        _ => None,
    }
}

fn source_matches(expected: &SourceSchemaId, source: &PaneSource) -> bool {
    source_schema(source).is_some_and(|actual| actual == expected)
}

struct UnavailableSurfaceState {
    label: String,
    reason: SurfaceUnavailableReason,
}

fn unavailable_surface_view(
    state: &UnavailableSurfaceState,
) -> impl View<UnavailableSurfaceState, (), GenetCtx, Element = GenetElement> + use<> {
    el::<_, UnavailableSurfaceState, ()>(
        "section",
        format!("{} unavailable: {:?}", state.label, state.reason),
    )
    .attr("class", "contributed-surface-unavailable")
    .attr("role", "status")
}

fn unavailable_surface_session(
    descriptor: SurfaceDescriptor,
    dom: DomHandle,
    reason: SurfaceUnavailableReason,
) -> Box<dyn RetainedSurfaceSession> {
    let state = UnavailableSurfaceState {
        label: descriptor.label.clone(),
        reason: reason.clone(),
    };
    let runner = GenetAppRunner::new(dom, unavailable_surface_view, state);
    Box::new(RunnerSurfaceSession::new(
        descriptor,
        runner,
        move |_state: &UnavailableSurfaceState| SurfaceAvailability::Unavailable(reason.clone()),
        |_state, _viewport| {},
        |_action: ()| Vec::new(),
    ))
}

/// A single admitted retained surface projected into a Turnstone pane.
pub struct ContributedSurfacePane {
    pane_kind: PaneKindId,
    source: PaneSource,
    /// The admitted session's retained DOM, kept beside the erased session so
    /// host diagnostics and semantic automation can borrow it without
    /// downcasting the provider-owned state.
    dom: DomHandle,
    session: Box<dyn RetainedSurfaceSession>,
    layout: RetainedLayout,
    scroll: PaneScroll,
    stylesheet: String,
    viewport: (u32, u32, f32),
    hover: Option<NodeId>,
}

impl ContributedSurfacePane {
    pub fn new(
        pane_kind: PaneKindId,
        source: PaneSource,
        session: Box<dyn RetainedSurfaceSession>,
        stylesheet: impl Into<String>,
    ) -> Self {
        let dom = session.dom();
        Self {
            pane_kind,
            source,
            dom,
            session,
            layout: RetainedLayout::new(),
            scroll: PaneScroll::new(),
            stylesheet: format!(
                "{} {} {}",
                crate::ui::CAMBIUM_SHEET,
                UNAVAILABLE_SURFACE_CSS,
                stylesheet.into()
            ),
            viewport: (0, 0, 1.0),
            hover: None,
        }
    }

    pub fn descriptor(&self) -> &SurfaceDescriptor {
        self.session.descriptor()
    }

    pub fn matches(&self, pane_kind: &PaneKindId, source: &PaneSource) -> bool {
        &self.pane_kind == pane_kind && &self.source == source
    }

    pub fn availability(&self) -> SurfaceAvailability {
        self.session.availability()
    }

    pub fn session(&self) -> &dyn RetainedSurfaceSession {
        self.session.as_ref()
    }

    pub fn session_mut(&mut self) -> &mut dyn RetainedSurfaceSession {
        self.session.as_mut()
    }

    /// Borrow the provider-owned retained DOM through a stable, erased seam.
    ///
    /// Probe, accessibility, and other host readers should inspect this DOM
    /// rather than downcasting the concrete session or rebuilding its view.
    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// The complete stylesheet used to lay out and hit-test this pane.
    ///
    /// Semantic automation must resolve controls under the same rules as the
    /// presented surface, including both Turnstone chrome and provider CSS.
    pub fn stylesheet(&self) -> &str {
        &self.stylesheet
    }

    pub fn scroll(&self) -> &PaneScroll {
        &self.scroll
    }

    pub fn scroll_mut(&mut self) -> &mut PaneScroll {
        &mut self.scroll
    }

    pub fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    pub fn hover_target(&self) -> Option<NodeId> {
        self.hover
    }

    pub fn layout(&self) -> &RetainedLayout {
        &self.layout
    }

    pub(crate) fn accessibility_tree(
        &self,
    ) -> Option<(accesskit::TreeUpdate, HashMap<accesskit::NodeId, NodeId>)> {
        let dom = self.session.dom();
        let dom = dom.borrow();
        self.layout.accessibility_tree(&dom, self.session.focus())
    }

    pub(crate) fn accessibility_focus(&self) -> Option<NodeId> {
        self.session.focus()
    }

    /// Render using the same retained layout that subsequent hit testing uses.
    pub fn scene(&mut self, width: u32, height: u32, scale_factor: f32) -> netrender::Scene {
        self.viewport = (width, height, scale_factor);
        let effects = self.session.sync_viewport(SurfaceViewport {
            width: width as f32,
            height: height as f32,
            scale_factor,
        });
        self.apply(effects);
        let dom = self.session.dom();
        let mut dom = dom.borrow_mut();
        self.layout
            .scene_scrolled(&mut dom, &self.stylesheet, width, height, &mut self.scroll)
    }

    pub fn hit_test(&mut self, x: f32, y: f32) -> Option<NodeId> {
        let (width, height, _) = self.viewport;
        let dom = self.session.dom();
        let mut dom = dom.borrow_mut();
        self.layout.hit_test_scrolled(
            &mut dom,
            &self.stylesheet,
            width,
            height,
            x,
            y,
            &self.scroll,
        )
    }

    pub fn click(&mut self, x: f32, y: f32) -> SurfaceRequest {
        let Some(hit) = self.hit_test(x, y) else {
            return SurfaceRequest::None;
        };
        self.dispatch(ResolvedSurfaceEvent::Click {
            // Click dispatch owns DOM capture/target/bubble routing and may
            // discover an `on_click` handler on any ancestor of the hit node.
            // `pointer_target` is deliberately narrower: it resolves only an
            // `on_pointer` drag target and must not gate ordinary buttons.
            target: hit,
            event: PointerClick::at((x, y)),
        })
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) -> SurfaceRequest {
        self.pointer(PointerPhase::Down, x, y, true)
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) -> SurfaceRequest {
        self.pointer(PointerPhase::Move, x, y, false)
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) -> SurfaceRequest {
        self.pointer(PointerPhase::Up, x, y, false)
    }

    fn pointer(
        &mut self,
        phase: PointerPhase,
        x: f32,
        y: f32,
        resolve_target: bool,
    ) -> SurfaceRequest {
        let (width, height, _) = self.viewport;
        let event = PointerEvent::new(phase, (x, y), (width as f32, height as f32));
        if resolve_target {
            let Some(hit) = self.hit_test(x, y) else {
                return SurfaceRequest::None;
            };
            let Some(target) = self.session.pointer_target(hit) else {
                return SurfaceRequest::None;
            };
            self.dispatch(ResolvedSurfaceEvent::PointerDown { target, event })
        } else {
            self.dispatch(match phase {
                PointerPhase::Move => ResolvedSurfaceEvent::PointerMove(event),
                PointerPhase::Up => ResolvedSurfaceEvent::PointerUp(event),
                PointerPhase::Down => unreachable!("down is resolved above"),
            })
        }
    }

    pub fn hover(&mut self, phase: HoverPhase, x: f32, y: f32) -> SurfaceRequest {
        let (width, height, _) = self.viewport;
        let leaving = matches!(phase, HoverPhase::Leave);
        let target = match phase {
            HoverPhase::Leave => self.hover.take(),
            HoverPhase::Enter | HoverPhase::Move => self
                .hit_test(x, y)
                .and_then(|hit| self.session.hover_target(hit)),
        };
        let Some(target) = target else {
            return SurfaceRequest::None;
        };
        if !leaving {
            self.hover = Some(target);
        }
        self.dispatch(ResolvedSurfaceEvent::Hover {
            target,
            event: HoverEvent::new(phase, (x, y), (width as f32, height as f32)),
        })
    }

    /// Wheel policy is product-first: a product wheel target receives the
    /// event, and only an unhandled hit moves Turnstone's viewport scroll.
    pub fn wheel(&mut self, dx: f32, dy: f32, x: f32, y: f32) -> SurfaceRequest {
        let (width, height, _) = self.viewport;
        let target = self
            .hit_test(x, y)
            .and_then(|hit| self.session.wheel_target(hit));
        if let Some(target) = target {
            return self.dispatch(ResolvedSurfaceEvent::Wheel {
                target,
                event: WheelEvent::new((dx, dy), (x, y), (width as f32, height as f32)),
            });
        }
        self.scroll.nudge(dx, dy);
        SurfaceRequest::Redraw
    }

    pub fn key(&mut self, event: KeyEvent) -> SurfaceRequest {
        self.dispatch(ResolvedSurfaceEvent::Key(event))
    }

    pub fn focus(&mut self, node: Option<NodeId>) -> SurfaceRequest {
        let effects = self.session.set_focus(node);
        self.apply(effects)
    }

    pub fn focus_at(&mut self, x: f32, y: f32) -> SurfaceRequest {
        let target = self
            .hit_test(x, y)
            .and_then(|hit| self.session.pointer_target(hit));
        self.focus(target)
    }

    pub fn focus_traverse(&mut self, forward: bool) -> SurfaceRequest {
        let effects = self.session.focus_traverse(forward);
        self.apply(effects)
    }

    /// Route one platform accessibility action through the same semantic
    /// session paths used by the Cambium reference host. Focus only moves the
    /// cursor; Click performs activation without inventing a pointer hit.
    pub(crate) fn accessibility_action(
        &mut self,
        action: accesskit::Action,
        node: NodeId,
    ) -> SurfaceRequest {
        match action {
            accesskit::Action::Click => self.dispatch(ResolvedSurfaceEvent::Click {
                target: node,
                event: PointerClick::at((0.0, 0.0)),
            }),
            accesskit::Action::Focus => self.focus(Some(node)),
            _ => SurfaceRequest::None,
        }
    }

    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> SurfaceRequest {
        let effects = self.session.dispatch(event);
        self.apply(effects)
    }

    fn apply(&mut self, effects: Vec<SurfaceEffect>) -> SurfaceRequest {
        let redraw = effects
            .iter()
            .any(|effect| matches!(effect, SurfaceEffect::Redraw));
        if redraw {
            SurfaceRequest::Redraw
        } else {
            SurfaceRequest::None
        }
    }
}

/// All contributed product sessions retained by pane identity.
///
/// Re-resolving the same pane and source preserves product state. Re-pinning a
/// pane replaces the old session before the next frame, so a retained product
/// cannot silently keep authority over its previous source.
#[derive(Default)]
pub struct ContributedSurfaceSessions {
    panes: HashMap<PaneId, ContributedSurfacePane>,
}

impl ContributedSurfaceSessions {
    pub fn resolve(
        &mut self,
        spec: &PaneSpec,
        registry: &SurfaceProviderRegistry,
    ) -> Result<&mut ContributedSurfacePane, SurfaceAdmissionError> {
        let replace = self
            .panes
            .get(&spec.id)
            .is_none_or(|pane| !pane.matches(&spec.kind, &spec.source));
        if replace {
            let admitted = registry.admit(&spec.kind, &spec.source)?;
            self.panes.insert(spec.id, admitted);
        }
        Ok(self
            .panes
            .get_mut(&spec.id)
            .expect("a contributed pane was admitted above"))
    }

    pub fn get(&self, pane: PaneId) -> Option<&ContributedSurfacePane> {
        self.panes.get(&pane)
    }

    pub fn get_mut(&mut self, pane: PaneId) -> Option<&mut ContributedSurfacePane> {
        self.panes.get_mut(&pane)
    }

    pub fn remove(&mut self, pane: PaneId) -> Option<ContributedSurfacePane> {
        self.panes.remove(&pane)
    }

    pub fn len(&self) -> usize {
        self.panes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    pub fn any_bars_visible(&mut self) -> bool {
        self.panes
            .values_mut()
            .any(ContributedSurfacePane::bars_visible)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (PaneId, &ContributedSurfacePane)> {
        self.panes.iter().map(|(pane, session)| (*pane, session))
    }
}

#[cfg(test)]
#[path = "contributed_surface/tests.rs"]
mod tests;
