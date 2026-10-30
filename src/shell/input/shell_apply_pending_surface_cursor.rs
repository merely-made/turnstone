// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn apply_pending_surface_cursor(&mut self) {
        let shape = self.hovered_surface.and_then(|node| {
            if self
                .content_sessions
                .get(&node)
                .is_some_and(|content| content.cue_surface().is_some())
            {
                self.content_sessions.cue_cursors.remove(&node)
            } else {
                self.content_sessions
                    .surface_mut(&node)
                    .and_then(|producer| producer.poll_cursor_shape())
            }
        });
        if let (Some(shape), Some(window)) = (shape, self.window.as_ref()) {
            let icon = surface_cursor_icon(shape);
            window.set_cursor_visible(icon.is_some());
            if let Some(icon) = icon {
                window.set_cursor(icon);
            }
            if let Some(node) = self.hovered_surface {
                self.app
                    .note(crate::observe::AppEvent::SurfaceCursorChanged {
                        node,
                        shape: surface_cursor_label(shape),
                    });
                self.run_effects(Vec::new());
            }
        }
    }

    pub(in super::super) fn reset_surface_cursor(&mut self) {
        let preview_changed = self.app.set_link_preview(None);
        if self.hovered_surface.take().is_none() {
            if preview_changed {
                self.request_redraw();
            }
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_cursor_visible(true);
            window.set_cursor(CursorIcon::Default);
        }
        if preview_changed {
            self.request_redraw();
        }
    }

    pub(in super::super) fn deliver_release(&mut self, x: f32, y: f32, button: MouseButton) {
        self.surface_pointer_buttons.0 &= !surface_button_mask(button).0;
        let std::ops::ControlFlow::Continue(captured) =
            dispatch_primary_release(self, x, y, button)
        else {
            return;
        };
        if captured.is_none()
            && button == MouseButton::Left
            && self.outer_release(0, (self.width.max(1), self.height.max(1)), x, y)
        {
            self.request_redraw();
            return;
        }
        let graph_capture = match captured {
            Some(crate::surface::SurfaceKind::Graph(pane)) => Some(pane),
            _ => None,
        };
        if let Some(crate::surface::SurfaceKind::Pane(pane)) = captured
            && let Some((local_x, local_y)) = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Pane(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            })
            && let Some(surface) = self.renderers.contributed.get_mut(pane)
        {
            if button == MouseButton::Left {
                if surface.mouse_release(local_x, local_y)
                    == crate::contributed_surface::SurfaceRequest::Redraw
                {
                    self.request_redraw();
                }
            }
            return;
        }
        if self.wb_divider_drag.take().is_some() {
            // Like the frisket seam: moves rode Redraw; persist on release.
            self.act(Action::SaveSession);
            return;
        }
        if let Some((pane, dragged)) = self.wb_tab_drag.take() {
            self.finish_wb_tab_gesture(pane, dragged, x, y);
            return;
        }
        if self.divider_drag.take().is_some() {
            // The drag's ratio moves rode Redraw only; the settled layout
            // persists once, on release.
            self.act(Action::SaveSession);
            return;
        }
        if let Some(pane) = graph_capture
            && let Some(button) = pointer_button(button)
            && let Some((local_x, local_y)) = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Graph(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            })
        {
            if self.release_graph_pointer(pane, button, local_x, local_y) {
                self.request_redraw();
            }
            // A right click the canvas did not use is a request for the
            // context menu: select the node under it, so node-scoped commands
            // apply to it, then open the command lane.
            if let Some(request) = self.app.graph_pane_take_context_request(pane) {
                if let Some(member) = request.node {
                    self.app.graph_pane_select_member(pane, member);
                }
                self.act(Action::OmnibarOpen { command: true });
            }
        }
    }

    /// Native badge expansion is a view edit even when it bypasses Action.
    /// Both primary and lens input use this save boundary after release.
    pub(in super::super) fn release_graph_pointer(
        &mut self,
        pane: crate::panes::PaneId,
        button: mere::canvas::PointerButton,
        x: f32,
        y: f32,
    ) -> bool {
        let graph = self.app.graph_for_pane(pane);
        let before = graph
            .and_then(|g| self.app.graph_runtimes.canvas(g))
            .map(|c| c.fold_records().to_vec());
        let mosaic_before = graph
            .and_then(|g| self.app.graph_runtimes.canvas(g))
            .and_then(|c| c.cartography_geometry().mosaic().cloned());
        if self.ctrl || self.shift || self.alt || self.meta {
            self.app
                .reading
                .scope
                .activation
                .entry(pane)
                .or_default()
                .cancel();
        }
        let handled = self.app.graph_pane_pointer_up(pane, button, x, y);
        let mosaic_after = graph
            .and_then(|g| self.app.graph_runtimes.canvas(g))
            .and_then(|c| c.cartography_geometry().mosaic().cloned());
        if mosaic_before != mosaic_after {
            self.app.mark_graph_scene_changed(pane);
        }
        if let Some(graph) = graph {
            let after = self
                .app
                .graph_runtimes
                .canvas(graph)
                .map(|c| c.fold_records().to_vec());
            if before != after {
                self.save_graph_folds(graph);
            }
        }
        if let Some((graph, instance, member)) = self.app.reading.scope.read_request.take()
            && self
                .app
                .graph_runtimes
                .get(graph)
                .is_some_and(|runtime| runtime.instance == instance)
        {
            self.app.active_pane = Some(pane);
            let effects = self.app.read_member_in_graph(graph, member);
            self.run_effects(effects);
        }
        handled
    }

    /// Lower one winit touch contact to Pointer Events. Each live contact keeps
    /// the content node hit at Started, matching implicit pointer capture for
    /// direct-manipulation devices even if later coordinates leave the tile.
    pub(in super::super) fn deliver_touch(&mut self, touch: Touch) {
        self.deliver_touch_in(0, touch);
    }

    pub(in super::super) fn deliver_touch_in(&mut self, window: u64, touch: Touch) {
        let phase = match touch.phase {
            TouchPhase::Started => inker::PointerPhase::Down,
            TouchPhase::Moved => inker::PointerPhase::Move,
            TouchPhase::Ended => inker::PointerPhase::Up,
            TouchPhase::Cancelled => inker::PointerPhase::Cancel,
        };
        let altitude_angle = match touch.force {
            Some(Force::Calibrated { altitude_angle, .. }) => {
                altitude_angle.map(|angle| angle as f32)
            },
            _ => None,
        };
        let pressure = touch.force.and_then(|force| {
            let value = force.normalized() as f32;
            value.is_finite().then(|| value.clamp(0.0, 1.0))
        });
        self.deliver_touch_contact_in(
            window,
            touch.id,
            phase,
            touch.location.x as f32,
            touch.location.y as f32,
            pressure,
            altitude_angle,
        );
    }

    /// The shared touch lowering seam. Winit and the headed scenario both use
    /// this, so the receipt exercises the same id allocation and per-contact
    /// capture that real hardware does after winit has decoded the OS event.
    pub(in super::super) fn deliver_touch_contact(
        &mut self,
        host_id: u64,
        phase: inker::PointerPhase,
        x: f32,
        y: f32,
        pressure: Option<f32>,
        altitude_angle: Option<f32>,
    ) {
        self.deliver_touch_contact_in(0, host_id, phase, x, y, pressure, altitude_angle);
    }

    pub(in super::super) fn deliver_touch_contact_in(
        &mut self,
        window: u64,
        host_id: u64,
        phase: inker::PointerPhase,
        x: f32,
        y: f32,
        pressure: Option<f32>,
        altitude_angle: Option<f32>,
    ) {
        if self.touch_input.reject_modal(
            &mut self.content_sessions.pool,
            window,
            self.app.user_agent_decision.is_open(),
        ) {
            return;
        }
        self.sync_content_placements();
        let plan = self.input_plan(window);
        if self.touch_input.deliver(
            &mut self.content_sessions.pool,
            &plan,
            self.app.focus,
            window,
            host_id,
            phase,
            inker::PhysicalPosition { x, y },
            pressure,
            altitude_angle,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        ) {
            self.request_redraw();
        }
    }
}
