// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn outer_move(&mut self, slot: usize, x: f32, y: f32) -> bool {
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
                let area = crate::chrome_view::workspace_area_for_window(
                    self.app.shell_chrome_config(),
                    slot,
                    size.0,
                    size.1,
                );
                let mut next = *rect;
                next.x = (rect.x + (x - start.0) / area.w.max(1.0))
                    .clamp(0.0, (1.0 - rect.width).max(0.0));
                next.y = (rect.y + (y - start.1) / area.h.max(1.0))
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

    pub(in super::super) fn outer_release(
        &mut self,
        slot: usize,
        size: (u32, u32),
        x: f32,
        y: f32,
    ) -> bool {
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
