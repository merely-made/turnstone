// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl GraphRuntime {
    pub(crate) fn origin(&self) -> Option<crate::host_journal::RuntimeOrigin> {
        self.origin
    }

    pub fn new(graph: GraphId, session: Option<SessionId>, mut canvas: Canvas) -> Self {
        canvas.set_graph_grammar(true);
        Self {
            instance: uuid::Uuid::new_v4(),
            graph,
            session,
            canvas,
            scene_transition: None,
            physics_status: None,
            scene_save_pending: false,
            scene_restore_pending: false,
            view: Default::default(),
            origin: None,
            persistence: crate::session_persistence::SessionPersistence::Unbound,
        }
    }
}
