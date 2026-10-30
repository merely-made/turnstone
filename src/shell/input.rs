// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Input routing: pointer, wheel, keys, and the drag gestures, shared by winit
//! and the scenario runner so one description drives two runners.
//!
//! Focus picks the lane, for keys as for the pointer: the omnibar when open,
//! the focused page when one holds focus, the canvas otherwise. Ephemeral
//! input (scroll, hover, blur) rides state directly per the gesture law;
//! durable intent becomes an `Action`.

use super::{NodeSessions, NodeSurfaces};
use winit::event::{Force, MouseButton, Touch, TouchPhase};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::CursorIcon;

use crate::panes::PaneContent;
use inker::{SessionClick, SessionScrollKey};
use mere::canvas::PointerButton;
use taproot::AutomatableExt as _;

use crate::action::{Action, CaretMove};
use crate::surface::Rect;

use super::{Shell, decode_sprite};

#[derive(Clone, Copy)]
pub(super) struct ContributedPointer {
    pub surface: crate::surface::Surface,
    pub generation: u64,
    pub window: u64,
}

/// Platform-independent part of the primary native caller. The host supplies
/// placement synchronization and receiver delivery; these shared branches own
/// the shell capture decisions before renderer-specific pane/graph gestures.
/// Keeping the non-Content branch here lets native input fixtures exercise the
/// same fresh-hit capture assignment and terminal-event admission as Shell.
pub(super) trait PrimaryPointerHost {
    fn pointer_state(
        &mut self,
    ) -> (
        &mut crate::surface::FocusTarget,
        &mut super::window_input::Pool,
        &mut Option<crate::surface::SurfaceKind>,
    );
    fn sync_pointer_placements(&mut self);
    fn pointer_content_focus(&mut self, appearance: crate::surface::SurfaceId);
    fn pointer_content_mouse(
        &mut self,
        appearance: crate::surface::SurfaceId,
        local: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    );
    fn pointer_window_mouse(
        &mut self,
        appearance: Option<crate::surface::SurfaceId>,
        position: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    );
    fn pointer_user_agent_decision_open(&self) -> bool;
    fn pointer_has_foreign_capture(&self) -> bool;
    fn pointer_cancel_primary_content(&mut self);
    fn pointer_has_contributed_capture(&self) -> bool;
    fn pointer_release_contributed(&mut self, x: f32, y: f32);
    fn pointer_content_hovered(&mut self, node: uuid::Uuid);
    fn pointer_redraw(&mut self);
}

/// Portable admission for primary presses consumed before surface dispatch.
/// The host executes the same App action as Shell's chrome/modal handlers.
pub(super) trait PrimaryPressAdmissionHost: PrimaryPointerHost {
    fn pointer_press_action(&mut self, action: Action);
    fn pointer_omnibar_open(&self) -> bool;
    fn pointer_retire_primary_pointers(&mut self);
}

#[cfg(test)]
#[path = "input/cursor_tests.rs"]
mod cursor_tests;

#[cfg(test)]
#[path = "input/contributed_custody_tests.rs"]
mod contributed_custody_tests;

#[path = "input/pointer_button.rs"]
mod pointer_button;
pub(super) use pointer_button::pointer_button;
#[path = "input/surface_mouse_button.rs"]
mod surface_mouse_button;
pub(super) use surface_mouse_button::surface_mouse_button;
#[path = "input/surface_button_mask.rs"]
mod surface_button_mask;
use surface_button_mask::surface_button_mask;
#[path = "input/mouse_pointer_event.rs"]
mod mouse_pointer_event;
pub(super) use mouse_pointer_event::mouse_pointer_event;
#[path = "input/surface_cursor_icon.rs"]
mod surface_cursor_icon;
use surface_cursor_icon::surface_cursor_icon;
#[path = "input/surface_cursor_label.rs"]
mod surface_cursor_label;
use surface_cursor_label::surface_cursor_label;
#[path = "input/contributed_pointer_is_current.rs"]
mod contributed_pointer_is_current;
#[path = "input/dispatch_primary_action_press.rs"]
mod dispatch_primary_action_press;
use dispatch_primary_action_press::dispatch_primary_action_press;
#[path = "input/dispatch_primary_chrome_press.rs"]
mod dispatch_primary_chrome_press;
pub(super) use dispatch_primary_chrome_press::dispatch_primary_chrome_press;
#[path = "input/reject_primary_omnibar_press.rs"]
mod reject_primary_omnibar_press;
pub(super) use reject_primary_omnibar_press::reject_primary_omnibar_press;
#[path = "input/dispatch_primary_context_press.rs"]
mod dispatch_primary_context_press;
pub(super) use dispatch_primary_context_press::dispatch_primary_context_press;
#[path = "input/impl_primary_press_admission_host_for.rs"]
mod impl_primary_press_admission_host_for;
#[path = "input/reject_primary_modal_press.rs"]
mod reject_primary_modal_press;
pub(super) use reject_primary_modal_press::reject_primary_modal_press;
#[path = "input/dispatch_primary_press.rs"]
mod dispatch_primary_press;
pub(super) use dispatch_primary_press::dispatch_primary_press;
#[path = "input/dispatch_primary_content_move.rs"]
mod dispatch_primary_content_move;
pub(super) use dispatch_primary_content_move::dispatch_primary_content_move;
#[path = "input/dispatch_primary_release.rs"]
mod dispatch_primary_release;
pub(super) use dispatch_primary_release::dispatch_primary_release;
#[path = "input/workbench_resize_target.rs"]
mod workbench_resize_target;
use workbench_resize_target::workbench_resize_target;
#[path = "input/impl_primary_pointer_host_for_shell.rs"]
mod impl_primary_pointer_host_for_shell;
#[path = "input/shell_apply_pending_surface_cursor.rs"]
mod shell_apply_pending_surface_cursor;
#[path = "input/shell_deliver_hover.rs"]
mod shell_deliver_hover;
#[path = "input/shell_deliver_press.rs"]
mod shell_deliver_press;
#[path = "input/shell_note_miss.rs"]
mod shell_note_miss;
