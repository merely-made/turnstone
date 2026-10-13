// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::Shell;
use crate::{
    action::{Action, SpaceRef},
    frame_controls::Control,
    panes::{PaneContent, PaneId},
    surface::FocusTarget,
};
use winit::keyboard::{Key, NamedKey};
use workbench::{TileEvent, WorkspaceEvent};

pub(super) struct HeaderFocus {
    pub slot: usize,
    pub session: uuid::Uuid,
    return_focus: FocusTarget,
    return_pane: Option<PaneId>,
}
fn space(slot: usize) -> SpaceRef {
    if slot == 0 {
        SpaceRef::Primary
    } else {
        SpaceRef::Lens(slot - 1)
    }
}
impl Shell {
    pub(super) fn enter_frame_focus(&mut self, slot: usize, control: Option<Control>) -> bool {
        let target = control.or_else(|| {
            self.outer_frames.borrow().get(&slot).and_then(|frame| {
                frame.controls.order().into_iter().filter(|c|frame.visible_control(c))
                .find(|c|matches!(c,Control::Tab(id) if Some(PaneId(id.0))==self.app.active_pane))
                .or_else(||frame.controls.order().into_iter().find(|c|frame.visible_control(c)))
            })
        });
        let Some(target) = target else { return false };
        if !self
            .outer_frames
            .borrow_mut()
            .get_mut(&slot)
            .is_some_and(|f| f.focus_control(Some(target)))
        {
            return false;
        }
        if self
            .outer_focus
            .as_ref()
            .is_none_or(|f| f.slot != slot || f.session != *self.app.session_id.as_uuid())
        {
            if let Some(old) = self.outer_focus.take() {
                if let Some(frame) = self.outer_frames.borrow_mut().get_mut(&old.slot) {
                    frame.focus_control(None);
                }
            }
            self.outer_focus = Some(HeaderFocus {
                slot,
                session: *self.app.session_id.as_uuid(),
                return_focus: self.app.focus,
                return_pane: self.app.active_pane,
            });
        }
        self.a11y_frames_since_push = 30;
        if slot == 0 {
            if let Some(window) = &self.window {
                window.set_ime_allowed(false);
            }
        } else {
            if let Some(lens) = self.lens_windows.values().find(|l| l.ordinal + 1 == slot) {
                lens.window.set_ime_allowed(false);
            }
        }
        self.request_redraw();
        true
    }
    pub(super) fn leave_frame_focus(&mut self) {
        let Some(owner) = self.outer_focus.take() else {
            return;
        };
        if let Some(frame) = self.outer_frames.borrow_mut().get_mut(&owner.slot) {
            frame.focus_control(None);
        }
        if owner.session == *self.app.session_id.as_uuid() {
            let active = self
                .app
                .active_pane
                .filter(|p| self.app.space_of(*p) == Some(space(owner.slot)))
                .or_else(|| {
                    self.app
                        .space(space(owner.slot))
                        .and_then(|s| s.outer_workspace().ok())
                        .and_then(|w| w.active_ids().first().copied())
                        .map(|id| PaneId(id.0))
                });
            let return_valid = owner.return_pane == active && match owner.return_focus {
                FocusTarget::Graph(p) | FocusTarget::Pane(p) => {
                    self.app.space_of(p) == Some(space(owner.slot))
                },
                FocusTarget::Content { node, appearance } => self.surface_plan().iter().any(|s| {
                    s.id == appearance
                        && matches!(s.kind,crate::surface::SurfaceKind::Content(id) if id==node)
                }),
                _ => false,
            };
            if return_valid {
                self.app.focus = owner.return_focus;
            } else if let Some(pane) = active {
                self.app.active_pane = Some(pane);
                self.app.focus = if matches!(self.app.pane_content(pane), Some(PaneContent::Orrery))
                {
                    FocusTarget::Graph(pane)
                } else {
                    FocusTarget::Pane(pane)
                };
            }
        }
        self.a11y_frames_since_push = 30;
        self.request_redraw();
        if owner.slot == 0 {
            if let Some(window) = &self.window {
                window.set_ime_allowed(self.accepts_controlled_text());
            }
        } else {
            if let Some(lens) = self
                .lens_windows
                .values()
                .find(|l| l.ordinal + 1 == owner.slot)
            {
                lens.window.set_ime_allowed(self.accepts_controlled_text());
            }
        }
    }
    pub(super) fn apply_frame_events(
        &mut self,
        slot: usize,
        events: Vec<WorkspaceEvent>,
        save_dividers: bool,
    ) {
        let divider = events
            .iter()
            .any(|e| matches!(e, WorkspaceEvent::Tile(TileEvent::DividerMoved { .. })));
        for event in events {
            self.act(Action::OuterWorkspace {
                space: space(slot),
                event,
            });
        }
        if divider && save_dividers {
            self.act(Action::SaveSession);
        }
        self.a11y_frames_since_push = 30;
    }
    pub(super) fn deliver_frame_key(&mut self, slot: usize, size: (u32, u32), key: &Key) -> bool {
        if self
            .outer_focus
            .as_ref()
            .is_some_and(|f| f.session != *self.app.session_id.as_uuid())
        {
            self.leave_frame_focus();
        }
        if self.app.omnibar.open
            || self.app.document_find.open
            || self.app.user_agent_decision.is_open()
        {
            return false;
        }
        if self.ctrl && matches!(key, Key::Named(NamedKey::F6)) {
            if self.outer_focus.as_ref().is_some_and(|f| f.slot == slot) {
                self.leave_frame_focus();
                return true;
            }
            if !self.prepare_outer(slot, size) {
                return false;
            }
            return self.enter_frame_focus(slot, None);
        }
        if self.outer_focus.as_ref().is_none_or(|f| f.slot != slot) {
            return false;
        }
        if self.ctrl
            && matches!(key,Key::Character(s) if s.eq_ignore_ascii_case("l")||s.eq_ignore_ascii_case("k"))
        {
            self.leave_frame_focus();
            return false;
        }
        if !self.prepare_outer(slot, size) {
            self.leave_frame_focus();
            return true;
        }
        let modifiers = cambium::Modifiers {
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.alt,
            meta: false,
        };
        let Some(event) = cambium_winit::key_event_from_winit(key, modifiers) else {
            return true;
        };
        let (events, exit) = self
            .outer_frames
            .borrow_mut()
            .get_mut(&slot)
            .unwrap()
            .key(event);
        self.apply_frame_events(slot, events, true);
        self.prepare_outer(slot, size);
        if exit {
            self.leave_frame_focus();
        }
        self.request_redraw();
        true
    }
    pub(super) fn apply_frame_a11y(
        &mut self,
        route: &crate::frame_controls::FrameRoute,
        action: accesskit::Action,
    ) -> bool {
        if self.app.omnibar.open
            || self.app.document_find.open
            || self.app.user_agent_decision.is_open()
        {
            return false;
        }
        if !self.prepare_outer(0, (self.width, self.height)) {
            return false;
        }
        let events = self
            .outer_frames
            .borrow_mut()
            .get_mut(&0)
            .and_then(|frame| frame.accessibility_action(route, action));
        let Some(events) = events else { return false };
        if !self.enter_frame_focus(0, Some(route.control.clone())) {
            return false;
        }
        self.apply_frame_events(0, events, true);
        self.prepare_outer(0, (self.width, self.height));
        self.request_redraw();
        true
    }
}
