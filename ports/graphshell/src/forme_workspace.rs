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
        let (arrangement, geometry) = layout.to_arrangement();
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
fn slot(graph: Uuid) -> String {
    format!("graphshell.forme-workspace/v1/{graph}")
}
fn valid_bounds(b: [f32; 4]) -> bool {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    b.iter().all(|v| v.is_finite()) && w.is_finite() && h.is_finite() && w >= 80. && h >= 80.
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
mod tests {
    use super::*;
    use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
    use muniment::MemoryBackend;
    fn graph() -> (Graph, Uuid, Uuid) {
        let mut g = Graph::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        add_node(
            &mut g,
            Some(a),
            "https://example.org/same".into(),
            Default::default(),
        );
        add_node(
            &mut g,
            Some(b),
            "https://example.org/same".into(),
            Default::default(),
        );
        (g, a, b)
    }
    #[test]
    fn two_accesses_share_resource_but_keep_separate_tiles_and_regions() {
        let (g, a, b) = graph();
        let ka = g.get_node_by_id(a).unwrap().0;
        let kb = g.get_node_by_id(b).unwrap().0;
        assert_ne!(ka, kb);
        assert!(g.shown_resource_id(ka).is_some());
        assert_eq!(g.shown_resource_id(ka), g.shown_resource_id(kb));
        let mut w = FormeWorkspace::new(Uuid::from_u128(10));
        w.open(a, &g).unwrap();
        w.open(b, &g).unwrap();
        w.open(a, &g).unwrap();
        assert_eq!(w.layout().tile_count(), 2);
        let r = w.region();
        assert_eq!(r.cells.len(), 2);
        assert!(r.cells[0].bounds[2] <= r.cells[1].bounds[0]);
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.fields().count(), 0);
    }
    #[test]
    fn nested_placement_and_active_tabs_roundtrip_without_pixel_geometry() {
        let (mut g, a, b) = graph();
        let c = Uuid::from_u128(3);
        add_node(
            &mut g,
            Some(c),
            "https://example.org/other".into(),
            Default::default(),
        );
        let mut w = FormeWorkspace::new(Uuid::from_u128(10));
        for m in [a, b, c] {
            w.open(m, &g).unwrap();
        }
        w.locked = false;
        w.event(
            TileEvent::Dragged {
                tile: TileId(3),
                to: DropTarget::Edge {
                    tile: TileId(2),
                    edge: workbench::Edge::Bottom,
                },
            },
            &g,
        )
        .unwrap();
        let before = w.geometry.clone();
        let json = serde_json::to_string(&w).unwrap();
        let copy: FormeWorkspace = serde_json::from_str(&json).unwrap();
        copy.validate(w.document.graph_id).unwrap();
        assert_eq!(copy.geometry, before);
        assert_eq!(copy.region().cells.len(), 3);
        assert!(copy.region().cells[1].bounds[1] < copy.region().cells[2].bounds[1]);
    }
    #[test]
    fn lock_blocks_layout_edits_but_allows_focus_and_close() {
        let (g, a, b) = graph();
        let mut w = FormeWorkspace::new(Uuid::from_u128(10));
        w.open(a, &g).unwrap();
        w.open(b, &g).unwrap();
        let before = w.geometry.clone();
        assert!(
            w.event(
                TileEvent::DividerMoved {
                    split: workbench::TilePath(vec![]),
                    fractions: vec![0.3, 0.7]
                },
                &g
            )
            .is_err()
        );
        assert_eq!(w.geometry, before);
        w.event(TileEvent::Activated(TileId(2)), &g).unwrap();
        w.event(TileEvent::Closed(TileId(1)), &g).unwrap();
        assert_eq!(w.members(), vec![b]);
    }
    #[test]
    fn persistence_reconciles_removed_access_and_rejects_foreign_session() {
        pollster::block_on(async {
            let (mut g, a, b) = graph();
            let backend = MemoryBackend::new();
            let id = Uuid::from_u128(10);
            let mut w = FormeWorkspace::new(id);
            w.open(a, &g).unwrap();
            w.open(b, &g).unwrap();
            let identity = w.document.id;
            w.bounds = [10., 20., 610., 420.];
            w.locked = false;
            w.save(backend.clone()).await.unwrap();
            let restored = FormeWorkspace::load(backend.clone(), id, &g).await.unwrap();
            assert_eq!(restored.document.id, identity);
            assert_eq!(restored.bounds, w.bounds);
            let key = g.get_node_by_id(b).unwrap().0;
            apply_graph_delta(&mut g, GraphDelta::RemoveNode { key });
            let restored = FormeWorkspace::load(backend, id, &g).await.unwrap();
            assert_eq!(restored.members(), vec![a]);
            assert!(w.validate(Uuid::from_u128(20)).is_err());
        });
    }
    #[test]
    fn malformed_geometry_is_refused_before_any_install_or_write() {
        let (g, a, _) = graph();
        let mut w = FormeWorkspace::new(Uuid::from_u128(10));
        w.open(a, &g).unwrap();
        w.geometry = Some(TreeGeometry::Stack {
            members: vec![a, a],
            active: 0,
        });
        assert!(w.validate(w.document.graph_id).is_err());
        w.geometry = Some(TreeGeometry::Stack {
            members: vec![a],
            active: 4,
        });
        assert!(w.validate(w.document.graph_id).is_err());
        w.geometry = Some(TreeGeometry::Stack {
            members: vec![a],
            active: 0,
        });
        w.bounds[0] = f32::NAN;
        assert!(w.validate(w.document.graph_id).is_err());
    }
}

#[cfg(test)]
mod stable_identity_tests {
    use super::*;
    use mere::kernel::graph::apply::add_node;
    #[test]
    fn closing_and_reopening_an_access_preserves_all_opaque_tile_ids() {
        let mut graph = Graph::new();
        let ids = [Uuid::from_u128(3), Uuid::from_u128(1), Uuid::from_u128(2)];
        for id in ids {
            add_node(
                &mut graph,
                Some(id),
                "https://example.net".into(),
                Default::default(),
            );
        }
        let mut w = FormeWorkspace::new(Uuid::from_u128(10));
        w.open(ids[0], &graph).unwrap();
        w.open(ids[1], &graph).unwrap();
        let first = w.tile_for_member(ids[0]).unwrap();
        let second = w.tile_for_member(ids[1]).unwrap();
        w.event(TileEvent::Closed(second), &graph).unwrap();
        w.open(ids[2], &graph).unwrap();
        assert_eq!(w.tile_for_member(ids[0]), Some(first));
        assert_ne!(w.tile_for_member(ids[2]), Some(second));
        w.open(ids[1], &graph).unwrap();
        assert_eq!(w.tile_for_member(ids[1]), Some(second));
    }
}

#[cfg(test)]
mod tab_placement_tests {
    use super::*;
    use mere::kernel::graph::apply::add_node;
    #[test]
    fn tab_drop_order_and_active_member_survive_persistence() {
        pollster::block_on(async {
            let mut g = Graph::new();
            let ids = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
            let mut w = FormeWorkspace::new(Uuid::from_u128(10));
            for id in ids {
                add_node(
                    &mut g,
                    Some(id),
                    "https://example.net".into(),
                    Default::default(),
                );
                w.open(id, &g).unwrap();
            }
            let mut layout = w.layout();
            layout.stack_all();
            w.keep_layout(&layout);
            w.locked = false;
            w.event(
                TileEvent::Dragged {
                    tile: w.tile_for_member(ids[2]).unwrap(),
                    to: DropTarget::Stack {
                        stack: workbench::TilePath(vec![]),
                        index: 0,
                    },
                },
                &g,
            )
            .unwrap();
            assert_eq!(
                w.geometry,
                Some(TreeGeometry::Stack {
                    members: vec![ids[2], ids[0], ids[1]],
                    active: 0
                })
            );
            w.event(TileEvent::Activated(w.tile_for_member(ids[1]).unwrap()), &g)
                .unwrap();
            let backend = muniment::MemoryBackend::new();
            w.save(backend.clone()).await.unwrap();
            let restored = FormeWorkspace::load(backend, w.document.graph_id, &g)
                .await
                .unwrap();
            assert_eq!(restored.geometry, w.geometry);
            assert_eq!(restored.region().cells[0].member, ids[1]);
        });
    }
}
