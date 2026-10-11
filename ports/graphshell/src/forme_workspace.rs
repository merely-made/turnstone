// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A saved forme and the two projections of its canonical arrangement.
//! The host owns this browser workspace, independently of graph source edits.
//! No resource coalescing, scalar field, coupling or inferred source link is created.

use mere::canvas::{FormeCell, FormeRegion};
use mere::forme::FormeDocument;
use mere::kernel::graph::Graph;
use muniment::{Backend, JsonSlots as Slots};
use platen::{
    TileLayout,
    projection_geometry::{Axis, TreeGeometry},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;
use workbench::{ContentSource, DropTarget, SplitAxis, Tile, TileEvent, TileId, TileTree};

pub const WORKSPACE_VERSION: u16 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormeWorkspace {
    pub version: u16,
    pub document: FormeDocument,
    pub geometry: Option<TreeGeometry>,
    /// World-space outer region, separate from the relative tile geometry.
    pub bounds: [f32; 4],
    pub locked: bool,
    pub visible: bool,
    tile_ids: BTreeMap<Uuid, u64>,
}

impl FormeWorkspace {
    pub fn new(graph_id: Uuid) -> Self {
        Self {
            version: WORKSPACE_VERSION,
            document: FormeDocument::new(graph_id, Some("Workbench".into())),
            geometry: None,
            bounds: [-240., -150., 240., 150.],
            locked: true,
            visible: true,
            tile_ids: BTreeMap::new(),
        }
    }
    pub fn layout(&self) -> TileLayout {
        TileLayout::from_arrangement(&self.document.arrangement, self.geometry.as_ref())
    }
    pub fn keep_layout(&mut self, layout: &TileLayout) {
        let (mut arrangement, geometry) = layout.to_arrangement();
        arrangement.preserve_member_identity(&self.document.arrangement);
        self.document.arrangement = arrangement;
        self.geometry = geometry;
    }
    /// Open or focus an existing access, keyed by surface UUID rather than address.
    pub fn open(&mut self, member: Uuid, graph: &Graph) -> Result<(), String> {
        graph
            .get_node_by_id(member)
            .ok_or("the access is no longer present")?;
        if !self.members().contains(&member) && self.members().len() >= 256 {
            return Err("Workbench tile limit reached".into());
        }
        if !self.tile_ids.contains_key(&member) {
            if self.tile_ids.len() >= 1024 {
                return Err("Workbench access limit reached".into());
            }
            let next = self
                .tile_ids
                .values()
                .max()
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Workbench tile identity limit reached")?;
            self.tile_ids.insert(member, next);
        }
        let mut layout = self.layout();
        if layout.has_tile(member) {
            layout.activate(member);
        } else {
            layout.open_tile(member);
        }
        self.keep_layout(&layout);
        Ok(())
    }
    pub fn tile_tree(&self, graph: &Graph) -> Option<TileTree> {
        // Saved opaque keys survive closing/reopening other tabs and never name a Resource.
        self.layout().to_tile_tree(|member| Tile {
            id: TileId(self.tile_ids[&member]),
            title: graph
                .get_node_by_id(member)
                .map(|(_, n)| n.title.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| member.to_string()),
            content: ContentSource::Open {
                kind: "graphshell.reading".into(),
                id: member.to_string(),
            },
            accent: None,
        })
    }
    pub fn members(&self) -> Vec<Uuid> {
        let mut members = self.document.arrangement.referenced_members();
        members.sort();
        members.dedup();
        members
    }
    pub fn member_for_tile(&self, tile: TileId) -> Option<Uuid> {
        self.tile_ids
            .iter()
            .find(|(_, id)| **id == tile.0)
            .map(|(member, _)| *member)
            .filter(|member| self.members().contains(member))
    }
    pub fn tile_for_member(&self, member: Uuid) -> Option<TileId> {
        self.tile_ids.get(&member).copied().map(TileId)
    }
    pub fn event(&mut self, event: TileEvent, graph: &Graph) -> Result<(), String> {
        let mut layout = self.layout();
        match event {
            TileEvent::Activated(tile) => {
                layout.activate(self.member_for_tile(tile).ok_or("unknown tile")?);
            },
            TileEvent::Closed(tile) => {
                layout.close_tile(self.member_for_tile(tile).ok_or("unknown tile")?);
            },
            TileEvent::DividerMoved { split, fractions } => {
                if self.locked {
                    return Err("Unlock the forme to change its arrangement".into());
                }
                if fractions.iter().any(|f| !f.is_finite() || *f <= 0.) {
                    return Err("Split shares must be finite and positive".into());
                }
                let current = layout.split_fractions(&split.0).ok_or("unknown split")?;
                if fractions.len() != current.len() {
                    return Err("Provide one split share for each region".into());
                }
                if !layout.set_split_fractions(&split.0, &fractions) {
                    return Err("unknown split".into());
                }
            },
            TileEvent::Dragged { tile, to } => {
                if self.locked {
                    return Err("Unlock the forme to change its arrangement".into());
                }
                let member = self.member_for_tile(tile).ok_or("unknown tile")?;
                let tree = self.tile_tree(graph).ok_or("empty forme")?;
                let target = match &to {
                    DropTarget::Stack { stack, .. } => match tree_at(&tree, &stack.0) {
                        Some(TileTree::Stack(s)) => s
                            .tabs
                            .iter()
                            .find(|t| t.id != tile)
                            .map(|t| t.id)
                            .or(Some(tile)),
                        _ => None,
                    },
                    DropTarget::Edge { tile: target, .. } => Some(*target),
                    _ => return Err("This host keeps tiles in its workbench".into()),
                }
                .and_then(|t| self.member_for_tile(t))
                .ok_or("unknown drop region")?;
                match to {
                    DropTarget::Stack { index, .. } => {
                        layout.move_to_stack_index_of(member, target, index);
                    },
                    DropTarget::Edge { edge, .. } => {
                        use workbench::Edge;
                        let (axis, after) = match edge {
                            Edge::Left => (SplitAxis::Row, false),
                            Edge::Right => (SplitAxis::Row, true),
                            Edge::Top => (SplitAxis::Column, false),
                            Edge::Bottom => (SplitAxis::Column, true),
                        };
                        if member == target {
                            layout.split_out(member, axis, after);
                        } else {
                            layout.split_beside_axis(member, target, axis, after);
                        }
                    },
                    _ => {},
                }
            },
        }
        let mut candidate = self.clone();
        candidate.keep_layout(&layout);
        candidate.validate(self.document.graph_id)?;
        *self = candidate;
        Ok(())
    }
    pub fn region(&self) -> FormeRegion {
        let mut cells = Vec::new();
        if let Some(geometry) = &self.geometry {
            cells_of(geometry, [0., 0., 1., 1.], &mut cells);
        }
        FormeRegion {
            id: self.document.id.as_uuid(),
            bounds: self.bounds,
            cells,
            locked: self.locked,
            visible: self.visible,
        }
    }
    pub fn validate(&self, graph_id: Uuid) -> Result<(), String> {
        if self.version != WORKSPACE_VERSION {
            return Err("unsupported forme workspace version".into());
        }
        if self.document.graph_id != graph_id {
            return Err("forme belongs to another graph session".into());
        }
        if !valid_bounds(self.bounds) {
            return Err("invalid forme extent".into());
        }
        let ids: HashSet<_> = self.tile_ids.values().copied().collect();
        if self.tile_ids.len() > 1024
            || ids.len() != self.tile_ids.len()
            || ids.contains(&0)
            || self
                .members()
                .iter()
                .any(|m| !self.tile_ids.contains_key(m))
        {
            return Err("invalid stable tile identities".into());
        }
        if let Some(g) = &self.geometry {
            let mut seen = HashSet::new();
            validate_geometry(g, 0, &mut seen)?;
            if seen.len() > 256 {
                return Err("forme exceeds workspace limits".into());
            }
            if seen != self.members().into_iter().collect::<HashSet<_>>() {
                return Err("forme membership and geometry disagree".into());
            }
            let region = self.region();
            if region.cells.iter().any(|cell| {
                !positive_bounds(cell.bounds) || !positive_bounds(region.cell_world_bounds(cell))
            }) {
                return Err("Forme geometry collapses a tile region".into());
            }
        } else if !self.members().is_empty() {
            return Err("forme geometry is missing".into());
        }
        Ok(())
    }
    pub async fn load<B: Backend>(
        backend: B,
        graph_id: Uuid,
        graph: &Graph,
    ) -> Result<Self, String> {
        let saved: Option<Self> = Slots::new(backend)
            .load(&slot(graph_id))
            .await
            .map_err(|e| e.to_string())?;
        let mut saved = saved.unwrap_or_else(|| Self::new(graph_id));
        saved.validate(graph_id)?;
        let mut layout = saved.layout();
        for member in layout.open_members() {
            if graph.get_node_by_id(member).is_none() {
                layout.close_tile(member);
            }
        }
        saved.keep_layout(&layout);
        Ok(saved)
    }
    pub async fn save<B: Backend>(&self, backend: B) -> Result<(), String> {
        self.validate(self.document.graph_id)?;
        Slots::new(backend)
            .save(&slot(self.document.graph_id), self)
            .await
            .map_err(|e| e.to_string())
    }
}
pub(crate) fn slot(graph: Uuid) -> String {
    format!("graphshell.forme-workspace/v1/{graph}")
}
fn valid_bounds(b: [f32; 4]) -> bool {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    b.iter().all(|v| v.is_finite()) && w.is_finite() && h.is_finite() && w >= 80. && h >= 80.
}
fn positive_bounds(b: [f32; 4]) -> bool {
    b.iter().all(|v| v.is_finite()) && b[2] > b[0] && b[3] > b[1]
}
fn validate_geometry(
    g: &TreeGeometry,
    depth: usize,
    seen: &mut HashSet<Uuid>,
) -> Result<(), String> {
    if depth > 32 || seen.len() > 256 {
        return Err("forme exceeds workspace limits".into());
    }
    match g {
        TreeGeometry::Stack { members, active } => {
            if members.is_empty() || *active >= members.len() {
                return Err("invalid tab stack".into());
            }
            for m in members {
                if !seen.insert(*m) {
                    return Err("duplicate access in forme".into());
                }
            }
        },
        TreeGeometry::Split { children, .. } => {
            if children.len() < 2 || children.len() > 256 {
                return Err("invalid split".into());
            }
            let total: f32 = children.iter().map(|b| b.fraction).sum();
            if !total.is_finite() || total <= 0. {
                return Err("invalid split shares".into());
            }
            for b in children {
                if !b.fraction.is_finite() || b.fraction <= 0. {
                    return Err("invalid split share".into());
                }
                validate_geometry(&b.node, depth + 1, seen)?;
            }
        },
    }
    Ok(())
}
fn cells_of(g: &TreeGeometry, b: [f32; 4], out: &mut Vec<FormeCell>) {
    match g {
        TreeGeometry::Stack { members, active } => {
            if let Some(member) = members.get(*active) {
                out.push(FormeCell {
                    member: *member,
                    bounds: b,
                });
            }
        },
        TreeGeometry::Split { axis, children } => {
            let total: f32 = children.iter().map(|c| c.fraction).sum();
            let i = if *axis == Axis::Row { 0 } else { 1 };
            let length = b[i + 2] - b[i];
            let mut start = b[i];
            for (index, child) in children.iter().enumerate() {
                let mut cb = b;
                cb[i] = start;
                start = if index + 1 == children.len() {
                    b[i + 2]
                } else {
                    start + length * child.fraction / total
                };
                cb[i + 2] = start;
                cells_of(&child.node, cb, out);
            }
        },
    }
}
fn tree_at<'a>(tree: &'a TileTree, path: &[usize]) -> Option<&'a TileTree> {
    if path.is_empty() {
        return Some(tree);
    }
    if let TileTree::Split { children, .. } = tree {
        tree_at(&children.get(path[0])?.tree, &path[1..])
    } else {
        None
    }
}
#[cfg(test)]
mod tests;
