// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ContributedSurfacePane {
    pub fn new(
        pane_kind: PaneKindId,
        source: PaneSource,
        session: Box<dyn RetainedSurfaceSession>,
        stylesheet: impl Into<String>,
    ) -> Self {
        let dom = session.dom();
        Self {
            generation: 0,
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
            mouse_gesture: None,
            click_slop: crate::shell::window_input::click_slop(
                std::env::var("TURNSTONE_CLICK_SLOP_PX").ok().as_deref(),
            ),
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn document_projection(
        &self,
    ) -> Option<document_session_api::DocumentA11yProjection> {
        self.layout
            .document_projection(&self.dom.borrow(), self.session.focus())
    }

    pub(crate) fn painted_rect(&self, node: NodeId) -> Option<(f32, f32, f32, f32)> {
        self.layout.painted_rect(&self.dom.borrow(), node)
    }

    pub(crate) fn visible_rect(&self, node: NodeId) -> Option<(f32, f32, f32, f32)> {
        self.layout.visible_rect(&self.dom.borrow(), node)
    }

    pub(crate) fn reveal(&mut self, node: NodeId) -> bool {
        self.layout
            .reveal(&self.dom.borrow(), node, &mut self.scroll)
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

    pub(crate) fn accepts_text(&self) -> bool {
        self.layout
            .editable_focus(&self.dom.borrow(), self.session.focus())
    }

    pub(crate) fn ime_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        self.layout
            .cursor_area(&self.dom.borrow(), self.session.focus())
    }

    /// Render using the same retained layout that subsequent hit testing uses.
    /// `width` and `height` are the pane's physical size: the provider's DOM is
    /// laid out at `size / scale_factor`, the scene is rasterized at
    /// `scale_factor`, and the pointer coordinates the provider receives are in
    /// the same layout pixels.
    pub fn scene(&mut self, width: u32, height: u32, scale_factor: f32) -> netrender::Scene {
        let scale_factor = crate::ui::clean_scale(scale_factor);
        self.viewport = (width, height, scale_factor);
        self.layout.set_scale(scale_factor);
        let effects = self.session.sync_viewport(SurfaceViewport {
            width: width as f32 / scale_factor,
            height: height as f32 / scale_factor,
            scale_factor,
        });
        self.apply(effects);
        let dom = self.session.dom();
        let mut dom = dom.borrow_mut();
        self.layout
            .scene_scrolled(&mut dom, &self.stylesheet, width, height, &mut self.scroll)
    }

    /// A point and the viewport size, in the layout pixels providers see.
    pub(super) fn logical(&self, x: f32, y: f32) -> ((f32, f32), (f32, f32)) {
        let (width, height, scale) = self.viewport;
        (
            (x / scale, y / scale),
            (width as f32 / scale, height as f32 / scale),
        )
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
            event: PointerClick::at(self.logical(x, y).0),
        })
    }

    /// Ordinary host mouse gesture entry points. Keep this separate from
    /// semantic Click, which remains available for accessibility and commands.
    pub(crate) fn mouse_press(&mut self, x: f32, y: f32) -> SurfaceRequest {
        if self.mouse_gesture.is_some() {
            self.mouse_cancel(x, y);
        }
        self.mouse_gesture = self.hit_test(x, y).map(|target| (target, (x, y), false));
        self.pointer_down(x, y)
    }

    pub(crate) fn mouse_move(&mut self, x: f32, y: f32) -> SurfaceRequest {
        if let Some((_, origin, moved)) = &mut self.mouse_gesture {
            *moved |= (origin.0 - x).hypot(origin.1 - y) > self.click_slop;
        }
        self.pointer_move(x, y)
    }

    pub(crate) fn mouse_release(&mut self, x: f32, y: f32) -> SurfaceRequest {
        let gesture = self.mouse_gesture.take();
        let pointer = self.pointer_up(x, y);
        let click = match gesture {
            Some((target, _, false)) if self.hit_test(x, y) == Some(target) => self.click(x, y),
            _ => SurfaceRequest::None,
        };
        if pointer == SurfaceRequest::Redraw || click == SurfaceRequest::Redraw {
            SurfaceRequest::Redraw
        } else {
            SurfaceRequest::None
        }
    }

    pub(crate) fn mouse_cancel(&mut self, x: f32, y: f32) -> SurfaceRequest {
        self.mouse_gesture = None;
        self.pointer_cancel(x, y)
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

    pub fn pointer_cancel(&mut self, x: f32, y: f32) -> SurfaceRequest {
        self.pointer(PointerPhase::Cancel, x, y, false)
    }

    pub(super) fn pointer(
        &mut self,
        phase: PointerPhase,
        x: f32,
        y: f32,
        resolve_target: bool,
    ) -> SurfaceRequest {
        let (local, size) = self.logical(x, y);
        let event = PointerEvent::new(phase, local, size);
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
                // This terminal envelope preserves the event's Cancel phase.
                // The retained runner clears custody by pointer identity and
                // delivers Cancel, without synthesizing an Up or activation.
                PointerPhase::Up | PointerPhase::Cancel => ResolvedSurfaceEvent::PointerUp(event),
                PointerPhase::Down => unreachable!("down is resolved above"),
            })
        }
    }

    pub fn hover(&mut self, phase: HoverPhase, x: f32, y: f32) -> SurfaceRequest {
        let (local, size) = self.logical(x, y);
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
            event: HoverEvent::new(phase, local, size),
        })
    }

    /// Wheel policy is product-first: a product wheel target receives the
    /// event, and only an unhandled hit moves Turnstone's viewport scroll.
    pub fn wheel(&mut self, dx: f32, dy: f32, x: f32, y: f32) -> SurfaceRequest {
        let (local, size) = self.logical(x, y);
        let scale = self.viewport.2;
        let target = self
            .hit_test(x, y)
            .and_then(|hit| self.session.wheel_target(hit));
        if let Some(target) = target {
            return self.dispatch(ResolvedSurfaceEvent::Wheel {
                target,
                event: WheelEvent::new((dx / scale, dy / scale), local, size),
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
    /// `None` refuses admission; `Some(None)` admits without a host effect.
    pub(crate) fn accessibility_action(
        &mut self,
        action: accesskit::Action,
        node: NodeId,
    ) -> Option<SurfaceRequest> {
        let Some(projected) = self.document_projection() else {
            return None;
        };
        let dom = self.dom.borrow();
        let Some(semantic) = projected.nodes().iter().find(|candidate| {
            candidate.id.get() == layout_dom_api::LayoutDom::opaque_id(&*dom, node)
        }) else {
            return None;
        };
        if semantic.state.hidden || semantic.state.disabled {
            return None;
        }
        let allowed = match action {
            accesskit::Action::Click => semantic
                .actions
                .contains(&document_session_api::DocumentA11yAction::Click),
            accesskit::Action::Focus => semantic
                .actions
                .contains(&document_session_api::DocumentA11yAction::Focus),
            _ => false,
        };
        drop(dom);
        if !allowed {
            return None;
        }
        match action {
            accesskit::Action::Click => Some(self.dispatch(ResolvedSurfaceEvent::Click {
                target: node,
                event: PointerClick::at((0.0, 0.0)),
            })),
            accesskit::Action::Focus => Some(self.focus(Some(node))),
            _ => None,
        }
    }

    pub(super) fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> SurfaceRequest {
        let effects = self.session.dispatch(event);
        self.apply(effects)
    }

    pub(super) fn apply(&mut self, effects: Vec<SurfaceEffect>) -> SurfaceRequest {
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
