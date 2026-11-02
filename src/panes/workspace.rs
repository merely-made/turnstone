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
    /// Optional pane-local view state travels and persists with the same pane
    /// identity, independently of its durable `PaneSpec` descriptor.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub view_states: BTreeMap<u64, serde_json::Value>,
}

impl<C: Clone> PaneWorkspace<C> {
    pub fn single(id: u64, title: String, payload: C) -> Self {
        Self {
            workspace: Workspace::new(TileTree::single(Self::tile(id, title))),
            panes: BTreeMap::from([(id, payload)]),
            descriptors: BTreeMap::new(),
            view_states: BTreeMap::new(),
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
        if self
            .view_states
            .keys()
            .any(|id| !self.panes.contains_key(id))
        {
            return Err("Unplaced pane view state".into());
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
        next.view_states.retain(|id, _| next.panes.contains_key(id));
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
        self.view_states.remove(&id);
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
        let view_state = source.view_states.remove(&id);
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
        if let Some(view_state) = view_state {
            next.view_states.insert(id, view_state);
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
#[path = "workspace/tests.rs"]
mod tests;
