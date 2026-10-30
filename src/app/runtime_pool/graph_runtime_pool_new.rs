// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
        };
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
        canvas.set_graph_grammar(true);
        runtime.scene_transition = None;
        runtime.scene_save_pending = false;
        runtime.scene_restore_pending = false;
        runtime.canvas = canvas;
        runtime.instance = uuid::Uuid::new_v4();
        runtime.origin = None;
        let _ = self.bind_recording(self.active);
    }

    pub(crate) fn active_persistence(
        &self,
    ) -> &crate::session_persistence::SessionPersistence<pandect::graph_placement::PlacementProfile>
    {
        &self
            .runtimes
            .get(&self.active)
            .expect("active runtime")
            .persistence
    }

    pub(crate) fn adopt_graph(
        &mut self,
        graph: mere::kernel::graph::Graph,
        state: crate::session_persistence::SessionPersistence<
            pandect::graph_placement::PlacementProfile,
        >,
    ) {
        let runtime = self.runtimes.get_mut(&self.active).expect("active runtime");
        runtime.scene_transition = None;
        runtime.scene_save_pending = false;
        runtime.scene_restore_pending = false;
        runtime.canvas.set_graph(graph);
        runtime.instance = uuid::Uuid::new_v4();
        runtime.persistence = state;
        runtime.origin = None;
        let _ = self.bind_recording(self.active);
    }

    /// Unqualified graph replacement revokes any prior placement. It cannot
    /// unlock a refused destination. Adoption carries graph and state together.
    pub fn set_graph(&mut self, graph: mere::kernel::graph::Graph) {
        let runtime = self.runtimes.get_mut(&self.active).expect("active runtime");
        runtime.persistence.clear_placement();
        runtime.scene_transition = None;
        runtime.scene_save_pending = false;
        runtime.scene_restore_pending = false;
        runtime.canvas.set_graph(graph);
        runtime.instance = uuid::Uuid::new_v4();
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

    pub(crate) fn pending_scene_graphs(&self) -> Vec<GraphId> {
        self.runtimes
            .iter()
            .filter_map(|(id, r)| r.scene_save_pending.then_some(*id))
            .collect()
    }

    pub fn contains(&self, graph: GraphId) -> bool {
        self.runtimes.contains_key(&graph)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &GraphRuntime> {
        self.runtimes.values()
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

    /// Edit the active graph's runtime facets through validated graph deltas.
    pub(crate) fn edit_facets<R>(
        &mut self,
        edit: impl FnOnce(&mut mere::kernel::graph::NodeFacetStore) -> R,
    ) -> Result<R, String> {
        let active = self.active;
        let runtime = self
            .get_mut(active)
            .ok_or("active graph runtime is absent")?;
        crate::graph_facets::edit_canvas_facets(&mut runtime.canvas, edit)
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
        self.runtimes
            .values()
            .flat_map(|runtime| runtime.canvas.graph().nodes().map(|(_, node)| node.id))
            .collect()
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
