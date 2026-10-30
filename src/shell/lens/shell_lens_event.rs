// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Route one lens window's event: pane presses dispatch into the SHARED
    /// retained runners (the same round trips the primary uses); canvas
    /// gestures run with the lens's own camera installed around the pass
    /// (pan, zoom, grab); resize, close.
    pub(in super::super) fn lens_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) {
        let Some(lens) = self.lens_windows.get(&id) else {
            return;
        };
        let (window, position) = (lens.ordinal as u64 + 1, lens.cursor);
        let mouse_delivered = if let WindowEvent::MouseInput { state, button, .. } = &event {
            let phase = match state {
                ElementState::Pressed => inker::PointerPhase::Down,
                ElementState::Released => inker::PointerPhase::Up,
            };
            let appearance = if phase == inker::PointerPhase::Up
                && super::super::input::surface_mouse_button(*button)
                    .is_some_and(super::super::window_input::lens_button)
                && self.contributed_pointer.is_none()
            {
                super::super::window_input::captured_mouse(&self.content_sessions.pool).filter(
                    |appearance| {
                        self.content_sessions
                            .pool
                            .placement(appearance)
                            .is_some_and(|p| p.window == window)
                    },
                )
            } else {
                None
            };
            if appearance.is_some() {
                self.sync_content_placements();
            }
            self.window_mouse_input(
                window,
                appearance,
                position,
                phase,
                super::super::input::surface_mouse_button(*button),
            )
        } else {
            false
        };
        let Some(lens) = self.lens_windows.get_mut(&id) else {
            return;
        };
        if matches!(
            event,
            WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            lens.a11y.update_window_bounds(&lens.window);
        }
        if let WindowEvent::Focused(focused) = &event {
            lens.focused = *focused;
            lens.a11y.update_window_focus(*focused);
            let window = lens.ordinal as u64 + 1;
            self.native_input_focus(window, *focused);
            self.push_a11y_tree();
            return;
        }
        match event {
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl = mods.state().control_key();
                self.alt = mods.state().alt_key();
                self.shift = mods.state().shift_key();
                self.meta = mods.state().super_key();
                return;
            },
            WindowEvent::Touch(touch) => {
                let window = lens.ordinal as u64 + 1;
                self.deliver_touch_in(window, touch);
                return;
            },
            WindowEvent::KeyboardInput { event, .. } => {
                let window = lens.ordinal as u64 + 1;
                self.deliver_window_key(
                    window,
                    &event.logical_key,
                    event.state == ElementState::Pressed,
                    event.text.as_deref(),
                );
                return;
            },
            WindowEvent::Ime(ime) => {
                let window = lens.ordinal as u64 + 1;
                super::super::controller_input::trace_native_ime(window, &ime);
                if self.deliver_window_ime(window, &ime) {
                    self.request_redraw();
                }
                return;
            },
            WindowEvent::CloseRequested => {
                let ordinal = lens.ordinal;
                self.bind_lens_capture();
                if self.pending_lens_capture_window == Some(id) {
                    let generation = self
                        .lens_windows
                        .get(&id)
                        .and_then(|lens| lens.host.as_ref())
                        .map_or(0, |host| host.core().generation());
                    self.refuse_window_capture(
                        ordinal as u64 + 1,
                        cambium_rootstock::CaptureRefusal::SurfaceUnavailable {
                            generation,
                            error: "capture target Lens closed".into(),
                        },
                    );
                }
                self.native_input_focus(ordinal as u64 + 1, false);
                self.retire_render_receivers(ordinal as u64 + 1);
                self.window_focus.remove(&(ordinal as u64 + 1));
                self.lens_windows.remove(&id);
                self.outer_frames.borrow_mut().remove(&(ordinal + 1));
                let closed_panes: Vec<_> = self
                    .app
                    .lenses
                    .get(ordinal)
                    .and_then(Option::as_ref)
                    .map(|space| space.iter_leaves().map(|(id, _, _)| id).collect())
                    .unwrap_or_default();
                if let Some(space) = self.app.lenses.get_mut(ordinal) {
                    *space = None;
                }
                if let Some(blueprint) = self.app.lens_blueprints.get_mut(ordinal) {
                    *blueprint = None;
                }
                for pane in closed_panes {
                    self.evict_pane_renderer(pane);
                }
                self.app.window_count = 1 + self.lens_windows.len();
                self.app.note(crate::observe::AppEvent::WindowClosed);
                // Persist the departure: a window closed on purpose stays
                // closed across a restart (its slot saves as null).
                self.act(Action::SaveSession);
                return;
            },
            WindowEvent::Resized(size) => {
                lens.width = size.width.max(1);
                lens.height = size.height.max(1);
                if let Some(host) = lens.host.as_mut() {
                    host.resize(lens.width, lens.height);
                }
                lens.window.request_redraw();
                return;
            },
            WindowEvent::RedrawRequested => {
                tracing::debug!(target: "turnstone::selfdrive", window, "begin lens render");
                let presented = self.render_lens(id);
                tracing::debug!(target: "turnstone::selfdrive", window, presented, "finished lens render");
                self.push_a11y_tree();
                if presented {
                    self.scenario_pump(event_loop);
                }
                return;
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if super::super::input::surface_mouse_button(button)
                .is_some_and(super::super::window_input::lens_button) =>
            {
                // A press routes by the lens's OWN plan: a pane press
                // dispatches into the shared runner; a tile press routes into
                // the shared SESSION (a link is a durable navigation through
                // the spine); a canvas press falls through to the
                // camera-gesture block below.
                let (x, y) = lens.cursor;
                let (lw, lh, ordinal) = (lens.width, lens.height, lens.ordinal);
                if button != MouseButton::Left {
                    self.leave_frame_focus();
                }
                if reject_lens_pane_press(self) {
                    return;
                }
                if button == MouseButton::Left
                    && self.outer_press(ordinal + 1, (lw.max(1), lh.max(1)), x, y)
                {
                    if let Some(lens) = self.lens_windows.get(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
                let plan = self.lens_plan(ordinal, lw, lh);
                // Revalidate while the retained content focus still names its
                // receiver, before a fresh Pane/None branch can change it.
                self.sync_content_placements();
                if let Some(hit) = lens_press_hit(
                    &self.content_sessions.pool,
                    ordinal as u64 + 1,
                    &plan,
                    crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                    inker::PhysicalPosition { x, y },
                ) {
                    match hit.kind {
                        crate::surface::SurfaceKind::Pane(pid) => {
                            // The press anchors pane ops here, exactly as in
                            // the primary — the active pane is GLOBAL (ids are
                            // unique across spaces), so close/divider/summon-
                            // beside now aim at this lens's tree.
                            if button != MouseButton::Left {
                                return;
                            }
                            self.app.active_pane = Some(pid);
                            self.app.raise_floating_pane(pid);
                            if let Some(pane) = self.renderers.contributed.get_mut(pid) {
                                self.app.focus = crate::surface::FocusTarget::Pane(pid);
                                self.pointer_capture = Some(SurfaceKind::Pane(pid));
                                self.contributed_pointer = plan
                                    .iter()
                                    .find(|surface| surface.id == hit.id)
                                    .copied()
                                    .map(|surface| super::super::input::ContributedPointer {
                                        surface,
                                        generation: pane.generation(),
                                        window: ordinal as u64 + 1,
                                    });
                                pane.focus_at(hit.local.0, hit.local.1);
                                pane.mouse_press(hit.local.0, hit.local.1);
                                self.sync_ime_allowed();
                                self.request_redraw();
                                return;
                            }
                            if let Some(content) = self.lens_pane_content(ordinal, pid) {
                                if self.renderers.inline_mount(pid).is_some() {
                                    self.app.focus = crate::surface::FocusTarget::Pane(pid);
                                    self.input_window = ordinal as u64 + 1;
                                }
                                let dims = plan
                                    .iter()
                                    .find(|s| s.id == hit.id)
                                    .map(|s| {
                                        (
                                            s.rect.w.round().max(1.0) as u32,
                                            s.rect.h.round().max(1.0) as u32,
                                        )
                                    })
                                    .unwrap_or((lw, lh));
                                let actions =
                                    self.pane_click_actions(pid, &content, hit.local, dims);
                                for action in actions {
                                    self.act(action);
                                }
                            }
                            self.sync_ime_allowed();
                            self.request_redraw();
                            return;
                        },
                        crate::surface::SurfaceKind::Content(_) => {
                            self.sync_content_placements();
                            let hit = super::super::window_input::content_press_target(
                                &mut self.app.focus,
                                &self.content_sessions.pool,
                                ordinal as u64 + 1,
                                inker::PhysicalPosition { x, y },
                                hit,
                            );
                            self.pointer_capture = Some(hit.kind);
                            self.content_mouse(
                                hit.id,
                                hit.local,
                                inker::PointerPhase::Down,
                                super::super::input::surface_mouse_button(button),
                            );
                            if let Some(lens) = self.lens_windows.get_mut(&id) {
                                lens.window.request_redraw();
                            }
                            return;
                        },
                        // A lens seam drag: capture the band; moves lower
                        // SetSplitRatio at THIS lens's space (same spine as
                        // the primary's seam, different target tree).
                        crate::surface::SurfaceKind::Divider(index)
                            if button == MouseButton::Left =>
                        {
                            if let Some(Some(space)) = self.app.lenses.get(ordinal) {
                                let area = Rect::full(lw, lh);
                                let tiling = crate::pane::place_panes(space, area, None);
                                self.lens_divider_drag = tiling
                                    .dividers
                                    .into_iter()
                                    .find(|d| d.index == index)
                                    .map(|d| (ordinal, d));
                            }
                            return;
                        },
                        _ => {},
                    }
                }
                // Canvas press: handled by the gesture block below.
            },
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                lens.cursor = (x, y);
                let ordinal = lens.ordinal;
                if self.move_contributed_pointer(ordinal as u64 + 1, x, y) {
                    return;
                }
                if let Some(appearance) =
                    super::super::window_input::captured_mouse(&self.content_sessions.pool)
                    && let Some(placement) = self.content_sessions.pool.placement(&appearance)
                    && placement.window == ordinal as u64 + 1
                {
                    let scale = placement.scale_factor;
                    let local = (x - placement.rect.x * scale, y - placement.rect.y * scale);
                    self.content_mouse(appearance, local, inker::PointerPhase::Move, None);
                    return;
                }
                if self.outer_move(ordinal + 1, x, y) {
                    if let Some(lens) = self.lens_windows.get(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
                // A held lens seam: each move becomes a ratio through the
                // same component math as the primary's seam, lowered at the
                // LENS's space. Falls through to the camera block otherwise.
                if let Some((ord, drag)) = self.lens_divider_drag.clone() {
                    let split = crate::pane::cambium_split(drag.axis, drag.ratio);
                    let ratio =
                        split.ratio_at(drag.area.w, drag.area.h, x - drag.area.x, y - drag.area.y);
                    self.act(Action::SetSplitRatio {
                        space: crate::action::SpaceRef::Lens(ord),
                        path: drag.path,
                        ratio,
                    });
                    if let Some(lens) = self.lens_windows.get_mut(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
            },
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } if super::super::input::surface_mouse_button(button)
                .is_some_and(super::super::window_input::lens_button) =>
            {
                let (cursor, ordinal, lw, lh) =
                    (lens.cursor, lens.ordinal, lens.width, lens.height);
                if dispatch_lens_pane_release(self, ordinal as u64 + 1, cursor, button) {
                    return;
                }
                if mouse_delivered {
                    return;
                }
                if button == MouseButton::Left
                    && self.outer_release(ordinal + 1, (lw.max(1), lh.max(1)), cursor.0, cursor.1)
                {
                    if let Some(lens) = self.lens_windows.get(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
                // Like the primary seam: moves rode Redraw; persist once.
                if self.lens_divider_drag.take().is_some() {
                    self.act(Action::SaveSession);
                    return;
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel over a tile scrolls the PAGE (the rung-5 slice-B rule,
                // per window); off-tile falls through to the camera pan below.
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        x * mere::canvas::WHEEL_PAN_SCALE,
                        y * mere::canvas::WHEEL_PAN_SCALE,
                    ),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                let (x, y) = lens.cursor;
                let (lw, lh, ordinal) = (lens.width, lens.height, lens.ordinal);
                let plan = self.lens_plan(ordinal, lw, lh);
                if let Some(hit) = crate::surface::hit_test(&plan, self.app.focus, x, y)
                    && let crate::surface::SurfaceKind::Content(node) = hit.kind
                {
                    self.content_wheel(ordinal as u64 + 1, hit.id, node, hit.local, (dx, dy));
                    if let Some(lens) = self.lens_windows.get(&id) {
                        lens.window.request_redraw();
                    }
                    return;
                }
                if self.lens_pane_wheel(ordinal as u64 + 1, x, y, dx, dy) {
                    return;
                }
            },
            _ => {},
        }
        // Continuous graph gestures route to the captured graph PaneId. The
        // graph runtime supplies truth; `GraphPaneViews` installs and stashes
        // this pane's camera and selection around every call.
        let Some(lens) = self.lens_windows.get(&id) else {
            return;
        };
        let (lw, lh, ordinal, cursor, captured) = (
            lens.width,
            lens.height,
            lens.ordinal,
            lens.cursor,
            lens.pointer_graph,
        );
        let plan = self.lens_plan(ordinal, lw, lh);
        let graph_local = |pane: PaneId, x: f32, y: f32| {
            plan.iter()
                .find(|surface| surface.kind == SurfaceKind::Graph(pane))
                .map(|surface| (x - surface.rect.x, y - surface.rect.y))
        };
        let mut redraw = false;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let pane = captured.or_else(|| {
                    crate::surface::hit_test(
                        &plan,
                        crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                        x,
                        y,
                    )
                    .and_then(|hit| match hit.kind {
                        SurfaceKind::Graph(pane) => Some(pane),
                        _ => None,
                    })
                });
                if let Some((pane, (local_x, local_y))) =
                    pane.and_then(|pane| graph_local(pane, x, y).map(|local| (pane, local)))
                {
                    redraw = self.app.graph_pane_cursor_moved(pane, local_x, local_y);
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        x * mere::canvas::WHEEL_PAN_SCALE,
                        y * mere::canvas::WHEEL_PAN_SCALE,
                    ),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                if let Some(hit) = crate::surface::hit_test(
                    &plan,
                    crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                    cursor.0,
                    cursor.1,
                ) && let SurfaceKind::Graph(pane) = hit.kind
                {
                    redraw = self.app.graph_pane_wheel(pane, dx, dy);
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = pointer_button(button) {
                    let pane = match state {
                        ElementState::Pressed => crate::surface::hit_test(
                            &plan,
                            crate::surface::FocusTarget::Graph(self.app.default_graph_pane()),
                            cursor.0,
                            cursor.1,
                        )
                        .and_then(|hit| match hit.kind {
                            SurfaceKind::Graph(pane) => Some(pane),
                            _ => None,
                        }),
                        ElementState::Released => self
                            .lens_windows
                            .get_mut(&id)
                            .and_then(|lens| lens.pointer_graph.take()),
                    };
                    if let Some((pane, (local_x, local_y))) = pane.and_then(|pane| {
                        graph_local(pane, cursor.0, cursor.1).map(|local| (pane, local))
                    }) {
                        redraw = match state {
                            ElementState::Pressed => {
                                self.raise_shared_float(pane);
                                let handled = self
                                    .app
                                    .graph_pane_pointer_down(pane, button, local_x, local_y);
                                self.app.focus = crate::surface::FocusTarget::Graph(pane);
                                if let Some(lens) = self.lens_windows.get_mut(&id) {
                                    lens.pointer_graph = Some(pane);
                                }
                                handled
                            },
                            ElementState::Released => {
                                self.release_graph_pointer(pane, button, local_x, local_y)
                            },
                        };
                    }
                }
            },
            _ => {},
        }
        if redraw {
            if let Some(lens) = self.lens_windows.get_mut(&id) {
                lens.window.request_redraw();
            }
        }
    }
}
