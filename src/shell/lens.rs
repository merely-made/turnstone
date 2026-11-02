// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Lens windows: a second window as a pane host over the one app state.
//!
//! A lens owns its own frisket space; each graph pane owns its camera and
//! selection, so panes torn out of the primary keep their
//! retained runners and their identity. Arrangement rides the SESSION, so
//! adopting a session closes these and reopens that session's own.

use super::NodeSessions;
use std::sync::Arc;

use genet_winit_host::SurfaceHost;
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, Scene};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::action::Action;
use crate::panes::{PaneContent, PaneId};
use crate::surface::{Rect, SurfaceKind};

use inker::SessionClick;

use super::input::pointer_button;
use super::{CompositeLayer, PlannedLayer, PlannedScene, Shell, capture_composed, frame_placement};

/// One lens window's record: its platform window, present stack, size,
/// cursor, and graph gesture capture.
pub(super) struct LensWindow {
    pub(super) window: Arc<Window>,
    pub(super) host: Option<SurfaceHost>,
    pub(super) recovery: super::WindowRecovery,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) cursor: (f32, f32),
    /// The graph pane holding a pointer gesture (grab/pan), if any.
    pub(super) pointer_graph: Option<PaneId>,
    /// Which `App::lenses` pane space this window shows (stable; the space
    /// tombstones on close).
    pub(super) ordinal: usize,
    pub(super) focused: bool,
    pub(super) a11y: genet_winit_host::AccessKitBridge,
    pub(super) a11y_actions: super::a11y_bridge::ActionQueue,
    pub(super) a11y_epoch: Option<u64>,
    pub(super) a11y_forest: crate::foreign_a11y::ForeignA11y,
    pub(super) a11y_scope: crate::window_projection::WindowScope,
    pub(super) a11y_routes: crate::a11y::A11yRoutes,
    pub(super) a11y_tree: Option<accesskit::TreeUpdate>,
    pub(super) a11y_placements: Vec<(uuid::Uuid, accesskit::Rect)>,
}

/// Platform-independent Lens pane custody, before fresh surface selection.
/// App plans and renderer generation validation remain owned by the host.
pub(super) trait LensPanePointerHost {
    fn lens_pane_admission(&self) -> Option<super::input::ContributedPointer>;
    fn lens_pane_admission_current(&self, admitted: super::input::ContributedPointer) -> bool;
    fn lens_cancel_stale_admission(&mut self);
    fn lens_release_admission(&mut self, window: u64, x: f32, y: f32);
}

#[cfg(test)]
#[path = "lens_tests.rs"]
mod tests;

#[path = "lens/lens_press_hit.rs"]
mod lens_press_hit;
pub(super) use lens_press_hit::lens_press_hit;
#[path = "lens/reject_lens_pane_press.rs"]
mod reject_lens_pane_press;
pub(super) use reject_lens_pane_press::reject_lens_pane_press;
#[path = "lens/dispatch_lens_pane_release.rs"]
mod dispatch_lens_pane_release;
pub(super) use dispatch_lens_pane_release::dispatch_lens_pane_release;
#[path = "lens/impl_lens_pane_pointer_host_for.rs"]
mod impl_lens_pane_pointer_host_for;
#[path = "lens/lens_content_plan.rs"]
mod lens_content_plan;
#[path = "lens/shell_lens_event.rs"]
mod shell_lens_event;
#[path = "lens/shell_lens_pane_wheel.rs"]
mod shell_lens_pane_wheel;
#[path = "lens/shell_render_lens.rs"]
mod shell_render_lens;
pub(super) use lens_content_plan::lens_content_plan;
#[path = "lens/lens_content_plan_with_rects.rs"]
mod lens_content_plan_with_rects;
use lens_content_plan_with_rects::lens_content_plan_with_rects;
#[path = "lens/lens_pane_content.rs"]
mod lens_pane_content;
use lens_pane_content::lens_pane_content;
#[path = "lens/append_foreign_lens.rs"]
mod append_foreign_lens;
pub(super) use append_foreign_lens::append_foreign_lens;

#[path = "lens/present_native_lens.rs"]
mod present_native_lens;
