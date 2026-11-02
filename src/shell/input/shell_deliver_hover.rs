// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Route a pointer release to whatever the matching press captured (rung 5
    /// slice B). The canvas gets a release only if its own press began the
    /// gesture, so a content click never ends a canvas drag. Shared by winit
    /// and the scenario runner.

    /// Route a pointer move. Today only the divider drag consumes moves: while
    /// a seam is captured, each move becomes a ratio through cambium's
    /// `Split::ratio_at` over the split's own container rect, lowered as an
    /// ordinary Action — the same spine as everything else.
    /// Route a pointer move into the pane under it (pane pointer-move
    /// routing): the swatch panes get their Enter/Leave hover transitions, so
    /// the hover emphasis the component always supported finally lights up.
    /// A move off a hovering pane delivers its Leave. Ephemeral, so it drives
    /// the panes' semantic methods directly (the gesture law), never an Action.
    pub(in super::super) fn deliver_hover(&mut self, x: f32, y: f32) {
        let plan = self.surface_plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, x, y);
        let pane_hit = match hit.as_ref().map(|h| h.kind) {
            Some(crate::surface::SurfaceKind::Pane(id)) => Some(id),
            _ => None,
        };
        let previous_pane = self.hovered_pane;
        let mut redraw = false;
        // Leaving the previously hovered pane clears its emphasis.
        if let Some(prev) = self.hovered_pane
            && pane_hit != Some(prev)
        {
            if let Some(pane) = self.renderers.contributed.get_mut(prev) {
                redraw |= pane.hover(cambium::HoverPhase::Leave, 0.0, 0.0)
                    == crate::contributed_surface::SurfaceRequest::Redraw;
            } else {
                redraw |= match self.pane_content(prev) {
                    Some(PaneContent::Gloss(_)) => self
                        .renderers
                        .gloss
                        .get_mut(&prev)
                        .is_some_and(|p| p.hover_leave()),
                    Some(PaneContent::Overmap(_)) => self
                        .renderers
                        .overmap
                        .get_mut(&prev)
                        .is_some_and(|p| p.hover_leave()),
                    _ => false,
                };
            }
        }
        self.hovered_pane = pane_hit;
        if let (Some(hit), Some(id)) = (hit, pane_hit) {
            let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                (
                    s.rect.w.round().max(1.0) as u32,
                    s.rect.h.round().max(1.0) as u32,
                )
            });
            if let Some((rw, rh)) = dims {
                if let Some(pane) = self.renderers.contributed.get_mut(id) {
                    redraw |= pane.hover(
                        if previous_pane == Some(id) {
                            cambium::HoverPhase::Move
                        } else {
                            cambium::HoverPhase::Enter
                        },
                        hit.local.0,
                        hit.local.1,
                    ) == crate::contributed_surface::SurfaceRequest::Redraw;
                } else {
                    redraw |= match self.pane_content(id) {
                        Some(PaneContent::Gloss(_)) => self
                            .renderers
                            .gloss
                            .get_mut(&id)
                            .is_some_and(|p| p.hover(hit.local.0, hit.local.1, rw, rh)),
                        Some(PaneContent::Overmap(_)) => self
                            .renderers
                            .overmap
                            .get_mut(&id)
                            .is_some_and(|p| p.hover(hit.local.0, hit.local.1, rw, rh)),
                        _ => false,
                    };
                }
            }
        }
        if redraw {
            self.request_redraw();
        }
    }

    pub(in super::super) fn deliver_move(&mut self, x: f32, y: f32) {
        if self.move_contributed_pointer(0, x, y) {
            return;
        }
        if let Some(crate::surface::SurfaceKind::Graph(pane)) = self.pointer_capture {
            let local = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Graph(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            });
            if let Some((local_x, local_y)) = local
                && self.app.graph_pane_cursor_moved(pane, local_x, local_y)
            {
                self.request_redraw();
            }
            return;
        }
        if dispatch_primary_content_move(self, x, y) {
            return;
        }
        if self.pointer_capture.is_none() && self.outer_move(0, x, y) {
            self.request_redraw();
            return;
        }
        if let Some(crate::surface::SurfaceKind::Pane(pane)) = self.pointer_capture
            && let Some((local_x, local_y)) = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Pane(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            })
            && let Some(surface) = self.renderers.contributed.get_mut(pane)
        {
            if surface.mouse_move(local_x, local_y)
                == crate::contributed_surface::SurfaceRequest::Redraw
            {
                self.request_redraw();
            }
            return;
        }
        // A workbench divider drag: the band's pair re-weights toward the
        // pointer (host math over platen's N-ary fractions), lowered as an
        // ordinary Action. The walk is pane-local; the origin converts.
        if let Some((div, origin)) = self.wb_divider_drag.clone() {
            let fractions =
                crate::workbench_tiling::drag_fractions(&div, x - origin.0, y - origin.1);
            self.act(Action::WorkbenchSetFractions {
                path: div.path,
                fractions,
            });
            return;
        }
        if let Some(drag) = self.divider_drag.clone() {
            let split = crate::pane::cambium_split(drag.axis, drag.ratio);
            let ratio = split.ratio_at(drag.area.w, drag.area.h, x - drag.area.x, y - drag.area.y);
            self.act(Action::SetSplitRatio {
                space: crate::action::SpaceRef::Primary,
                path: drag.path,
                ratio,
            });
            return;
        }

        let content_hit = crate::surface::hit_test(&self.surface_plan(), self.app.focus, x, y)
            .and_then(|hit| match hit.kind {
                crate::surface::SurfaceKind::Content(node) => {
                    Some((node, hit.id, hit.local.0, hit.local.1))
                },
                _ => None,
            });
        let Some((node, appearance, local_x, local_y)) = content_hit else {
            self.reset_surface_cursor();
            return;
        };
        self.sync_content_placements();
        let _ = self.content_sessions.pool.prepare(&appearance);
        if let Some(links) = self
            .content_sessions
            .appearance(node, appearance)
            .and_then(pelt_core::PeltContent::document)
            .map(|controller| controller.links())
        {
            let target = links.into_iter().find_map(|link| {
                let [left, top, width, height] = link.rect;
                (local_x >= left
                    && local_x <= left + width.max(0.0)
                    && local_y >= top
                    && local_y <= top + height.max(0.0))
                .then_some(link.url)
            });
            let preview =
                target.map(|url| super::super::content_link_target(&self.app, node, &url));
            let hovering_link = preview.is_some();
            let changed = self.app.set_link_preview(preview);
            self.hovered_surface = None;
            if let Some(window) = self.window.as_ref() {
                window.set_cursor_visible(true);
                window.set_cursor(if hovering_link {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
            }
            if changed {
                self.request_redraw();
            }
            return;
        }
        let preview_changed = self.app.set_link_preview(None);
        if !self.content_sessions.has_surface(&node) {
            self.reset_surface_cursor();
            return;
        }
        self.content_mouse(
            appearance,
            (local_x, local_y),
            inker::PointerPhase::Move,
            None,
        );
        self.hovered_surface = Some(node);
        self.apply_pending_surface_cursor();
        if preview_changed {
            self.request_redraw();
        }
    }

    pub(in super::super) fn contributed_pointer_is_current(
        &self,
        admitted: ContributedPointer,
    ) -> bool {
        let crate::surface::SurfaceKind::Pane(pane) = admitted.surface.kind else {
            return false;
        };
        let plan = if admitted.window == 0 {
            self.surface_plan()
        } else if let Some(lens) = self
            .lens_windows
            .values()
            .find(|lens| lens.ordinal as u64 + 1 == admitted.window)
        {
            self.lens_plan(lens.ordinal, lens.width, lens.height)
        } else {
            return false;
        };
        admitted.is_current(
            plan.into_iter()
                .find(|surface| surface.id == admitted.surface.id),
            self.renderers
                .contributed
                .get(pane)
                .map(|surface| surface.generation()),
            admitted.window,
        )
    }

    pub(in super::super) fn move_contributed_pointer(
        &mut self,
        window: u64,
        x: f32,
        y: f32,
    ) -> bool {
        let Some(admitted) = self.contributed_pointer else {
            return false;
        };
        if admitted.window != window {
            return false;
        }
        if !self.contributed_pointer_is_current(admitted) {
            self.cancel_content_pointer();
            return true;
        }
        let crate::surface::SurfaceKind::Pane(pane) = admitted.surface.kind else {
            return false;
        };
        if let Some(surface) = self.renderers.contributed.get_mut(pane) {
            if surface.mouse_move(x - admitted.surface.rect.x, y - admitted.surface.rect.y)
                == crate::contributed_surface::SurfaceRequest::Redraw
            {
                self.request_redraw();
            }
        }
        true
    }

    pub(in super::super) fn release_contributed_pointer(&mut self, window: u64, x: f32, y: f32) {
        let Some(admitted) = self.contributed_pointer else {
            return;
        };
        if admitted.window != window {
            return;
        }
        if !self.contributed_pointer_is_current(admitted) {
            self.cancel_content_pointer();
            return;
        }
        self.contributed_pointer = None;
        self.pointer_capture = None;
        let crate::surface::SurfaceKind::Pane(pane) = admitted.surface.kind else {
            return;
        };
        if let Some(surface) = self.renderers.contributed.get_mut(pane) {
            if surface.mouse_release(x - admitted.surface.rect.x, y - admitted.surface.rect.y)
                == crate::contributed_surface::SurfaceRequest::Redraw
            {
                self.request_redraw();
            }
        }
    }

    /// Focus loss cancels actual retained custody, without manufacturing Up.
    pub(in super::super) fn cancel_content_pointer(&mut self) {
        self.outer_capture = None;
        // Retire host custody first: pool input synchronizes placements and
        // may otherwise recursively discover the same invalid admission.
        let mut windows = vec![0];
        windows.extend(
            self.lens_windows
                .values()
                .map(|lens| lens.ordinal as u64 + 1),
        );
        for window in windows {
            self.touch_input
                .focus_lost(&mut self.content_sessions.pool, window);
        }
        self.window_buttons.clear();
        let captured = self.pointer_capture.take();
        let contributed = self.contributed_pointer.take();
        let pane = contributed
            .map(|admitted| admitted.surface.kind)
            .or(captured);
        if let Some(crate::surface::SurfaceKind::Pane(pane)) = pane
            && let Some(surface) = self.renderers.contributed.get_mut(pane)
        {
            surface.mouse_cancel(0.0, 0.0);
        }
        if let Some(id) = super::super::window_input::captured_mouse(&self.content_sessions.pool) {
            self.content_input(
                id,
                pelt_core::PeltInput::Session(inker::SessionInput::Cancel),
            );
        }
        self.surface_pointer_buttons = inker::PointerButtons::NONE;
        self.wb_divider_drag = None;
        self.wb_tab_drag = None;
        self.lens_divider_drag = None;
    }

    pub(in super::super) fn content_mouse(
        &mut self,
        appearance: crate::surface::SurfaceId,
        local: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.sync_content_placements();
        let Some(placement) = self.content_sessions.pool.placement(&appearance).cloned() else {
            return;
        };
        let window = placement.window;
        let routed = super::super::window_input::content_mouse_input(
            &mut self.content_sessions.pool,
            &self.window_buttons,
            window,
            Some(appearance),
            inker::PhysicalPosition {
                x: local.0 + placement.rect.x * placement.scale_factor,
                y: local.1 + placement.rect.y * placement.scale_factor,
            },
            phase,
            button,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        );
        self.apply_window_mouse_input(window, routed);
    }

    pub(in super::super) fn window_mouse_input(
        &mut self,
        window: u64,
        appearance: Option<crate::surface::SurfaceId>,
        position: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) -> bool {
        let routed = super::super::window_input::native_mouse_input(
            &mut self.content_sessions.pool,
            &mut self.window_buttons,
            window,
            appearance,
            inker::PhysicalPosition {
                x: position.0,
                y: position.1,
            },
            phase,
            button,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        );
        self.apply_window_mouse_input(window, routed)
    }

    pub(super) fn apply_window_mouse_input(
        &mut self,
        window: u64,
        routed: Option<(
            super::super::content_pool::ContentKey,
            pelt_core::PeltHostEffect,
        )>,
    ) -> bool {
        if window == 0 {
            self.surface_pointer_buttons = self
                .window_buttons
                .get(&window)
                .copied()
                .unwrap_or(inker::PointerButtons::NONE);
        }
        let Some((content, effect)) = routed else {
            return false;
        };
        let node = match content {
            super::super::content_pool::ContentKey::Node(node)
            | super::super::content_pool::ContentKey::Reader { node, .. } => node,
        };
        self.apply_content_effect(node, effect);
        true
    }
}
