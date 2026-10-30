// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! One retained Frisket composition per window. In-house bodies share its
//! document, cascade, fragments and paint; native/foreign producers use slots.
use crate::frame_controls::{Control, FrameControls, FrameRoute, FrameStamp};
use crate::{
    panes::{FrisketLayout, PaneId},
    surface::Rect,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use workbench::{DropTarget, Edge, TileId, TileTree, Workspace};

#[derive(Clone, Debug)]
pub enum HeaderHit {
    Tab(TileId),
    Close(TileId),
    Divider {
        target: cambium::DividerTarget,
        area: Rect,
        axis: workbench::SplitAxis,
        shares: Vec<f32>,
        gaps: Vec<f32>,
    },
}

pub struct OuterFrame {
    window_scope: NodeId,
    window_root: NodeId,
    workspace_root: NodeId,
    workspace_area: Option<Rect>,
    main_view_switch: bool,
    scrolls: Vec<(u64, u32, u32)>,
    pub(crate) body_focus: Option<NodeId>,
    pub(crate) controls: FrameControls,
    pub(crate) layout: crate::ui::InlineLayoutHandle,
    workspace: Workspace,
    current: Option<TileId>,
    /// Physical window size and the layout scale the frame is laid out at:
    /// its DOM is in layout pixels, `size / scale`.
    size: (u32, u32),
    scale: f32,
    /// The scale `prepare` last composed at.
    laid_scale: f32,
    sheet: String,
    appearance: crate::shell_services::AppearanceConfig,
}

#[cfg(test)]
#[path = "outer_frame/native_aperture_tests.rs"]
mod native_aperture_tests;

#[cfg(test)]
#[path = "outer_frame/prepare_work_tests.rs"]
mod prepare_work_tests;

#[path = "outer_frame/impl_default_for_outer_frame.rs"]
mod impl_default_for_outer_frame;
#[path = "outer_frame/impl_drop_for_outer_frame.rs"]
mod impl_drop_for_outer_frame;
#[path = "outer_frame/outer_frame_native_source_node.rs"]
mod outer_frame_native_source_node;
#[path = "outer_frame/outer_frame_new.rs"]
mod outer_frame_new;
#[path = "outer_frame/outer_frame_set_main_view_switch.rs"]
mod outer_frame_set_main_view_switch;
#[path = "outer_frame/tree_at.rs"]
mod tree_at;
use tree_at::tree_at;
#[path = "outer_frame/attr.rs"]
mod attr;
use attr::attr;
#[path = "outer_frame/nodes_at.rs"]
mod nodes_at;
use nodes_at::nodes_at;
#[path = "outer_frame/body_slots.rs"]
mod body_slots;
use body_slots::body_slots;
