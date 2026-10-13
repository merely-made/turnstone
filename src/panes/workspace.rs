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
    /// Optional host-owned descriptors travel with the payload, never its station.
    #[serde(default)]
    pub descriptors: BTreeMap<u64, serde_json::Value>,
}

impl<C: Clone> PaneWorkspace<C> {
    pub fn single(id: u64, title: String, payload: C) -> Self {
        Self {
            workspace: Workspace::new(TileTree::single(Self::tile(id, title))),
            panes: BTreeMap::from([(id, payload)]),
            descriptors: BTreeMap::new(),
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
        if self
            .descriptors
            .keys()
            .any(|id| !self.panes.contains_key(id))
        {
            return Err("Unplaced pane descriptor".into());
        }
        for float in self.workspace.floating() {
            let r = float.rect;
            let c = float.constraints;
            if ![r.x, r.y, r.width, r.height, c.min_width, c.min_height]
                .into_iter()
                .all(f32::is_finite)
                || r.width <= 0.0
                || r.height <= 0.0
                || c.min_width < 0.0
                || c.min_height < 0.0
                || c.max_width
                    .is_some_and(|v| !v.is_finite() || v < c.min_width)
                || c.max_height
                    .is_some_and(|v| !v.is_finite() || v < c.min_height)
            {
                return Err("Invalid floating pane geometry".into());
            }
        }
        validate_tree(self.workspace.tiled())
    }

    pub fn apply(&mut self, event: &WorkspaceEvent) -> Result<WorkbenchOutcome, String> {
        self.validate()?;
        let mut next = self.clone();
        let outcome = next.workspace.apply(event);
        next.panes
            .retain(|id, _| next.workspace.contains(TileId(*id)));
        next.descriptors.retain(|id, _| next.panes.contains_key(id));
        next.validate()?;
        if outcome.changed() {
            if next.workspace == self.workspace {
                return Ok(WorkbenchOutcome::Unchanged);
            }
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
        if next.workspace.tiled().is_empty() {
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

    /// Visible bodies, independently of retained inactive-tab custody.
    pub fn active_ids(&self) -> Vec<TileId> {
        let mut out = self.active_tiled_ids();
        out.extend(
            self.workspace
                .visible_floating(true)
                .iter()
                .map(|f| f.tile.id),
        );
        out
    }

    pub fn active_tiled_ids(&self) -> Vec<TileId> {
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
        self.descriptors.remove(&id);
        self.panes.remove(&id)
    }

    /// Move one station and all its host custody atomically between windows.
    /// Floats retain geometry and policy; the destination assigns local z.
    pub fn transfer_to(
        &mut self,
        destination: &mut Self,
        id: u64,
        target: Option<u64>,
    ) -> Result<(), String> {
        self.validate()?;
        destination.validate()?;
        if destination.workspace.contains(TileId(id)) {
            return Err("Destination already owns pane".into());
        }
        let mut source = self.clone();
        let mut next = destination.clone();
        let payload = source.panes.remove(&id).ok_or("Source pane is absent")?;
        let descriptor = source.descriptors.remove(&id);
        if let Some(float) = source.workspace.take_floating(TileId(id)) {
            if !next.workspace.insert_floating(float) {
                return Err("Float transfer refused".into());
            }
        } else {
            let tile = source
                .workspace
                .take_tile(TileId(id))
                .ok_or("Source station is absent")?;
            if next.workspace.tiled().is_empty() {
                *next.workspace.tiled_mut() = TileTree::single(tile);
            } else if !target.is_some_and(|target| {
                next.workspace
                    .tiled_mut()
                    .split_beside(TileId(target), Edge::Right, tile)
            }) {
                return Err("Destination tiled anchor is absent".into());
            }
        }
        next.panes.insert(id, payload);
        if let Some(descriptor) = descriptor {
            next.descriptors.insert(id, descriptor);
        }
        source.validate()?;
        next.validate()?;
        *self = source;
        *destination = next;
        Ok(())
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
    #[test]
    fn float_transfer_and_reopen_preserve_payload_descriptor_geometry_and_policy() {
        let mut source = two();
        source.descriptors.insert(
            2,
            serde_json::json!({"source":"same address, this access","draft":"retained"}),
        );
        source
            .apply(
                &workbench::FloatEvent::Float {
                    tile: TileId(2),
                    rect: workbench::RelativeRect {
                        x: 0.12,
                        y: 0.23,
                        width: 0.45,
                        height: 0.56,
                    },
                }
                .into(),
            )
            .unwrap();
        source
            .apply(
                &workbench::FloatEvent::SetConstraints {
                    tile: TileId(2),
                    constraints: workbench::FloatSizeConstraints {
                        min_width: 220.0,
                        min_height: 90.0,
                        max_width: Some(800.0),
                        max_height: None,
                    },
                }
                .into(),
            )
            .unwrap();
        source
            .apply(
                &workbench::FloatEvent::SetPinned {
                    tile: TileId(2),
                    pinned: true,
                }
                .into(),
            )
            .unwrap();
        let station = source.workspace.floating()[0].clone();
        let mut lens = PaneWorkspace::single(9, "Lens".into(), "other".into());
        source.transfer_to(&mut lens, 2, Some(9)).unwrap();
        assert_eq!(source.panes.len(), 1);
        assert!(source.descriptors.is_empty());
        let moved = &lens.workspace.floating()[0];
        assert_eq!(moved.tile, station.tile);
        assert_eq!(moved.rect, station.rect);
        assert_eq!(moved.constraints, station.constraints);
        assert!(moved.pinned);
        let mut reopened: PaneWorkspace<String> =
            serde_json::from_str(&serde_json::to_string(&lens).unwrap()).unwrap();
        reopened.validate().unwrap();
        reopened.transfer_to(&mut source, 2, Some(1)).unwrap();
        source
            .apply(
                &workbench::FloatEvent::Dock {
                    tile: TileId(2),
                    target: workbench::FloatDockTarget::Tab { target: TileId(1) },
                }
                .into(),
            )
            .unwrap();
        assert_eq!(source.active_ids(), vec![TileId(2)]);
        assert_eq!(source.panes[&2], "inspector");
        assert_eq!(source.descriptors[&2]["draft"], "retained");
        assert_eq!(reopened.panes.len(), 1);
    }
    #[test]
    fn failed_cross_window_transfer_changes_neither_custody() {
        let mut source = two();
        let mut destination = PaneWorkspace::single(9, "Lens".into(), "other".into());
        let before = (source.clone(), destination.clone());
        assert!(source.transfer_to(&mut destination, 2, Some(999)).is_err());
        assert_eq!((source.clone(), destination.clone()), before);
        assert!(source.transfer_to(&mut destination, 999, Some(9)).is_err());
        assert_eq!((source, destination), before);
    }
    #[test]
    fn malformed_float_geometry_is_refused_before_publish() {
        let mut frame = two();
        let before = frame.clone();
        assert!(
            frame
                .apply(
                    &workbench::FloatEvent::Float {
                        tile: TileId(2),
                        rect: workbench::RelativeRect {
                            x: 0.0,
                            y: f32::NAN,
                            width: 0.5,
                            height: 0.5
                        }
                    }
                    .into()
                )
                .is_err()
        );
        assert_eq!(frame, before);
        frame
            .apply(
                &workbench::FloatEvent::Float {
                    tile: TileId(2),
                    rect: workbench::RelativeRect {
                        x: 0.0,
                        y: 0.0,
                        width: 0.5,
                        height: 0.5,
                    },
                }
                .into(),
            )
            .unwrap();
        let before = frame.clone();
        assert!(
            frame
                .apply(
                    &workbench::FloatEvent::SetConstraints {
                        tile: TileId(2),
                        constraints: workbench::FloatSizeConstraints {
                            min_width: 200.0,
                            min_height: 0.0,
                            max_width: Some(100.0),
                            max_height: None
                        }
                    }
                    .into()
                )
                .is_err()
        );
        assert_eq!(frame, before);
    }
    #[test]
    fn closing_a_float_retires_its_descriptor_and_invisible_float_keeps_custody() {
        let mut frame = two();
        frame
            .descriptors
            .insert(2, serde_json::json!({"draft":true}));
        frame
            .apply(
                &workbench::FloatEvent::Float {
                    tile: TileId(2),
                    rect: workbench::RelativeRect {
                        x: 0.0,
                        y: 0.0,
                        width: 0.5,
                        height: 0.5,
                    },
                }
                .into(),
            )
            .unwrap();
        frame
            .apply(
                &workbench::FloatEvent::SetVisible {
                    tile: TileId(2),
                    visible: false,
                }
                .into(),
            )
            .unwrap();
        assert_eq!(frame.active_ids(), vec![TileId(1)]);
        assert_eq!(frame.panes.len(), 2);
        frame.validate().unwrap();
        frame.apply(&TileEvent::Closed(TileId(2)).into()).unwrap();
        assert!(frame.descriptors.is_empty());
        assert_eq!(frame.panes.len(), 1);
    }
}
