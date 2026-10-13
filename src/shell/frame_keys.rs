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
    pub(super) return_focus: FocusTarget,
    pub(super) return_pane: Option<PaneId>,
}
fn space(slot: usize) -> SpaceRef {
    if slot == 0 {
        SpaceRef::Primary
    } else {
        SpaceRef::Lens(slot - 1)
    }
}
pub(super) fn header_focus_in_window(owner: Option<&HeaderFocus>, window: u64, session: &uuid::Uuid) -> bool {
    owner.is_some_and(|owner| owner.slot as u64 == window && owner.session == *session)
}

fn retire_header_focus(owner: &mut Option<HeaderFocus>, window: u64) -> Option<HeaderFocus> {
    if owner.as_ref().is_some_and(|owner| owner.slot as u64 == window) { owner.take() } else { None }
}

/// Host editors retain first claim; a focused header then precedes body focus.
pub(super) fn accessibility_focus(
    host: Option<accesskit::NodeId>, header: Option<accesskit::NodeId>, body: Option<accesskit::NodeId>,
) -> Option<accesskit::NodeId> {
    host.or(header).or(body)
}

pub(super) fn return_focus_is_valid(
    app: &crate::app::App, owner: &HeaderFocus, active: Option<PaneId>,
    plan: &[crate::surface::Surface],
) -> bool {
    owner.session == *app.session_id.as_uuid() && owner.return_pane == active && match owner.return_focus {
        FocusTarget::Graph(p) | FocusTarget::Pane(p) => app.space_of(p) == Some(space(owner.slot)),
        FocusTarget::Content { node, appearance } => plan.iter().any(|surface| {
            surface.id == appearance
                && matches!(surface.kind, crate::surface::SurfaceKind::Content(id) if id == node)
        }),
        _ => false,
    }
}

impl Shell {
    /// Withdrawal cannot restore old body focus into a different physical window.
    pub(super) fn retire_frame_focus(&mut self, window: u64) {
        if let Some(owner) = retire_header_focus(&mut self.outer_focus, window) {
            if let Some(frame) = self.outer_frames.borrow_mut().get_mut(&owner.slot) {
                frame.focus_control(None);
            }
            self.request_redraw();
        }
    }

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
            let plan = self.input_plan(owner.slot as u64);
            let return_valid = return_focus_is_valid(&self.app, &owner, active, &plan);
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
            meta: self.meta,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_focus_retirement_is_window_scoped_and_terminal() {
        let app = crate::app::App::test_stub();
        let original_focus = app.focus;
        let mut owner = Some(HeaderFocus {
            slot: 2, session: *app.session_id.as_uuid(), return_focus: app.focus,
            return_pane: app.active_pane,
        });
        assert!(header_focus_in_window(owner.as_ref(), 2, app.session_id.as_uuid()));
        assert!(!header_focus_in_window(owner.as_ref(), 0, app.session_id.as_uuid()));
        let next_session = uuid::Uuid::from_u128(app.session_id.as_uuid().as_u128().wrapping_add(1));
        assert!(!header_focus_in_window(owner.as_ref(), 2, &next_session));
        assert!(retire_header_focus(&mut owner, 0).is_none());
        assert!(owner.is_some());
        assert!(retire_header_focus(&mut owner, 2).is_some());
        assert!(owner.is_none());
        assert!(retire_header_focus(&mut owner, 2).is_none());
        assert_eq!(app.focus, original_focus);
    }

    #[test]
    fn header_accessibility_focus_precedes_body_but_keeps_host_editor_priority() {
        let app = crate::app::App::test_stub();
        let mut frame = crate::outer_frame::OuterFrame::default();
        frame.prepare(&app.frisket, app.active_pane, app.maximized, (1180, 800),
            &app.shell_chrome_config().appearance, app.session_id.as_uuid().as_u128()).unwrap();
        let target = frame.controls.order().into_iter().find(|c| matches!(c, Control::Tab(_))).unwrap();
        assert!(frame.focus_control(Some(target)));
        let projection = frame.accessibility(true).unwrap();
        let header = projection.focus.expect("actual retained header focus");
        let body = accesskit::NodeId(u64::MAX - 1);
        assert_eq!(accessibility_focus(None, Some(header), Some(body)), Some(header));
        let editor = accesskit::NodeId(u64::MAX - 2);
        assert_eq!(accessibility_focus(Some(editor), Some(header), Some(body)), Some(editor));
        assert_eq!(accessibility_focus(None, None, Some(body)), Some(body));
    }

    #[test]
    fn actual_frame_prepare_uses_session_identity_and_refuses_retired_route() {
        let app = crate::app::App::test_stub();
        let mut frame = crate::outer_frame::OuterFrame::default();
        let scope = app.session_id.as_uuid().as_u128();
        let bodies = frame.prepare(&app.frisket, app.active_pane, app.maximized, (1180, 800),
            &app.shell_chrome_config().appearance, scope).unwrap();
        assert!(!bodies.is_empty());
        let control = frame.controls.order().into_iter().find(|c| matches!(c, Control::Tab(_))).unwrap();
        let route = frame.controls.route(control);
        assert!(frame.accessibility_action(&route, accesskit::Action::Focus).is_some());
        frame.prepare(&app.frisket, app.active_pane, app.maximized, (1180, 800),
            &app.shell_chrome_config().appearance, scope.wrapping_add(1)).unwrap();
        assert!(frame.accessibility_action(&route, accesskit::Action::Click).is_none());
        assert!(frame.controls.focus().is_none());
    }
}
