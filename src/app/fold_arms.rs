// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Resolve the actual graph pane before touching a runtime. Chrome's
    /// command palette keeps the active pane; initial chrome uses the primary
    /// graph pane. An active non-graph pane cannot retarget this command.
    pub(crate) fn fold_owner_pane(&self) -> Option<PaneId> {
        let pane = self
            .focused_graph_pane()
            .or(self.active_pane)
            .unwrap_or_else(|| self.default_graph_pane());
        (self.pane_content(pane) == Some(&PaneContent::Orrery)).then_some(pane)
    }

    pub(super) fn edit_graph_folds(&mut self, action: Action) -> Vec<Effect> {
        let Some(pane) = self.fold_owner_pane() else {
            return vec![];
        };
        let Some(graph) = self.graph_for_pane(pane) else {
            return vec![];
        };
        let Some(runtime) = self.graph_runtimes.get(graph) else {
            return vec![];
        };
        let writable = runtime.session.is_some_and(|session| {
            let dir = crate::session::session_dir(&self.data_root, session);
            runtime.persistence.placement_for(&dir).is_ok()
        });
        if runtime.folds.read_only || !writable {
            self.note(crate::observe::AppEvent::InteractionMissed {
                what: "graph-fold",
                target: "saved fold sidecar or session write authority refused; preserved intact"
                    .into(),
            });
            return vec![];
        }
        let changed = self
            .with_graph_pane(pane, |canvas| match action {
                Action::FoldSelection => canvas
                    .fold_selected(crate::graph_folds::scope(graph))
                    .is_some(),
                Action::ExpandFold => canvas.expand_selected_fold(),
                Action::ExpandAllFolds => {
                    let ids: Vec<_> = canvas.fold_records().iter().map(|r| r.id.clone()).collect();
                    ids.iter()
                        .fold(false, |changed, id| canvas.expand_fold(id) || changed)
                },
                Action::UndoFold => canvas.undo_fold(),
                Action::RedoFold => canvas.redo_fold(),
                _ => false,
            })
            .unwrap_or(false);
        if changed {
            vec![Effect::SaveGraphFolds { graph }, Effect::Redraw]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = App::test_stub();
        let pane = app.default_graph_pane();
        app.with_graph_pane(pane, |canvas| {
            canvas.visit("mere://fold-a");
            canvas.visit("mere://fold-b");
            let members: Vec<_> = canvas.graph().nodes().map(|(_, n)| n.id).collect();
            canvas.set_selected_members(&members);
            canvas.set_physics_paused(true);
        });
        app.focus = FocusTarget::Graph(pane);
        app
    }
    #[test]
    fn product_commands_retain_graph_positions_and_have_reversible_history() {
        let mut app = fixture();
        let truth = serde_json::to_value(app.graph_runtimes.graph().to_snapshot_at(0)).unwrap();
        let pane = app.default_graph_pane();
        let graph = app.graph_for_pane(pane).unwrap();
        let positions: Vec<_> = app
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(k, _)| app.graph_runtimes.world_position_of(k))
            .collect();
        let effects = app.update(Action::FoldSelection);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::SaveGraphFolds { graph: owner } if *owner == graph))
        );
        let records = app.graph_runtimes.fold_records().to_vec();
        assert_eq!(records.len(), 1);
        app.update(Action::UndoFold);
        assert!(app.graph_runtimes.fold_records().is_empty());
        app.update(Action::RedoFold);
        assert_eq!(app.graph_runtimes.fold_records(), records);
        app.update(Action::ExpandAllFolds);
        assert!(app.graph_runtimes.fold_records().is_empty());
        app.update(Action::UndoFold);
        assert_eq!(app.graph_runtimes.fold_records(), records);
        let after = serde_json::to_value(app.graph_runtimes.graph().to_snapshot_at(0)).unwrap();
        assert_eq!(truth, after);
        assert_eq!(
            positions,
            app.graph_runtimes
                .graph()
                .nodes()
                .map(|(k, _)| app.graph_runtimes.world_position_of(k))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn unsupported_saved_state_refuses_edit_and_reports_it() {
        let mut app = fixture();
        let graph = app.graph_runtimes.active_graph();
        app.graph_runtimes.get_mut(graph).unwrap().folds.read_only = true;
        assert!(app.update(Action::FoldSelection).is_empty());
        assert!(app.graph_runtimes.fold_records().is_empty());
        assert!(app.take_events().iter().any(|e| matches!(
            e,
            AppEvent::InteractionMissed {
                what: "graph-fold",
                ..
            }
        )));
    }
    #[test]
    fn commands_resolve_pane_graph_even_when_compatibility_cursor_differs() {
        let mut app = fixture();
        let owner = app.graph_for_pane(app.default_graph_pane()).unwrap();
        let other = GraphId::from_uuid(uuid::Uuid::new_v4());
        app.graph_runtimes
            .activate_or_insert(other, None, mere::canvas::Canvas::new());
        let effects = app.update(Action::FoldSelection);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e,Effect::SaveGraphFolds{graph} if *graph==owner))
        );
        assert_eq!(
            app.graph_runtimes
                .canvas(owner)
                .unwrap()
                .fold_records()
                .len(),
            1
        );
        assert!(
            app.graph_runtimes
                .canvas(other)
                .unwrap()
                .fold_records()
                .is_empty()
        );
    }
    #[test]
    fn a_single_member_and_unbound_session_cannot_create_a_durable_fold() {
        let mut app = fixture();
        let pane = app.default_graph_pane();
        app.with_graph_pane(pane, |c| {
            c.clear_selection();
            c.select_by_url("mere://fold-a");
        });
        assert!(app.update(Action::FoldSelection).is_empty());
        assert!(app.graph_runtimes.fold_records().is_empty());
        app.with_graph_pane(pane, |c| {
            let members: Vec<_> = c.graph().nodes().map(|(_, n)| n.id).collect();
            c.set_selected_members(&members);
        });
        let graph = app.graph_for_pane(pane).unwrap();
        app.graph_runtimes.get_mut(graph).unwrap().persistence =
            crate::session_persistence::SessionPersistence::Unbound;
        assert!(app.update(Action::FoldSelection).is_empty());
        assert!(app.graph_runtimes.fold_records().is_empty());
    }
}
