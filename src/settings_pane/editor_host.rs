// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{
    arrangement::{ArrangementOperation, DeletionApproval, PreparedArrangement},
    editor::{ConfirmationDecision, ConfirmationTarget, EditConfirmation, EditorIntent},
    *,
};
use cambium::composition::{ComponentKey, ContainerKey, ContainerKind, PlacementNode};
use genet_scripted_dom::NodeId;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::collections::BTreeSet;

pub(super) enum HistoryCommit {
    Edit,
    Undo,
    Redo,
}

impl SettingsPane {
    /// Call after native/scenario input dispatch. View callbacks only queue
    /// structural intents; route preparation must run outside the view walk.
    pub(super) fn flush_editor_commands(&mut self) {
        let intents = self
            .runner
            .update_deferred(|state| state.editor.take_intents().collect::<Vec<_>>());
        if intents.is_empty() {
            return;
        }
        for intent in intents {
            let result = match intent {
                EditorIntent::ToggleEditor => {
                    self.runner
                        .update(|state| state.editor.enabled = !state.editor.enabled);
                    Ok(())
                },
                EditorIntent::Structure(mut operation) => {
                    if let ArrangementOperation::Rebind {
                        key,
                        draft_clean,
                        no_pending_write,
                        ..
                    } = &mut operation
                    {
                        match self.runner.state().rows.get(key) {
                            Some(row) => {
                                let snapshot = row.session.snapshot();
                                *draft_clean = !snapshot.dirty;
                                *no_pending_write = snapshot.pending_generation.is_none();
                            },
                            None => {
                                // A restored unavailable descriptor may never
                                // have realized an editor. It has no draft or
                                // pending write to settle; the arrangement
                                // validator still admits the target binding.
                                *draft_clean = true;
                                *no_pending_write = true;
                            },
                        }
                        if !*draft_clean || !*no_pending_write {
                            self.notice("Apply or discard the edit before rebinding this row.");
                            continue;
                        }
                    }
                    let prepared = self
                        .runner
                        .state()
                        .arrangement
                        .preview(operation)
                        .map_err(|error| format!("{error:?}"));
                    prepared
                        .and_then(|prepared| self.commit_prepared(prepared, HistoryCommit::Edit))
                },
                EditorIntent::RequestDelete(key) => {
                    self.runner.update(|state| {
                        state.editor.confirmation = Some(EditConfirmation {
                            target: ConfirmationTarget::Delete(key),
                            base_revision: state.arrangement.revision(),
                        })
                    });
                    Ok(())
                },
                EditorIntent::ConfirmDelete { decision } => self.resolve_confirmation(decision),
                EditorIntent::Undo => {
                    let prepared = self
                        .runner
                        .state()
                        .arrangement
                        .preview_undo()
                        .map_err(|error| format!("{error:?}"));
                    prepared
                        .and_then(|prepared| self.commit_prepared(prepared, HistoryCommit::Undo))
                },
                EditorIntent::Redo => {
                    let prepared = self
                        .runner
                        .state()
                        .arrangement
                        .preview_redo()
                        .map_err(|error| format!("{error:?}"));
                    prepared
                        .and_then(|prepared| self.commit_prepared(prepared, HistoryCommit::Redo))
                },
                EditorIntent::ToggleContainer(container) => self.toggle_container(container),
                EditorIntent::Save => self.save_arrangement(),
                EditorIntent::ReloadSaved => {
                    let dirty = self.runner.state().rows.values().any(|row| {
                        let s = row.session.snapshot();
                        s.dirty || s.pending_generation.is_some()
                    });
                    if dirty || self.runner.state().editor.unsaved {
                        self.runner.update(|state| {
                            state.editor.confirmation = Some(EditConfirmation {
                                target: ConfirmationTarget::ReloadSaved,
                                base_revision: state.arrangement.revision(),
                            })
                        });
                        Ok(())
                    } else {
                        self.reload_saved_arrangement()
                    }
                },
            };
            if let Err(reason) = result {
                self.notice(&reason);
            }
        }
        self.runner.update(|_| {});
    }

    fn notice(&mut self, reason: &str) {
        self.runner.update(|state| {
            state.editor.notice = reason.into();
            state.status = reason.into();
        });
    }

    fn resolve_confirmation(&mut self, decision: ConfirmationDecision) -> Result<(), String> {
        let Some(confirmation) = self.runner.state().editor.confirmation.clone() else {
            return Ok(());
        };
        if decision == ConfirmationDecision::Cancel {
            self.runner.update(|state| state.editor.confirmation = None);
            return Ok(());
        }
        if confirmation.base_revision != self.runner.state().arrangement.revision() {
            self.runner.update(|state| state.editor.confirmation = None);
            return Err("The arrangement changed. Review the operation again.".into());
        }
        let keys = match &confirmation.target {
            ConfirmationTarget::Delete(key) => vec![key.clone()],
            ConfirmationTarget::ReloadSaved => self.runner.state().rows.keys().cloned().collect(),
        };
        self.prepare_keys(&keys.iter().cloned().collect(), true)?;
        if keys.iter().any(|key| {
            self.runner
                .state()
                .rows
                .get(key)
                .is_some_and(|row| row.session.snapshot().pending_generation.is_some())
        }) {
            return Err("Wait for every pending Apply before continuing. No edits were applied or discarded.".into());
        }
        for key in &keys {
            let Some(row) = self.runner.state().rows.get(key) else {
                continue;
            };
            let snapshot = row.session.snapshot();
            if snapshot.dirty {
                match decision {
                    ConfirmationDecision::Apply => {
                        self.runner.update(|state| {
                            row_commands::apply_instance(
                                state,
                                key,
                                snapshot.draft_value,
                                snapshot.edit_generation,
                            )
                        });
                    },
                    ConfirmationDecision::Discard => {
                        self.runner
                            .update(|state| row_commands::discard_instance(state, key));
                    },
                    ConfirmationDecision::Cancel => unreachable!(),
                }
            }
            if self.runner.state().rows.get(key).is_some_and(|row| {
                let s = row.session.snapshot();
                s.dirty || s.pending_generation.is_some()
            }) {
                return Err("An edit could not be resolved. The arrangement was kept; any earlier successful Apply or Discard remains in effect.".into());
            }
        }
        match confirmation.target {
            ConfirmationTarget::Delete(key) => {
                let approval = DeletionApproval::after_confirmation(key.clone(), true, true, true)
                    .map_err(|error| format!("{error:?}"))?;
                let prepared = self
                    .runner
                    .state()
                    .arrangement
                    .preview(ArrangementOperation::Delete { key, approval })
                    .map_err(|error| format!("{error:?}"))?;
                self.commit_prepared(prepared, HistoryCommit::Edit)?;
            },
            ConfirmationTarget::ReloadSaved => self.reload_saved_arrangement()?,
        }
        self.runner.update(|state| state.editor.confirmation = None);
        Ok(())
    }

    pub(super) fn commit_prepared(
        &mut self,
        prepared: PreparedArrangement,
        mode: HistoryCommit,
    ) -> Result<(), String> {
        let effect = prepared.effect().clone();
        let retiring = effect
            .removed
            .union(&effect.rebound)
            .cloned()
            .collect::<BTreeSet<_>>();
        if retiring.iter().any(|key| {
            self.runner.state().rows.get(key).is_some_and(|row| {
                let s = row.session.snapshot();
                s.dirty || s.pending_generation.is_some()
            })
        }) {
            return Err("Apply or discard edits before removing or rebinding these rows.".into());
        }
        self.prepare_keys(&effect.prepare_routes, false)?;
        let mut withdraw = retiring.clone();
        withdraw.extend(hidden_keys(
            &prepared.candidate().placement,
            &self.runner.state().collapsed,
            false,
        ));
        withdraw.extend(prepared.candidate().unplaced.iter().cloned());
        self.prepare_keys(&withdraw, true)?;
        // IME preparation may itself turn a clean row into a dirty row.
        if retiring.iter().any(|key| {
            self.runner.state().rows.get(key).is_some_and(|row| {
                let s = row.session.snapshot();
                s.dirty || s.pending_generation.is_some()
            })
        }) {
            return Err(
                "Move preparation committed an edit. Apply or discard it before continuing.".into(),
            );
        }
        let mut result = Ok(());
        self.runner.update(|state| {
            let committed = match mode {
                HistoryCommit::Edit => state.arrangement.commit(prepared),
                HistoryCommit::Undo => state.arrangement.commit_undo(prepared),
                HistoryCommit::Redo => state.arrangement.commit_redo(prepared),
            };
            match committed {
                Ok(_) => {
                    let valid = container_keys(&state.arrangement.accepted().placement);
                    state.collapsed.retain(|key| valid.contains(key));
                    source_sync::reconcile_rows(state, &Default::default());
                    state.editor.unsaved = true;
                    state.editor.notice =
                        "Arrangement changed; setting drafts were retained.".into();
                },
                Err(error) => result = Err(format!("{error:?}")),
            }
        });
        self.recompute_unsaved();
        result
    }

    pub(super) fn prepare_keys(
        &mut self,
        keys: &BTreeSet<ComponentKey>,
        retiring: bool,
    ) -> Result<(), String> {
        let nodes = keys
            .iter()
            .filter_map(|key| self.component_node(key))
            .collect::<Vec<_>>();
        for node in nodes {
            let _: Vec<()> = self.runner.prepare_subtree_move(node);
            if self.runner.subtree_move_has_active_interaction(node) {
                return Err(
                    "The active text composition could not be committed. The arrangement was kept."
                        .into(),
                );
            }
            if retiring {
                let focused = self.runner.focus().is_some_and(|focus| {
                    let dom = self.dom.borrow();
                    let mut current = Some(focus);
                    while let Some(candidate) = current {
                        if candidate == node {
                            return true;
                        }
                        current = dom.parent(candidate);
                    }
                    false
                });
                if focused {
                    self.runner.set_focus(None);
                }
            }
        }
        Ok(())
    }

    pub(super) fn component_node(&self, key: &ComponentKey) -> Option<NodeId> {
        let dom = self.dom.borrow();
        fn find(dom: &ScriptedDom, node: NodeId, key: &str) -> Option<NodeId> {
            if dom.attribute(
                node,
                &Namespace::default(),
                &LocalName::from("data-setting-instance"),
            ) == Some(key)
            {
                return Some(node);
            }
            dom.dom_children(node)
                .find_map(|child| find(dom, child, key))
        }
        find(&dom, self.runner.root(), &key.0)
    }

    fn toggle_container(&mut self, container: ContainerKey) -> Result<(), String> {
        let keys = keys_in_container(
            &self.runner.state().arrangement.accepted().placement,
            &container,
        );
        let closing = !self.runner.state().collapsed.contains(&container);
        self.prepare_keys(&keys, closing)?;
        self.runner.update(|state| {
            if !state.collapsed.remove(&container) {
                state.collapsed.insert(container);
            }
            state.editor.unsaved = true;
        });
        self.recompute_unsaved();
        Ok(())
    }
}

pub(super) fn container_keys(nodes: &[PlacementNode]) -> BTreeSet<ContainerKey> {
    let mut keys = BTreeSet::new();
    for node in nodes {
        let (container, children) = match node {
            PlacementNode::Component { .. } => continue,
            PlacementNode::Group { key, children } => (
                ContainerKey {
                    kind: ContainerKind::Group,
                    key: key.0.clone(),
                },
                children,
            ),
            PlacementNode::Drawer { key, children } => (
                ContainerKey {
                    kind: ContainerKind::Drawer,
                    key: key.0.clone(),
                },
                children,
            ),
        };
        keys.insert(container);
        keys.extend(container_keys(children));
    }
    keys
}

fn keys_in_container(nodes: &[PlacementNode], container: &ContainerKey) -> BTreeSet<ComponentKey> {
    fn all(nodes: &[PlacementNode], out: &mut BTreeSet<ComponentKey>) {
        for node in nodes {
            match node {
                PlacementNode::Component { key } => {
                    out.insert(key.clone());
                },
                PlacementNode::Group { children, .. } | PlacementNode::Drawer { children, .. } => {
                    all(children, out)
                },
            }
        }
    }
    let mut result = BTreeSet::new();
    for node in nodes {
        let (key, children) = match node {
            PlacementNode::Component { .. } => continue,
            PlacementNode::Group { key, children } => (
                ContainerKey {
                    kind: ContainerKind::Group,
                    key: key.0.clone(),
                },
                children,
            ),
            PlacementNode::Drawer { key, children } => (
                ContainerKey {
                    kind: ContainerKind::Drawer,
                    key: key.0.clone(),
                },
                children,
            ),
        };
        if &key == container {
            all(children, &mut result);
        } else {
            result.extend(keys_in_container(children, container));
        }
    }
    result
}

fn hidden_keys(
    nodes: &[PlacementNode],
    collapsed: &BTreeSet<ContainerKey>,
    inherited: bool,
) -> BTreeSet<ComponentKey> {
    let mut result = BTreeSet::new();
    for node in nodes {
        match node {
            PlacementNode::Component { key } => {
                if inherited {
                    result.insert(key.clone());
                }
            },
            PlacementNode::Group { key, children } => result.extend(hidden_keys(
                children,
                collapsed,
                inherited
                    || collapsed.contains(&ContainerKey {
                        kind: ContainerKind::Group,
                        key: key.0.clone(),
                    }),
            )),
            PlacementNode::Drawer { key, children } => result.extend(hidden_keys(
                children,
                collapsed,
                inherited
                    || collapsed.contains(&ContainerKey {
                        kind: ContainerKind::Drawer,
                        key: key.0.clone(),
                    }),
            )),
        }
    }
    result
}
