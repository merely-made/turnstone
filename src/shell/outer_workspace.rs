// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{NodeSessions, Shell};
use crate::{
    action::{Action, SpaceRef},
    outer_frame::HeaderHit,
    panes::{FrisketLayout, PaneId},
    surface::Rect,
};
use layout_dom_api::LayoutDomMut;
use workbench::{FloatEvent, RelativeRect, TileEvent, TileId, TilePath, WorkspaceEvent};

pub(super) enum OuterCapture {
    Float {
        slot: usize,
        id: TileId,
        start: (f32, f32),
        rect: RelativeRect,
        size: (u32, u32),
        session: uuid::Uuid,
        appearance: crate::shell_services::AppearanceConfig,
    },
    Tab {
        slot: usize,
        id: TileId,
        start: (f32, f32),
        session: uuid::Uuid,
        appearance: crate::shell_services::AppearanceConfig,
    },
    Divider {
        slot: usize,
        path: TilePath,
        index: usize,
        area: Rect,
        axis: workbench::SplitAxis,
        shares: Vec<f32>,
        gaps: Vec<f32>,
        session: uuid::Uuid,
        appearance: crate::shell_services::AppearanceConfig,
    },
}
fn space(slot: usize) -> SpaceRef {
    if slot == 0 {
        SpaceRef::Primary
    } else {
        SpaceRef::Lens(slot - 1)
    }
}

/// Withdraw only the physical window that owned this header gesture.
pub(super) fn retire_outer_capture(capture: &mut Option<OuterCapture>, window: u64) -> bool {
    let owned = capture.as_ref().is_some_and(|capture| match capture {
        OuterCapture::Float { slot, .. }
        | OuterCapture::Tab { slot, .. }
        | OuterCapture::Divider { slot, .. } => *slot as u64 == window,
    });
    if owned {
        *capture = None;
    }
    owned
}

/// Portable owner validation uses the same retained frame aperture calculation
/// as the live Shell, without booting a native host or another render core.
pub(super) fn frame_body_geometry(
    app: &crate::app::App,
    space: SpaceRef,
    size: (u32, u32),
) -> Option<(Vec<(PaneId, Rect)>, Vec<(PaneId, Rect, Rect)>)> {
    if app.blueprint_space(space).is_some() {
        return None;
    }
    let layout = app.space(space)?;
    let mut frame = crate::outer_frame::OuterFrame::default();
    let docked = frame
        .prepare(
            layout,
            app.active_pane,
            if space == SpaceRef::Primary {
                app.maximized
            } else {
                None
            },
            size,
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap_or_default();
    let floats = frame.floats();
    Some((docked, floats))
}

/// Compatibility projection for callers that need only docked pane bodies.
pub(super) fn frame_body_rects(
    app: &crate::app::App,
    space: SpaceRef,
    size: (u32, u32),
) -> Option<Vec<(PaneId, Rect)>> {
    frame_body_geometry(app, space, size).map(|(docked, _)| docked)
}

fn pane_selector_family(content: Option<&crate::panes::PaneContent>) -> &'static str {
    use crate::panes::PaneContent;
    match content {
        Some(PaneContent::Roster) => "roster",
        Some(PaneContent::Trail) => "trail",
        Some(PaneContent::Workbench) => "workbench",
        Some(PaneContent::Inspector) => "inspector",
        Some(PaneContent::Gloss(_)) => "gloss",
        Some(PaneContent::Overmap(_)) => "overmap",
        Some(PaneContent::Registered(kind)) => match kind.as_str() {
            crate::panes::kind::TRANSCRIPT => "transcript",
            crate::panes::kind::SETTINGS => "settings",
            crate::panes::kind::ARRANGE => "arrange",
            crate::panes::kind::PUBLISHING => "publishing",
            crate::panes::kind::SHARED_KNOT => "shared-knot",
            crate::panes::kind::DEVICE_RECEIPTS => "device-receipts",
            crate::panes::kind::FROZEN_PROJECTION => "frozen-projection",
            _ => "contributed",
        },
        _ => "pane",
    }
}

fn ordinary_pane_selector(
    renderers: &super::renderers::PaneRenderers,
    pane: PaneId,
    family: &str,
    selector: &taproot::Selector,
) -> Result<Option<(f32, f32)>, &'static str> {
    match family {
        "workbench" => renderers
            .workbench
            .get(&pane)
            .map(|view| view.selector_point(selector))
            .unwrap_or(Ok(None)),
        "settings" => renderers
            .settings
            .get(&pane)
            .map(|view| view.selector_point(selector))
            .unwrap_or(Ok(None)),
        "inspector" => renderers
            .inspector
            .get(&pane)
            .map(|view| view.selector_point(selector))
            .unwrap_or(Ok(None)),
        _ => renderers.inline_selector(pane, selector),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unscoped_selector_uses_a_standalone_panes_retained_route() {
        let pane = PaneId(7);
        let mut roster = crate::cambium_pane::RosterGrid::new();
        roster.sync(&crate::app::App::test_stub(), pane, 512.0, 600.0);
        roster.scene(512, 600);
        assert!(roster.inline_mount().is_none(), "fixture is not mounted inline");
        let mut renderers = super::super::renderers::PaneRenderers::default();
        renderers.roster.insert(pane, roster);
        let selector = taproot::Selector::class("tab").containing("Nodes");
        let family = pane_selector_family(Some(&crate::panes::PaneContent::Roster));
        assert!(selector.matches_surface(family));
        assert!(
            ordinary_pane_selector(&renderers, pane, family, &selector)
                .expect("current retained geometry")
                .is_some(),
            "the Shell's unscoped route must see standalone pane paint"
        );
        assert_eq!(
            pane_selector_family(Some(&crate::panes::PaneContent::Workbench)),
            "workbench"
        );
        assert_eq!(
            pane_selector_family(Some(&crate::panes::PaneContent::Inspector)),
            "inspector"
        );
    }

    #[test]
    fn outer_capture_retirement_is_window_scoped_and_terminal() {
        let mut tab = Some(OuterCapture::Tab {
            slot: 2,
            id: TileId(7),
            start: (20.0, 30.0),
            session: uuid::Uuid::nil(),
            appearance: Default::default(),
        });
        assert!(!retire_outer_capture(&mut tab, 1));
        assert!(tab.is_some(), "another window retains its gesture");
        assert!(retire_outer_capture(&mut tab, 2));
        assert!(
            tab.take().is_none(),
            "a late Up cannot revive withdrawn custody"
        );
        assert!(!retire_outer_capture(&mut tab, 2));

        let mut divider = Some(OuterCapture::Divider {
            slot: 0,
            path: TilePath(Vec::new()),
            index: 0,
            area: Rect::new(0.0, 0.0, 200.0, 100.0),
            axis: workbench::SplitAxis::Row,
            shares: vec![0.5, 0.5],
            gaps: vec![10.0],
            session: uuid::Uuid::nil(),
            appearance: Default::default(),
        });
        assert!(!retire_outer_capture(&mut divider, 2));
        assert!(retire_outer_capture(&mut divider, 0));
        assert!(divider.is_none());

        let mut floating = Some(OuterCapture::Float {
            slot: 3,
            id: TileId(9),
            start: (20.0, 30.0),
            rect: RelativeRect {
                x: 0.1,
                y: 0.1,
                width: 0.5,
                height: 0.5,
            },
            size: (400, 300),
            session: uuid::Uuid::nil(),
            appearance: Default::default(),
        });
        assert!(!retire_outer_capture(&mut floating, 2));
        assert!(
            matches!(
                floating.as_ref(),
                Some(OuterCapture::Float {
                    slot: 3,
                    id: TileId(9),
                    ..
                })
            ),
            "another window retains its floating gesture"
        );
        assert!(retire_outer_capture(&mut floating, 3));
        assert!(
            floating.take().is_none(),
            "a late Up cannot revive withdrawn floating custody"
        );
        assert!(!retire_outer_capture(&mut floating, 3));
    }
}
impl Shell {
    pub(super) fn retained_pane_selector_target(
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
                        || (name == "workbench"
                            && self.renderers.workbench.get(&pane).is_none_or(|view| {
                                view.tab_at(
                                    local.0,
                                    local.1,
                                    surface.rect.w as u32,
                                    surface.rect.h as u32,
                                )
                                .is_none()
                            }))
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
    pub(super) fn outer_rects(
        &self,
        slot: usize,
        layout: &FrisketLayout,
        maximize: Option<PaneId>,
        size: (u32, u32),
    ) -> Vec<(PaneId, Rect)> {
        self.outer_frames
            .borrow_mut()
            .entry(slot)
            .or_insert_with(|| crate::outer_frame::OuterFrame::new(self.renderers.dom.clone()))
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
    pub(super) fn outer_scene(&self, slot: usize) -> netrender::Scene {
        self.outer_frames
            .borrow_mut()
            .entry(slot)
            .or_insert_with(|| crate::outer_frame::OuterFrame::new(self.renderers.dom.clone()))
            .scene()
    }
    pub(super) fn outer_float_rects(&self, slot: usize) -> Vec<(PaneId, Rect, Rect)> {
        self.outer_frames
            .borrow()
            .get(&slot)
            .map(|f| f.floats())
            .unwrap_or_default()
    }
    pub(super) fn prepare_outer(&self, slot: usize, size: (u32, u32)) -> bool {
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
    pub(super) fn prepare_inline_owners(&mut self, slot: usize, size: (u32, u32)) {
        if !self.prepare_outer(slot, size) {
            // Projection withdrawal detaches only bodies still parented here.
            // Pane runners and owners survive; a moved destination keeps custody.
            self.retire_frame_focus(slot as u64);
            retire_outer_capture(&mut self.outer_capture, slot as u64);
            self.outer_frames.borrow_mut().remove(&slot);
            if let Some(host) = &self.host {
                super::native_composition::retire(
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
    pub(super) fn finish_inline_frame(&mut self) {
        for frame in self.outer_frames.borrow_mut().values_mut() {
            frame.controls.observe_body_mutations();
        }
        let roots = self.renderers.owner_roots();
        let mut dom = self.renderers.dom.borrow_mut();
        let mut mutations = Vec::new();
        dom.drain_mutations(&mut mutations);
        dom.collect(roots);
    }
    pub(super) fn outer_press(&mut self, slot: usize, size: (u32, u32), x: f32, y: f32) -> bool {
        // Headers never steal an already admitted pane, document or graph gesture.
        if self.contributed_pointer.is_some()
            || self.pointer_capture.is_some()
            || super::window_input::captured_mouse(&self.content_sessions.pool).is_some()
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
    pub(super) fn outer_move(&mut self, slot: usize, x: f32, y: f32) -> bool {
        if self
            .outer_capture
            .as_ref()
            .is_some_and(|capture| match capture {
                OuterCapture::Float {
                    session,
                    appearance,
                    ..
                }
                | OuterCapture::Tab {
                    session,
                    appearance,
                    ..
                }
                | OuterCapture::Divider {
                    session,
                    appearance,
                    ..
                } => {
                    *session != *self.app.session_id.as_uuid()
                        || appearance != &self.app.shell_chrome_config().appearance
                },
            })
        {
            self.outer_capture = None;
            return true;
        }
        let Some(capture) = &self.outer_capture else {
            return false;
        };
        let event = match capture {
            OuterCapture::Float {
                slot: owner,
                id,
                start,
                rect,
                size,
                ..
            } => {
                if *owner != slot {
                    return false;
                }
                let current = if slot == 0 {
                    (self.width, self.height)
                } else {
                    self.lens_windows
                        .values()
                        .find(|w| w.ordinal + 1 == slot)
                        .map(|w| (w.width, w.height))
                        .unwrap_or_default()
                };
                if current != *size {
                    self.outer_capture = None;
                    return true;
                }
                if (x - start.0).hypot(y - start.1) < 5.0 {
                    return true;
                }
                let mut next = *rect;
                next.x = (rect.x + (x - start.0) / size.0.max(1) as f32)
                    .clamp(0.0, (1.0 - rect.width).max(0.0));
                next.y = (rect.y + (y - start.1) / size.1.max(1) as f32)
                    .clamp(0.0, (1.0 - rect.height).max(0.0));
                WorkspaceEvent::Float(FloatEvent::SetRect {
                    tile: *id,
                    rect: next,
                })
            },
            OuterCapture::Tab { slot: owner, .. } => return *owner == slot,
            OuterCapture::Divider {
                slot: owner,
                path,
                index,
                area,
                axis,
                shares,
                gaps,
                ..
            } => {
                if *owner != slot || *index + 1 >= shares.len() {
                    return false;
                }
                let row = *axis == workbench::SplitAxis::Row;
                let available = (if row { area.w } else { area.h }) - gaps.iter().sum::<f32>();
                if available <= 0.0 {
                    return true;
                }
                let total: f32 = shares.iter().sum();
                let before: f32 = shares[..*index].iter().sum();
                let pair = shares[*index] + shares[*index + 1];
                let point = if row { x - area.x } else { y - area.y };
                let left = ((point - gaps[..*index].iter().sum::<f32>()) / available * total
                    - before)
                    .clamp(pair * 0.05, pair * 0.95);
                let mut next = shares.clone();
                next[*index] = left;
                next[*index + 1] = pair - left;
                TileEvent::DividerMoved {
                    split: path.clone(),
                    fractions: next,
                }
                .into()
            },
        };
        self.act(Action::OuterWorkspace {
            space: space(slot),
            event,
        });
        true
    }
    pub(super) fn outer_release(&mut self, slot: usize, size: (u32, u32), x: f32, y: f32) -> bool {
        if self
            .outer_capture
            .as_ref()
            .is_some_and(|capture| match capture {
                OuterCapture::Float {
                    session,
                    appearance,
                    ..
                }
                | OuterCapture::Tab {
                    session,
                    appearance,
                    ..
                }
                | OuterCapture::Divider {
                    session,
                    appearance,
                    ..
                } => {
                    *session != *self.app.session_id.as_uuid()
                        || appearance != &self.app.shell_chrome_config().appearance
                },
            })
        {
            self.outer_capture = None;
            return true;
        }
        let Some(capture) = self.outer_capture.take() else {
            return false;
        };
        match capture {
            capture @ OuterCapture::Float { slot: owner, .. } if owner != slot => {
                self.outer_capture = Some(capture);
                return false;
            },
            OuterCapture::Float {
                id,
                start,
                size: captured,
                ..
            } => {
                if captured != size {
                    return true;
                }
                if (x - start.0).hypot(y - start.1) >= 5.0 && self.prepare_outer(slot, size) {
                    if x < 0.0 || y < 0.0 || x >= size.0 as f32 || y >= size.1 as f32 {
                        self.act(Action::OuterWorkspace {
                            space: space(slot),
                            event: TileEvent::Dragged {
                                tile: id,
                                to: workbench::DropTarget::Outside,
                            }
                            .into(),
                        });
                    } else if self.ctrl {
                        let target = self
                            .outer_frames
                            .borrow()
                            .get(&slot)
                            .and_then(|f| f.float_dock_target(x, y));
                        if let Some(target) = target {
                            self.act(Action::OuterWorkspace {
                                space: space(slot),
                                event: FloatEvent::Dock { tile: id, target }.into(),
                            });
                        }
                    }
                    self.act(Action::SaveSession);
                }
            },
            OuterCapture::Tab {
                slot: owner,
                id,
                start,
                session,
                appearance,
            } => {
                if owner != slot {
                    self.outer_capture = Some(OuterCapture::Tab {
                        slot: owner,
                        id,
                        start,
                        session,
                        appearance,
                    });
                    return false;
                }
                if (x - start.0).hypot(y - start.1) >= 5.0 && self.prepare_outer(slot, size) {
                    let target = self
                        .outer_frames
                        .borrow_mut()
                        .get_mut(&slot)
                        .and_then(|frame| frame.drop_target(x, y));
                    if let Some(to) = target {
                        self.act(Action::OuterWorkspace {
                            space: space(slot),
                            event: TileEvent::Dragged { tile: id, to }.into(),
                        });
                    }
                }
            },
            capture @ OuterCapture::Divider { slot: owner, .. } => {
                if owner != slot {
                    self.outer_capture = Some(capture);
                    return false;
                }
                self.act(Action::SaveSession);
            },
        }
        true
    }
}
