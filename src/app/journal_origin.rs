// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::App;
use crate::host_journal::RuntimeOrigin;
use crate::observe::AppEvent;

impl App {
    /// Called only at an accepted session/fixture baseline, after quiet setup.
    pub(crate) fn bind_behavior_journal(&mut self) -> Result<(), String> {
        self.behavior_binding = None;
        let graph = self.graph_runtimes.active_graph();
        self.graph_runtimes.suspend_recording(graph);
        if self.session_load_refused() {
            return Err("refused session cannot bind behavior capture".into());
        }
        let runtime = self
            .graph_runtimes
            .get(graph)
            .ok_or("behavior graph is absent")?;
        if runtime.session != Some(self.session_id) {
            return Err("behavior graph belongs to another session".into());
        }
        let floor = self
            .watches
            .watches()
            .iter()
            .map(|watch| watch.cursor)
            .max()
            .unwrap_or(0);
        let boundary = {
            let mut journal = self
                .journal
                .lock()
                .map_err(|_| "behavior journal capture lock poisoned")?;
            journal
                .raise_cursor_floor(floor)
                .map_err(|error| error.to_string())?;
            journal.high_water()
        };
        let origin = self.graph_runtimes.bind_recording(graph)?;
        self.behavior_binding = Some(origin);
        self.behavior_cursor = boundary;
        self.behavior_refusal = None;
        Ok(())
    }

    /// Resolve the loaded authority context independently of pane focus/cursor.
    pub(crate) fn behavior_origin(&self) -> Option<RuntimeOrigin> {
        let origin = self.behavior_binding?;
        if origin.session != Some(self.session_id) {
            return None;
        }
        let runtime = self.graph_runtimes.get(origin.graph)?;
        if !origin.is_current(self.session_id, runtime.origin())
            || runtime.session != origin.session
            || !runtime.canvas.graph().is_recording()
            || matches!(
                &runtime.persistence,
                crate::session_persistence::SessionPersistence::Refused { .. }
            )
        {
            return None;
        }
        let journal = self.journal.lock().ok()?;
        if journal.error().is_some() {
            return None;
        }
        Some(origin)
    }

    pub(crate) fn behavior_execution_origin(&self) -> Option<RuntimeOrigin> {
        let origin = self.behavior_origin()?;
        if self.journal.lock().ok()?.execution_error().is_some() {
            return None;
        }
        (self.graph_runtimes.active_graph() == origin.graph).then_some(origin)
    }

    pub(crate) fn refuse_behavior(&mut self, error: String) {
        if self.behavior_refusal.as_ref() != Some(&error) {
            self.behavior_refusal = Some(error.clone());
            self.events.push(AppEvent::DenizenRefused(error));
        }
    }
}
