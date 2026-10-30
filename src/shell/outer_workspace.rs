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

#[cfg(test)]
#[path = "outer_workspace/tests.rs"]
mod tests;

#[path = "outer_workspace/space.rs"]
mod space;
use space::space;
#[path = "outer_workspace/retire_outer_capture.rs"]
mod retire_outer_capture;
pub(super) use retire_outer_capture::retire_outer_capture;
#[path = "outer_workspace/frame_body_geometry.rs"]
mod frame_body_geometry;
pub(super) use frame_body_geometry::frame_body_geometry;
#[path = "outer_workspace/frame_body_rects.rs"]
mod frame_body_rects;
pub(super) use frame_body_rects::frame_body_rects;
#[path = "outer_workspace/pane_selector_family.rs"]
mod pane_selector_family;
use pane_selector_family::pane_selector_family;
#[path = "outer_workspace/ordinary_pane_selector.rs"]
mod ordinary_pane_selector;
use ordinary_pane_selector::ordinary_pane_selector;
#[path = "outer_workspace/shell_outer_move.rs"]
mod shell_outer_move;
#[path = "outer_workspace/shell_retained_pane_selector_target.rs"]
mod shell_retained_pane_selector_target;
