// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained Frisket control dispatch. The host still owns Workspace mutation.
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Key, KeyEvent, NamedKey,
    PointerClick, Slot, WorkspaceModel,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use workbench::{TileEvent, TileId, TilePath, TileTree, Workspace, WorkspaceEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// An actual in-house body node; input stays with its retained pane owner.
    Body {
        tile: TileId,
        node: NodeId,
    },
    Tab(TileId),
    Close(TileId),
    Divider {
        path: TilePath,
        index: usize,
        left: Vec<TileId>,
        right: Vec<TileId>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameRoute {
    pub admission: u64,
    pub revision: u64,
    pub control: Control,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrameStamp {
    pub admission: u64,
    pub revision: u64,
    pub focus: Option<Control>,
}
struct State {
    workspace: Workspace,
    current: Option<TileId>,
    events: Vec<WorkspaceEvent>,
    mounts: std::collections::HashMap<TileId, cambium::RetainedMount>,
}
type View = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
type Runner = GenetAppRunner<State, fn(&State) -> View, View, ()>;

static NEXT: AtomicU64 = AtomicU64::new(1);
pub(crate) struct FrameControls {
    pub dom: DomHandle,
    runner: Runner,
    scope: Option<u128>,
    admission: u64,
    revision: u64,
    focus: Option<Control>,
    body_cursor: u64,
}

#[path = "frame_controls/view.rs"]
mod view;
use view::view;
#[path = "frame_controls/frame_controls_divider_tree.rs"]
mod frame_controls_divider_tree;
#[path = "frame_controls/frame_controls_withdraw_mounts.rs"]
mod frame_controls_withdraw_mounts;
#[path = "frame_controls/impl_default_for_frame_controls.rs"]
mod impl_default_for_frame_controls;
#[path = "frame_controls/nodes.rs"]
mod nodes;
pub(crate) use nodes::nodes;
#[path = "frame_controls/qual.rs"]
mod qual;
use qual::qual;
#[path = "frame_controls/attr.rs"]
mod attr;
use attr::attr;
#[path = "frame_controls/has_class.rs"]
mod has_class;
use has_class::has_class;
#[path = "frame_controls/tree_at.rs"]
mod tree_at;
use tree_at::tree_at;
