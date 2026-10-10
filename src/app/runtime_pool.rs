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
    pub graph: GraphId,
    pub session: Option<SessionId>,
    pub canvas: Canvas,
    origin: Option<crate::host_journal::RuntimeOrigin>,
    pub(crate) persistence: crate::session_persistence::SessionPersistence<pandect::graph_placement::PlacementProfile>,
}

impl GraphRuntime {
    pub(crate) fn origin(&self) -> Option<crate::host_journal::RuntimeOrigin> {
        self.origin
    }
    pub fn new(graph: GraphId, session: Option<SessionId>, canvas: Canvas) -> Self {
        Self {
            graph,
            session,
            canvas,
            origin: None,
            persistence: crate::session_persistence::SessionPersistence::Unbound,
        }
    }
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

impl GraphRuntimePool {
    pub fn new(graph: GraphId, session: Option<SessionId>, canvas: Canvas) -> Self {
        Self::with_journal(
            graph,
            session,
            canvas,
            crate::host_journal::shared_journal(),
        )
    }

    pub(crate) fn with_journal(
        graph: GraphId,
        session: Option<SessionId>,
        canvas: Canvas,
        journal: crate::host_journal::SharedJournal,
    ) -> Self {
        let runtime = GraphRuntime::new(graph, session, canvas);
        let mut runtimes = HashMap::new();
        runtimes.insert(graph, runtime);
        let mut pool = Self {
            runtimes,
            active: graph,
            display_rate: None,
        journal,
        }
    ;
        let _ = pool.bind_recording(graph);
        pool
    }

    /// Recorder installation changes host instrumentation, not graph truth.
    pub(crate) fn suspend_recording(&mut self, graph: GraphId) {
        if let Some(runtime) = self.runtimes.get_mut(&graph) {
            runtime.origin = None;
            runtime.canvas.ingest_graph(|graph| {
                graph.set_recorder(None);
                false
            });
        }
    }

    pub(crate) fn bind_recording(
        &mut self,
        graph: GraphId,
    ) -> Result<crate::host_journal::RuntimeOrigin, String> {
        self.suspend_recording(graph);
        let runtime = self
            .runtimes
            .get_mut(&graph)
            .ok_or("graph runtime is absent")?;
        if matches!(
            &runtime.persistence,
            crate::session_persistence::SessionPersistence::Refused { .. }
        ) {
            return Err("refused graph cannot record live edits".into());
        }
        let origin = self
            .journal
            .lock()
            .map_err(|_| "behavior journal capture lock poisoned")?
            .allocate_origin(graph, runtime.session)
            .map_err(|error| error.to_string())?;
        let recorder = crate::host_journal::recorder(self.journal.clone(), origin);
        runtime.canvas.ingest_graph(|graph| {
            graph.set_recorder(Some(recorder));
            false
        });
        runtime.origin = Some(origin);
        Ok(origin)
    }

    pub(crate) fn replace_active_canvas(&mut self, mut canvas: Canvas) {
        if let Some(rate) = self.display_rate {
            canvas.set_physics_display_rate(rate);
        }
        let runtime = self.runtimes.get_mut(&self.active).expect("active runtime");
        runtime.persistence.clear_placement();
        runtime.canvas = canvas;
        runtime.origin = None;
        let _ = self.bind_recording(self.active);
    }

    pub(crate) fn active_persistence(&self) -> &crate::session_persistence::SessionPersistence<pandect::graph_placement::PlacementProfile> {
        &self.runtimes.get(&self.active).expect("active runtime").persistence
    }

    pub(crate) fn adopt_graph(
        &mut self, graph: mere::kernel::graph::Graph,
        state: crate::session_persistence::SessionPersistence<pandect::graph_placement::PlacementProfile,
        >,
    ) {
        let runtime = self.runtimes.get_mut(&self.active).expect("active runtime");
        runtime.canvas.set_graph(graph);
        runtime.persistence = state;
        runtime.origin = None;
        let _ = self.bind_recording(self.active);
    }

    /// Unqualified graph replacement revokes any prior placement. It cannot
    /// unlock a refused destination. Adoption carries graph and state together.
    pub fn set_graph(&mut self, graph: mere::kernel::graph::Graph) {
        let runtime = self.runtimes.get_mut(&self.active).expect("active runtime");
        runtime.persistence.clear_placement();
        runtime.canvas.set_graph(graph);
        runtime.origin = None;
        let _ = self.bind_recording(self.active);
    }

    pub fn active_graph(&self) -> GraphId {
        self.active
    }

    /// Primary native host's display budget, including an explicit unknown
    /// rate. New/reopened graph runtimes inherit it.
    pub fn set_physics_display_rate(&mut self, millihertz: Option<u32>) {
        if self.display_rate == Some(millihertz) {
            return;
        }
        self.display_rate = Some(millihertz);
        for runtime in self.runtimes.values_mut() {
            runtime.canvas.set_physics_display_rate(millihertz);
        }
    }

    pub fn contains(&self, graph: GraphId) -> bool {
        self.runtimes.contains_key(&graph)
    }

    pub fn get(&self, graph: GraphId) -> Option<&GraphRuntime> {
        self.runtimes.get(&graph)
    }

    pub fn get_mut(&mut self, graph: GraphId) -> Option<&mut GraphRuntime> {
        self.runtimes.get_mut(&graph)
    }

    pub fn canvas(&self, graph: GraphId) -> Option<&Canvas> {
        self.get(graph).map(|runtime| &runtime.canvas)
    }

    pub fn canvas_mut(&mut self, graph: GraphId) -> Option<&mut Canvas> {
        self.get_mut(graph).map(|runtime| &mut runtime.canvas)
    }

    pub fn active_canvas(&self) -> &Canvas {
        &self
            .runtimes
            .get(&self.active)
            .expect("the active graph runtime must exist")
            .canvas
    }

    pub fn active_canvas_mut(&mut self) -> &mut Canvas {
        &mut self
            .runtimes
            .get_mut(&self.active)
            .expect("the active graph runtime must exist")
            .canvas
    }

    /// Insert or replace one graph runtime, then make it the legacy cursor.
    ///
    /// A session switch still reloads its persisted graph through this method;
    /// a two-graph space instead inserts both runtimes and resolves each pane
    /// with [`Self::canvas`] or [`Self::canvas_mut`].
    pub fn activate_or_insert(
        &mut self,
        graph: GraphId,
        session: Option<SessionId>,
        mut canvas: Canvas,
    ) -> &mut GraphRuntime {
        if let Some(rate) = self.display_rate {
            canvas.set_physics_display_rate(rate);
        }
        self.runtimes
            .insert(graph, GraphRuntime::new(graph, session, canvas));
        self.active = graph;
        let _ = self.bind_recording(graph);
        self.runtimes
            .get_mut(&graph)
            .expect("the runtime was just inserted")
    }

    /// Select an already-open runtime without reloading graph truth.
    pub fn activate(&mut self, graph: GraphId) -> bool {
        if self.runtimes.contains_key(&graph) {
            self.active = graph;
            true
        } else {
            false
        }
    }

    /// Number of live graph authorities. Exposed for receipts and lifecycle
    /// checks, not as a window or pane count.
    pub fn len(&self) -> usize {
        self.runtimes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runtimes.is_empty()
    }

    /// Stable Surface IDs across every live graph authority.
    pub(crate) fn member_ids(&self) -> Vec<uuid::Uuid> {
        self.runtimes.values().flat_map(|runtime| runtime.canvas.graph().nodes().map(|(_, node)| node.id)).collect()
    }

    /// Resolve graph authority from member identity instead of the legacy
    /// active-runtime cursor. A live content callback carries its member, so
    /// focus changes must not retarget the mutation to another graph.
    pub fn graph_containing_member(&self, member: uuid::Uuid) -> Option<GraphId> {
        self.runtimes.iter().find_map(|(graph, runtime)| {
            runtime
                .canvas
                .graph()
                .get_node_by_id(member)
                .is_some()
                .then_some(*graph)
        })
    }
}

/// The active canvas's physics through its dynamics spec. Mere's canvas
/// exposes physics as a `DynamicsSpec` edited through the flat
/// `PhysicsChoice` view (dynamics grammar plan, F162); these keep Turnstone's
/// one-field reads and edits, each written into the spec as one change.
impl GraphRuntimePool {
    /// What the active canvas runs, as the flat view.
    pub fn physics(&self) -> PhysicsChoice {
        PhysicsChoice::live(self.active_canvas())
    }

    pub fn physics_law(&self) -> PhysicsLaw {
        self.physics().law
    }

    pub fn physics_overlays(&self) -> Vec<PhysicsOverlay> {
        self.physics().overlays
    }

    pub fn physics_kind_source(&self) -> PhysicsKindSource {
        self.physics().kind
    }

    pub fn physics_mass_source(&self) -> PhysicsMassSource {
        self.physics().mass
    }

    pub fn physics_depth_source(&self) -> PhysicsDepthSource {
        self.physics().depth
    }

    pub fn physics_profile_id(&self) -> Option<&'static str> {
        self.physics().profile_id()
    }

    /// Change the flat view and set it as one spec change. Overlays the law
    /// refuses are left out, and the refusal carries its reason.
    pub fn edit_physics(
        &mut self,
        edit: impl FnOnce(&mut PhysicsChoice),
    ) -> Result<(), OverlayRefusal> {
        let canvas = self.active_canvas_mut();
        let running = PhysicsChoice::live(canvas);
        let mut choice = running.clone();
        edit(&mut choice);
        let (choice, refusal) = choice.admitted();
        let applied = canvas.dynamics_spec().and_then(|mut spec| {
            choice.write_into(&mut spec, &running);
            canvas
                .set_dynamics_spec(&spec)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
        if let Err(error) = applied {
            tracing::warn!(%error, "the canvas refused the physics change");
        }
        refusal.map_or(Ok(()), Err)
    }

    pub fn set_physics_law(&mut self, law: PhysicsLaw) -> Result<(), OverlayRefusal> {
        self.edit_physics(|choice| choice.law = law)
    }

    pub fn set_physics_overlays(
        &mut self,
        overlays: Vec<PhysicsOverlay>,
    ) -> Result<(), OverlayRefusal> {
        self.edit_physics(|choice| choice.overlays = overlays)
    }

    pub fn set_physics_kind_source(&mut self, source: PhysicsKindSource) {
        let _ = self.edit_physics(|choice| choice.kind = source);
    }

    pub fn set_physics_mass_source(&mut self, source: PhysicsMassSource) {
        let _ = self.edit_physics(|choice| choice.mass = source);
    }

    pub fn set_physics_depth_source(&mut self, source: PhysicsDepthSource) {
        let _ = self.edit_physics(|choice| choice.depth = source);
    }

    /// Apply a catalog profile's law and overlays; `false` for an unknown id.
    pub fn apply_physics_profile(&mut self, id: &str) -> bool {
        let Some(profile) = self.physics().with_profile(id) else {
            return false;
        };
        let _ = self.edit_physics(|choice| *choice = profile);
        true
    }
}

/// Compatibility only for callers that have not yet crossed the A2 routing
/// boundary. It dereferences to the active runtime's canvas, while new code
/// must use the explicit graph lookup above.
impl Deref for GraphRuntimePool {
    type Target = Canvas;

    fn deref(&self) -> &Self::Target {
        self.active_canvas()
    }
}

impl DerefMut for GraphRuntimePool {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active_canvas_mut()
    }
}

/// A graph pane's independently-owned viewport.
///
/// The canvas owns graph truth and its working viewport for one pass. This
/// map stashes that viewport by `PaneId`, so two panes can use the same graph
/// without silently sharing camera, pan inertia, yaw, or tilt.
#[derive(Default)]
pub struct GraphPaneViews {
    viewports: HashMap<PaneId, Viewport>,
    selections: HashMap<PaneId, Vec<uuid::Uuid>>,
}

impl GraphPaneViews {
    /// Install `pane`'s saved viewport, or capture the runtime's current
    /// viewport for a fresh pane. Call before the pane's input/render pass.
    pub fn install(&mut self, pane: PaneId, canvas: &mut Canvas) {
        if let Some(viewport) = self.viewports.get(&pane).copied() {
            canvas.set_viewport(viewport);
        } else {
            self.viewports.insert(pane, canvas.viewport());
        }
        if let Some(selection) = self.selections.get(&pane) {
            canvas.set_selected_members(selection);
        } else {
            self.selections.insert(pane, canvas.selected_members());
        }
    }

    /// Save the working viewport after the pane's input/render pass.
    pub fn stash(&mut self, pane: PaneId, canvas: &Canvas) {
        self.viewports.insert(pane, canvas.viewport());
        self.selections.insert(pane, canvas.selected_members());
    }

    pub fn viewport(&self, pane: PaneId) -> Option<Viewport> {
        self.viewports.get(&pane).copied()
    }

    pub fn remove(&mut self, pane: PaneId) -> Option<Viewport> {
        self.selections.remove(&pane);
        self.viewports.remove(&pane)
    }

    /// The one selected graph member this pane publishes, if its selection is
    /// unambiguous. Multiple selections deliberately do not manufacture a
    /// focused member for followers.
    pub fn focused_member(&self, pane: PaneId) -> Option<uuid::Uuid> {
        let selection = self.selections.get(&pane)?;
        if selection.len() == 1 {
            Some(selection[0])
        } else {
            None
        }
    }

    pub fn selected_members(&self, pane: PaneId) -> Option<&[uuid::Uuid]> {
        self.selections.get(&pane).map(Vec::as_slice)
    }
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

impl FormeRuntimePool {
    pub fn get_or_create(&mut self, graph: GraphId, forme: FormeRef) -> &mut FormeRuntime {
        let key = FormeRuntimeKey { graph, forme };
        self.runtimes.entry(key).or_insert_with(|| FormeRuntime {
            key,
            workbench: Workbench::new(),
        })
    }

    pub fn get(&self, graph: GraphId, forme: FormeRef) -> Option<&FormeRuntime> {
        self.runtimes.get(&FormeRuntimeKey { graph, forme })
    }

    pub fn get_mut(&mut self, graph: GraphId, forme: FormeRef) -> Option<&mut FormeRuntime> {
        self.runtimes.get_mut(&FormeRuntimeKey { graph, forme })
    }

    pub fn iter(&self) -> impl Iterator<Item = &FormeRuntime> {
        self.runtimes.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut FormeRuntime> {
        self.runtimes.values_mut()
    }

    pub fn len(&self) -> usize {
        self.runtimes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runtimes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: u128) -> GraphId {
        GraphId::from_uuid(uuid::Uuid::from_u128(n))
    }

    #[test]
    fn display_budget_survives_graph_replacement_and_unknown_rate() {
        let a = graph(901);
        let b = graph(902);
        let assert_budget = |pool: &GraphRuntimePool, id, rate| {
            let actual = pool.canvas(id).unwrap().physics_step_budget().unwrap();
            let expected = mere::canvas::StepBudget::for_display(rate);
            assert_eq!(actual.per_frame, expected.per_frame);
            assert_eq!(actual.margin, expected.margin);
        };
        let mut pool = GraphRuntimePool::new(a, None, Canvas::new());
        pool.set_physics_display_rate(Some(144_000));
        pool.activate_or_insert(b, None, Canvas::new());
        assert_budget(&pool, a, Some(144_000));
        assert_budget(&pool, b, Some(144_000));
        pool.activate_or_insert(a, None, Canvas::new());
        assert_budget(&pool, a, Some(144_000));
        pool.set_physics_display_rate(None);
        for id in [a, b] {
            assert_budget(&pool, id, None);
        }
    }

    #[test]
    fn graph_pool_keeps_two_graph_authorities_independent() {
        let a = graph(1);
        let b = graph(2);
        let mut pool = GraphRuntimePool::new(a, None, Canvas::new());
        pool.active_canvas_mut().visit("https://a.example/");
        pool.activate_or_insert(b, None, Canvas::new())
            .canvas
            .visit("https://b.example/");

        assert_eq!(pool.len(), 2);
        assert!(
            pool.canvas(a)
                .expect("graph A")
                .graph()
                .get_node_by_url("https://a.example/")
                .is_some()
        );
        assert!(
            pool.canvas(a)
                .expect("graph A")
                .graph()
                .get_node_by_url("https://b.example/")
                .is_none()
        );
        assert!(
            pool.canvas(b)
                .expect("graph B")
                .graph()
                .get_node_by_url("https://b.example/")
                .is_some()
        );
    }

    #[test]
    fn pane_viewports_do_not_alias_on_one_graph() {
        let a = graph(1);
        let first = PaneId(10);
        let second = PaneId(11);
        let mut pool = GraphRuntimePool::new(a, None, Canvas::with_sample_graph());
        let mut views = GraphPaneViews::default();
        let canvas = pool.canvas_mut(a).expect("graph runtime");

        views.install(first, canvas);
        views.stash(first, canvas);
        views.install(second, canvas);
        views.stash(second, canvas);
        views.install(first, canvas);
        canvas.wheel(0.0, 240.0);
        views.stash(first, canvas);
        let first_view = views.viewport(first).expect("first view");

        views.install(second, canvas);
        let second_view = views.viewport(second).expect("second view");
        assert_ne!(first_view, second_view, "pane-local views do not alias");

        views.install(first, canvas);
        assert_eq!(canvas.viewport(), first_view);
    }

    #[test]
    fn pane_selections_do_not_alias_on_one_graph() {
        let a = graph(1);
        let first = PaneId(10);
        let second = PaneId(11);
        let mut pool = GraphRuntimePool::new(a, None, Canvas::new());
        let one = pool.active_canvas_mut().visit("https://one.example/");
        let one = pool.active_canvas().graph().get_node(one).expect("one").id;
        let two = pool.active_canvas_mut().visit("https://two.example/");
        let two = pool.active_canvas().graph().get_node(two).expect("two").id;
        let mut views = GraphPaneViews::default();
        let canvas = pool.canvas_mut(a).expect("graph runtime");

        views.install(first, canvas);
        canvas.select_member(one);
        views.stash(first, canvas);
        views.install(second, canvas);
        canvas.select_member(two);
        views.stash(second, canvas);

        assert_eq!(views.focused_member(first), Some(one));
        assert_eq!(views.focused_member(second), Some(two));
        views.install(first, canvas);
        assert_eq!(canvas.focused_member(), Some(one));
    }

    #[test]
    fn forme_runtime_key_keeps_graph_and_forme_separate() {
        let graph_a = graph(1);
        let graph_b = graph(2);
        let forme = FormeRef::Stored(mere::forme::FormeId::from_uuid(uuid::Uuid::from_u128(7)));
        let mut pool = FormeRuntimePool::default();

        pool.get_or_create(graph_a, forme);
        pool.get_or_create(graph_b, forme);

        assert_eq!(pool.len(), 2);
        assert!(pool.get(graph_a, forme).is_some());
        assert!(pool.get(graph_b, forme).is_some());
    }

    #[test]
    fn workbenches_are_owned_by_their_forme_runtime() {
        let graph_a = graph(1);
        let graph_b = graph(2);
        let forme = FormeRef::Stored(mere::forme::FormeId::from_uuid(uuid::Uuid::from_u128(7)));
        let member = uuid::Uuid::from_u128(42);
        let mut pool = FormeRuntimePool::default();

        pool.get_or_create(graph_a, forme)
            .workbench
            .open_tile(member);
        pool.get_or_create(graph_b, forme);

        assert!(pool.get(graph_a, forme).unwrap().workbench.has_tile(member));
        assert!(!pool.get(graph_b, forme).unwrap().workbench.has_tile(member));
    }
}
