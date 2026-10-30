// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn retained_pane_selector_target(
        &self,
        selector: &taproot::Selector,
    ) -> taproot::SelectorTarget {
        let plan = self.surface_plan();
        let mut found = None;
        let mut owned_family = false;
        for surface in &plan {
            let crate::surface::SurfaceKind::Pane(pane) = surface.kind else {
                continue;
            };
            let content = self.pane_content(pane);
            let name = pane_selector_family(content.as_ref());
            if !selector.matches_surface(name) {
                continue;
            }
            if name == "contributed" {
                // Provider semantics and painted geometry have their own
                // owner, evaluated with other families below.
                continue;
            }
            owned_family = true;
            let local = ordinary_pane_selector(&self.renderers, pane, name, selector);
            match local {
                Ok(Some(local)) => {
                    let point = (surface.rect.x + local.0, surface.rect.y + local.1);
                    if found.is_some()
                        || crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                            .is_none_or(|hit| hit.id != surface.id)
                    {
                        return taproot::SelectorTarget::Miss;
                    }
                    found = Some(taproot::Hit {
                        surface: name,
                        point,
                    });
                },
                Err(_) => return taproot::SelectorTarget::Miss,
                _ => {},
            }
        }
        if selector.matches_surface("knot") {
            for surface in &plan {
                let crate::surface::SurfaceKind::Content(node) = surface.kind else {
                    continue;
                };
                let Some(knot) = self.content_sessions.session(&node).and_then(|session| {
                    session
                        .as_any_ref()
                        .downcast_ref::<crate::knot_authoring::KnotDocumentSession>()
                }) else {
                    continue;
                };
                owned_family = true;
                match knot.selector_point(selector) {
                    Ok(Some(local)) => {
                        let point = (surface.rect.x + local.0, surface.rect.y + local.1);
                        if found.is_some()
                            || crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                                .is_none_or(|hit| hit.id != surface.id)
                        {
                            return taproot::SelectorTarget::Miss;
                        }
                        found = Some(taproot::Hit {
                            surface: "knot",
                            point,
                        });
                    },
                    Err(_) => return taproot::SelectorTarget::Miss,
                    _ => {},
                }
            }
        }
        if selector.matches_surface("contributed") {
            let readings = self.contributed_readings();
            if !readings.is_empty() {
                owned_family = true;
                match self.contributed_selector(selector) {
                    Ok(target) => {
                        if let Some(point) = target.point() {
                            if found.is_some()
                                || crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                                    .is_none_or(|hit| {
                                        hit.kind != crate::surface::SurfaceKind::Pane(target.pane())
                                    })
                            {
                                return taproot::SelectorTarget::Miss;
                            }
                            found = Some(taproot::Hit {
                                surface: "contributed",
                                point,
                            });
                        }
                    },
                    Err(error) if error.starts_with("multiple") => {
                        return taproot::SelectorTarget::Miss;
                    },
                    Err(_) => {},
                }
            }
        }
        if found.is_none() && selector.surface.is_none() {
            let mut frames = self.outer_frames.borrow_mut();
            if let Some(frame) = frames.get_mut(&0) {
                match frame.selector_point(selector) {
                    Ok(Some(point)) => {
                        return taproot::SelectorTarget::Hit(taproot::Hit {
                            surface: "frame",
                            point,
                        });
                    },
                    Err(_) => return taproot::SelectorTarget::Miss,
                    _ => {},
                }
            }
        }
        found.map_or_else(
            || {
                if owned_family {
                    taproot::SelectorTarget::Miss
                } else {
                    taproot::SelectorTarget::Unsupported
                }
            },
            taproot::SelectorTarget::Hit,
        )
    }

    pub(in super::super) fn outer_rects(
        &self,
        slot: usize,
        layout: &FrisketLayout,
        maximize: Option<PaneId>,
        size: (u32, u32),
    ) -> Vec<(PaneId, Rect)> {
        let mut frames = self.outer_frames.borrow_mut();
        let frame = frames
            .entry(slot)
            .or_insert_with(|| crate::outer_frame::OuterFrame::new(self.renderers.dom.clone()));
        frame.set_main_view_switch(slot == 0);
        frame.set_layout_scale(self.app.shell_chrome_config().layout_scale(slot));
        frame.set_workspace_area(crate::chrome_view::workspace_area_for_window(
            self.app.shell_chrome_config(),
            slot,
            size.0,
            size.1,
        ));
        frame
            .prepare(
                layout,
                self.app.active_pane,
                maximize,
                size,
                &self.app.shell_chrome_config().appearance,
                self.app.session_id.as_uuid().as_u128(),
            )
            .unwrap_or_default()
    }

    pub(in super::super) fn outer_scene(&self, slot: usize) -> netrender::Scene {
        self.outer_frames
            .borrow_mut()
            .entry(slot)
            .or_insert_with(|| crate::outer_frame::OuterFrame::new(self.renderers.dom.clone()))
            .scene()
    }

    pub(in super::super) fn outer_float_rects(&self, slot: usize) -> Vec<(PaneId, Rect, Rect)> {
        self.outer_frames
            .borrow()
            .get(&slot)
            .map(|f| f.floats())
            .unwrap_or_default()
    }

    pub(in super::super) fn prepare_outer(&self, slot: usize, size: (u32, u32)) -> bool {
        if self.app.blueprint_space(space(slot)).is_some() {
            return false;
        }
        let Some(layout) = self.app.space(space(slot)) else {
            return false;
        };
        self.outer_rects(
            slot,
            layout,
            if slot == 0 { self.app.maximized } else { None },
            size,
        );
        true
    }

    /// Sync host-owned components before the shared frame is painted. Pane
    /// surfaces remain in the input plan, but inline owners are never rasterized
    /// as an additional texture.
    pub(in super::super) fn prepare_inline_owners(&mut self, slot: usize, size: (u32, u32)) {
        if !self.prepare_outer(slot, size) {
            // Projection withdrawal detaches only bodies still parented here.
            // Pane runners and owners survive; a moved destination keeps custody.
            self.retire_frame_focus(slot as u64);
            retire_outer_capture(&mut self.outer_capture, slot as u64);
            self.outer_frames.borrow_mut().remove(&slot);
            if let Some(host) = &self.host {
                super::super::native_composition::retire(
                    host.core(),
                    &mut self.renderers.native_images,
                    &mut self.renderers.native_masks,
                    slot,
                );
                host.core()
                    .invalidate_surface(crate::surface::SurfaceId::frame_window(slot).0);
            }
            let panes: Vec<_> = self
                .app
                .space(space(slot))
                .map(|layout| layout.iter_leaves().map(|(pane, _, _)| pane).collect())
                .unwrap_or_default();
            for pane in panes {
                self.renderers.unbind_inline(pane);
            }
            return;
        }
        let plan = if slot == 0 {
            self.surface_plan()
        } else {
            self.lens_plan(slot - 1, size.0, size.1)
        };
        let layout = self
            .outer_frames
            .borrow()
            .get(&slot)
            .unwrap()
            .layout
            .clone();
        for surface in &plan {
            let crate::surface::SurfaceKind::Pane(pane) = surface.kind else {
                continue;
            };
            let content = if slot == 0 {
                self.pane_content(pane)
            } else {
                self.lens_pane_content(slot - 1, pane)
            };
            if self.renderers.inline_mount(pane).is_some() {
                self.renderers.bind_inline(pane, &layout);
            }
            self.pane_scene_by_kind(
                pane,
                content.as_ref(),
                surface.rect.w.round().max(1.0) as u32,
                surface.rect.h.round().max(1.0) as u32,
            );
            self.renderers.bind_inline(pane, &layout);
        }
        let mounts = plan
            .iter()
            .filter_map(|surface| {
                let crate::surface::SurfaceKind::Pane(pane) = surface.kind else {
                    return None;
                };
                Some((TileId(pane.0), self.renderers.inline_mount(pane)?))
            })
            .collect();
        self.outer_frames
            .borrow_mut()
            .get_mut(&slot)
            .unwrap()
            .controls
            .set_mounts(mounts);
        self.prepare_outer(slot, size);
        // Paint state, scroll clamps and custom leaves use measured shared
        // sizes. The first pass already synced app data and owner view state.
        for surface in &plan {
            let crate::surface::SurfaceKind::Pane(pane) = surface.kind else {
                continue;
            };
            self.renderers.refresh_inline_paint(
                pane,
                surface.rect.w.round().max(1.0) as u32,
                surface.rect.h.round().max(1.0) as u32,
                self.app.pane_layout_scale(pane),
            );
        }
        self.prepare_outer(slot, size);
        let focused = match self.app.focus {
            crate::surface::FocusTarget::Pane(pane)
                if plan
                    .iter()
                    .any(|s| s.kind == crate::surface::SurfaceKind::Pane(pane)) =>
            {
                self.renderers.inline_focus(pane)
            },
            _ => None,
        };
        self.outer_frames
            .borrow_mut()
            .get_mut(&slot)
            .unwrap()
            .body_focus = focused;
    }

    pub(in super::super) fn finish_inline_frame(&mut self) {
        for frame in self.outer_frames.borrow_mut().values_mut() {
            frame.controls.observe_body_mutations();
        }
        let roots = self.renderers.owner_roots();
        let mut dom = self.renderers.dom.borrow_mut();
        let mut mutations = Vec::new();
        dom.drain_mutations(&mut mutations);
        dom.collect(roots);
    }

    pub(in super::super) fn outer_press(
        &mut self,
        slot: usize,
        size: (u32, u32),
        x: f32,
        y: f32,
    ) -> bool {
        // Headers never steal an already admitted pane, document or graph gesture.
        if self.contributed_pointer.is_some()
            || self.pointer_capture.is_some()
            || super::super::window_input::captured_mouse(&self.content_sessions.pool).is_some()
            || self
                .lens_windows
                .values()
                .any(|lens| lens.pointer_graph.is_some())
        {
            return false;
        }
        if self.outer_capture.is_some() {
            return true;
        }
        if !self.prepare_outer(slot, size) {
            return false;
        }
        let hit = self
            .outer_frames
            .borrow_mut()
            .get_mut(&slot)
            .and_then(|frame| frame.hit(x, y));
        if let Some(hit) = &hit {
            let control=match hit {
                HeaderHit::Tab(id)=>Some(crate::frame_controls::Control::Tab(*id)),
                HeaderHit::Close(id)=>Some(crate::frame_controls::Control::Close(*id)),
                HeaderHit::Divider {target,..}=>self.outer_frames.borrow().get(&slot).and_then(|frame|frame.controls.order().into_iter().find(|c|matches!(c,crate::frame_controls::Control::Divider{path,index,..} if *path==target.path && *index==target.index))),
            };
            if let Some(control) = control {
                self.enter_frame_focus(slot, Some(control));
            }
        } else {
            self.leave_frame_focus();
        }
        match hit {
            Some(HeaderHit::Close(id)) => self.act(Action::OuterWorkspace {
                space: space(slot),
                event: TileEvent::Closed(id).into(),
            }),
            Some(HeaderHit::Tab(id)) => {
                self.act(Action::OuterWorkspace {
                    space: space(slot),
                    event: TileEvent::Activated(id).into(),
                });
                let floating = self
                    .app
                    .space(space(slot))
                    .and_then(|s| s.outer_workspace().ok())
                    .and_then(|frame| {
                        frame
                            .workspace
                            .floating()
                            .iter()
                            .find(|f| f.tile.id == id)
                            .map(|f| f.rect)
                    });
                self.outer_capture = Some(if let Some(rect) = floating {
                    OuterCapture::Float {
                        slot,
                        id,
                        start: (x, y),
                        rect,
                        size,
                        session: *self.app.session_id.as_uuid(),
                        appearance: self.app.shell_chrome_config().appearance.clone(),
                    }
                } else {
                    OuterCapture::Tab {
                        slot,
                        id,
                        start: (x, y),
                        session: *self.app.session_id.as_uuid(),
                        appearance: self.app.shell_chrome_config().appearance.clone(),
                    }
                });
            },
            Some(HeaderHit::Divider {
                target,
                area,
                axis,
                shares,
                gaps,
            }) => {
                self.outer_capture = Some(OuterCapture::Divider {
                    slot,
                    path: target.path,
                    index: target.index,
                    area,
                    axis,
                    shares,
                    gaps,
                    session: *self.app.session_id.as_uuid(),
                    appearance: self.app.shell_chrome_config().appearance.clone(),
                })
            },
            None => return false,
        }
        true
    }
}
