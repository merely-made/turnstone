// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Runtime ownership below pane presentation.
//!
//! A graph-bearing pane names a [`GraphId`], never a process-wide canvas.  The
//! pool owns the live graph runtimes; [`GraphPaneViews`] owns the pane-local
//! viewport that is installed only for a particular render or input pass.  A
//! Workbench is deliberately not nested in either: its durable arrangement is
//! keyed by a graph/Forme pair in [`FormeRuntimePool`].

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use mere::canvas::{
    Canvas, OverlayRefusal, PhysicsChoice, PhysicsDepthSource, PhysicsKindSource, PhysicsLaw,
    PhysicsMassSource, PhysicsOverlay, Viewport,
};
use mere::forme::FormeRef;
use mere::platen::Workbench;

use crate::panes::{GraphId, PaneId, SessionId};

/// One live graph authority and the resources currently coupled to it.
///
/// `Canvas` is still Mere's all-in-one graph/physics implementation.  Keeping
/// it inside the graph runtime makes that incumbent coupling explicit while
/// preventing the host from treating a window or pane as the graph owner.
pub struct GraphRuntime {
    /// Process-local incarnation; durable graph/document IDs never name a live gesture.
    pub(crate) instance: uuid::Uuid,
    pub graph: GraphId,
    pub session: Option<SessionId>,
    pub canvas: Canvas,
    pub(crate) scene_save_pending: bool,
    pub(crate) scene_restore_pending: bool,
    pub(crate) scene_transition: Option<mere::canvas::physics_controls::ArrangementTransition>,
    /// What the last composing physics control came to, for the Arrange pane.
    pub(crate) physics_status: Option<String>,
    pub(crate) view: crate::scene_state::ViewState,
    origin: Option<crate::host_journal::RuntimeOrigin>,
    pub(crate) persistence:
        crate::session_persistence::SessionPersistence<pandect::graph_placement::PlacementProfile>,
}

/// The live graph authorities available to this process.
///
/// `active` is a compatibility cursor for pre-A2 callers. New pane render,
/// input, command, and content paths must ask for a graph explicitly. Keeping
/// the cursor inside the pool is an intentionally narrow transition aid: it
/// cannot replace the pane-to-graph lookup at those boundaries.
pub struct GraphRuntimePool {
    runtimes: HashMap<GraphId, GraphRuntime>,
    active: GraphId,
    display_rate: Option<Option<u32>>,
    journal: crate::host_journal::SharedJournal,
}

/// A graph pane's independently-owned viewport.
///
/// The canvas owns graph truth and its working viewport for one pass. This
/// map stashes that viewport by `PaneId`, so two panes can use the same graph
/// without silently sharing camera, pan inertia, yaw, or tilt.
#[derive(Default)]
pub struct GraphPaneViews {
    owners: HashMap<PaneId, GraphId>,
    pending_fits: std::collections::HashSet<PaneId>,
    scopes: HashMap<PaneId, Option<Vec<uuid::Uuid>>>,
    neighborhoods: HashMap<PaneId, uuid::Uuid>,
    forme_regions: HashMap<PaneId, mere::canvas::FormeRegion>,
    forme_scopes: HashMap<PaneId, uuid::Uuid>,
    viewports: HashMap<PaneId, Viewport>,
    selections: HashMap<PaneId, Vec<uuid::Uuid>>,
}

/// A Forme runtime is an arrangement projection scope, never graph truth or a
/// pane instance. Its concrete geometry/physics is introduced by the first
/// Forme projection consumer; the pool establishes the key now so Graph and
/// Workbench panes cannot fall back to an application-global arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormeRuntimeKey {
    pub graph: GraphId,
    pub forme: FormeRef,
}

#[derive(Debug)]
pub struct FormeRuntime {
    pub key: FormeRuntimeKey,
    /// Curated member arrangement for exactly this graph/Forme source. It is
    /// not a window-global workbench and it is not graph truth.
    pub workbench: Workbench,
}

#[derive(Default)]
pub struct FormeRuntimePool {
    runtimes: HashMap<FormeRuntimeKey, FormeRuntime>,
}

#[cfg(test)]
#[path = "runtime_pool/tests.rs"]
mod tests;

#[path = "runtime_pool/forme_runtime_pool_get_or_create.rs"]
mod forme_runtime_pool_get_or_create;
#[path = "runtime_pool/graph_pane_views_forme_region.rs"]
mod graph_pane_views_forme_region;
#[path = "runtime_pool/graph_runtime_origin.rs"]
mod graph_runtime_origin;
#[path = "runtime_pool/graph_runtime_pool_new.rs"]
mod graph_runtime_pool_new;
#[path = "runtime_pool/graph_runtime_pool_physics.rs"]
mod graph_runtime_pool_physics;
#[path = "runtime_pool/impl_deref_for_graph_runtime_pool.rs"]
mod impl_deref_for_graph_runtime_pool;
#[path = "runtime_pool/impl_deref_mut_for_graph_runtime.rs"]
mod impl_deref_mut_for_graph_runtime;
