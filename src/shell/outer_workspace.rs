// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::Shell;
use crate::{
    action::{Action, SpaceRef},
    outer_frame::HeaderHit,
    panes::{FrisketLayout, PaneId},
    surface::Rect,
};
use workbench::{TileEvent, TileId, TilePath};

pub(super) enum OuterCapture {
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
impl Shell {
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
            .or_default()
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
            .or_default()
            .scene()
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
    pub(super) fn outer_press(&mut self, slot: usize, size: (u32, u32), x: f32, y: f32) -> bool {
        if !self.prepare_outer(slot, size) {
            return false;
        }
        let hit = self
            .outer_frames
            .borrow_mut()
            .get_mut(&slot)
            .and_then(|frame| frame.hit(x, y));
        if let Some(hit)=&hit {
            let control=match hit {
                HeaderHit::Tab(id)=>Some(crate::frame_controls::Control::Tab(*id)),
                HeaderHit::Close(id)=>Some(crate::frame_controls::Control::Close(*id)),
                HeaderHit::Divider {target,..}=>self.outer_frames.borrow().get(&slot).and_then(|frame|frame.controls.order().into_iter().find(|c|matches!(c,crate::frame_controls::Control::Divider{path,index,..} if *path==target.path && *index==target.index))),
            };
            if let Some(control)=control {self.enter_frame_focus(slot,Some(control));}
        } else {self.leave_frame_focus();}
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
                self.outer_capture = Some(OuterCapture::Tab {
                    slot,
                    id,
                    start: (x, y),
                    session: *self.app.session_id.as_uuid(),
                    appearance: self.app.shell_chrome_config().appearance.clone(),
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
                OuterCapture::Tab {
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
            },
        };
        self.act(Action::OuterWorkspace {
            space: space(slot),
            event: event.into(),
        });
        true
    }
    pub(super) fn outer_release(&mut self, slot: usize, size: (u32, u32), x: f32, y: f32) -> bool {
        if self
            .outer_capture
            .as_ref()
            .is_some_and(|capture| match capture {
                OuterCapture::Tab {
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
