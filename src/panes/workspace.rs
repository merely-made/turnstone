// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Outer-pane custody over the shared Workspace. Payloads stay host-owned.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use workbench::{
    ContentSource, Edge, Tile, TileId, TileTree, WorkbenchOutcome, Workspace, WorkspaceEvent,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneWorkspace<C> {
    pub workspace: Workspace,
    pub panes: BTreeMap<u64, C>,
}

impl<C: Clone> PaneWorkspace<C> {
    pub fn single(id: u64, title: String, payload: C) -> Self {
        Self {
            workspace: Workspace::new(TileTree::single(Self::tile(id, title))),
            panes: BTreeMap::from([(id, payload)]),
        }
    }

    pub fn tile(id: u64, title: String) -> Tile {
        Tile {
            id: TileId(id),
            title,
            content: ContentSource::Open {
                kind: "turnstone.pane".into(),
                id: id.to_string(),
            },
            accent: None,
        }
    }

    /// Reject duplicate stations and orphaned payloads before installing a load/edit.
    pub fn validate(&self) -> Result<(), String> {
        let tiles = self.workspace.tiles();
        let mut seen = HashSet::new();
        for tile in &tiles {
            if !seen.insert(tile.id) {
                return Err("Duplicate pane station".into());
            }
            if !self.panes.contains_key(&tile.id.0) {
                return Err("Pane has no host payload".into());
            }
            if tile.content
                != (ContentSource::Open {
                    kind: "turnstone.pane".into(),
                    id: tile.id.0.to_string(),
                })
            {
                return Err("Pane tile names a foreign content owner".into());
            }
        }
        if seen.len() != self.panes.len() {
            return Err("Unplaced pane payload".into());
        }
        validate_tree(self.workspace.tiled())
    }

    pub fn apply(&mut self, event: &WorkspaceEvent) -> Result<WorkbenchOutcome, String> {
        self.validate()?;
        let mut next = self.clone();
        let outcome = next.workspace.apply(event);
        next.panes
            .retain(|id, _| next.workspace.contains(TileId(*id)));
        next.validate()?;
        if outcome.changed() {
            *self = next;
        }
        Ok(outcome)
    }

    pub fn insert_beside(
        &mut self,
        target: u64,
        edge: Edge,
        id: u64,
        title: String,
        payload: C,
    ) -> bool {
        if self.workspace.contains(TileId(id)) {
            return false;
        }
        let mut next = self.clone();
        let tile = Self::tile(id, title);
        if next.workspace.tiles().is_empty() {
            *next.workspace.tiled_mut() = TileTree::single(tile);
        } else if !next
            .workspace
            .tiled_mut()
            .split_beside(TileId(target), edge, tile)
        {
            return false;
        }
        next.panes.insert(id, payload);
        if next.validate().is_err() {
            return false;
        }
        *self = next;
        true
    }

    /// Visible tiled bodies, independently of retained inactive-tab custody.
    pub fn active_ids(&self) -> Vec<TileId> {
        fn walk(tree: &TileTree, out: &mut Vec<TileId>) {
            match tree {
                TileTree::Stack(stack) => {
                    if let Some(tile) = stack.tabs.get(stack.active) {
                        out.push(tile.id);
                    }
                },
                TileTree::Split { children, .. } => {
                    for child in children {
                        walk(&child.tree, out);
                    }
                },
            }
        }
        let mut out = Vec::new();
        walk(self.workspace.tiled(), &mut out);
        out
    }

    pub fn take(&mut self, id: u64) -> Option<C> {
        self.workspace.take_tile(TileId(id))?;
        self.panes.remove(&id)
    }
}

fn validate_tree(tree: &TileTree) -> Result<(), String> {
    match tree {
        TileTree::Stack(stack) => {
            if !stack.tabs.is_empty() && stack.active >= stack.tabs.len() {
                return Err("Active pane tab is out of range".into());
            }
        },
        TileTree::Split { children, .. } => {
            if children.is_empty()
                || children
                    .iter()
                    .any(|child| !child.fraction.is_finite() || child.fraction <= 0.0)
            {
                return Err("Invalid pane split shares".into());
            }
            for child in children {
                validate_tree(&child.tree)?;
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use workbench::{DropTarget, TileEvent, TilePath, WorkbenchEffect};

    fn two() -> PaneWorkspace<String> {
        let mut frame = PaneWorkspace::single(1, "Graph".into(), "graph".into());
        assert!(frame.insert_beside(1, Edge::Right, 2, "Inspector".into(), "inspector".into()));
        frame
    }

    #[test]
    fn stacking_switching_closing_and_restart_preserve_payload_identity() {
        let mut frame = two();
        frame
            .apply(
                &TileEvent::Dragged {
                    tile: TileId(2),
                    to: DropTarget::Stack {
                        stack: TilePath(vec![0]),
                        index: 1,
                    },
                }
                .into(),
            )
            .unwrap();
        assert_eq!(frame.workspace.tiled().tiles().len(), 2);
        assert_eq!(frame.active_ids(), vec![TileId(2)]);
        frame
            .apply(&TileEvent::Activated(TileId(1)).into())
            .unwrap();
        assert_eq!(frame.active_ids(), vec![TileId(1)]);
        let text = serde_json::to_string(&frame).unwrap();
        let mut restored: PaneWorkspace<String> = serde_json::from_str(&text).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, frame);
        restored
            .apply(&TileEvent::Closed(TileId(2)).into())
            .unwrap();
        assert_eq!(restored.panes, BTreeMap::from([(1, "graph".into())]));
    }

    #[test]
    fn outside_drop_retains_custody_until_host_accepts() {
        let mut frame = two();
        let before = frame.clone();
        let outcome = frame
            .apply(
                &TileEvent::Dragged {
                    tile: TileId(2),
                    to: DropTarget::Outside,
                }
                .into(),
            )
            .unwrap();
        assert_eq!(
            outcome.effect(),
            Some(WorkbenchEffect::TearOut { tile: TileId(2) })
        );
        assert_eq!(frame, before);
        assert_eq!(frame.take(2), Some("inspector".into()));
        frame.validate().unwrap();
    }

    #[test]
    fn malformed_or_orphaned_payloads_refuse_without_mutation() {
        let mut frame = two();
        frame.panes.remove(&1);
        let before = frame.clone();
        assert!(frame.apply(&TileEvent::Closed(TileId(2)).into()).is_err());
        assert_eq!(frame, before);
    }

    #[test]
    fn duplicate_ids_and_missing_drop_targets_do_not_orphan_a_pane() {
        let mut frame = two();
        let before = frame.clone();
        assert!(!frame.insert_beside(1, Edge::Bottom, 2, "Duplicate".into(), "duplicate".into()));
        assert!(!frame.insert_beside(999, Edge::Bottom, 3, "Missing".into(), "missing".into()));
        assert_eq!(frame, before);
    }
    #[test]
    fn invalid_stack_drop_does_not_remove_source_custody() {
        let mut frame = two();
        let before = frame.clone();
        frame
            .apply(
                &TileEvent::Dragged {
                    tile: TileId(2),
                    to: DropTarget::Stack {
                        stack: TilePath(vec![99]),
                        index: 0,
                    },
                }
                .into(),
            )
            .unwrap();
        assert_eq!(frame, before);
    }
    #[test]
    fn non_finite_split_edit_is_refused_atomically() {
        let mut frame = two();
        let before = frame.clone();
        let _ = frame.apply(
            &TileEvent::DividerMoved {
                split: TilePath(vec![]),
                fractions: vec![f32::NAN, 1.0],
            }
            .into(),
        );
        assert_eq!(frame, before);
    }
    #[test]
    fn duplicate_station_is_rejected_before_a_close() {
        let mut frame = two();
        let duplicate = frame.workspace.tiled().clone();
        *frame.workspace.tiled_mut() = TileTree::Split {
            axis: workbench::SplitAxis::Column,
            children: vec![
                workbench::TileBranch::new(0.5, duplicate.clone()),
                workbench::TileBranch::new(0.5, duplicate),
            ],
        };
        let before = frame.clone();
        assert!(frame.apply(&TileEvent::Closed(TileId(1)).into()).is_err());
        assert_eq!(frame, before);
    }
    #[test]
    fn shared_float_and_dock_keep_the_same_host_payload() {
        let mut frame = two();
        frame
            .apply(
                &workbench::FloatEvent::Float {
                    tile: TileId(2),
                    rect: workbench::RelativeRect {
                        x: 0.1,
                        y: 0.2,
                        width: 0.4,
                        height: 0.5,
                    },
                }
                .into(),
            )
            .unwrap();
        assert_eq!(frame.workspace.floating().len(), 1);
        assert_eq!(frame.panes.get(&2).unwrap(), "inspector");
        frame
            .apply(
                &workbench::FloatEvent::Dock {
                    tile: TileId(2),
                    target: workbench::FloatDockTarget::Tab { target: TileId(1) },
                }
                .into(),
            )
            .unwrap();
        assert!(frame.workspace.floating().is_empty());
        assert_eq!(frame.panes.len(), 2);
        frame.validate().unwrap();
    }
}
