// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Pane operations over the canonical shared Workspace. Legacy paths remain
//! accepted by existing actions; shared frame gestures address TileId directly.
use super::workspace_bridge::{active_at, edge, path_for, path_indices, tree_at};
use crate::panes::{
    FrisketLayout, GraphId, InsertSide, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice,
    SplitPath,
};
use workbench::{DropTarget, TileEvent, TileId, TileTree};

impl FrisketLayout {
    pub fn set_split_ratio(&mut self, path: &[SplitChoice], ratio: f32) -> bool {
        if !ratio.is_finite() {
            return false;
        }
        let Some(frame) = self.workspace_mut() else {
            return false;
        };
        let Some(TileTree::Split { children, .. }) = tree_at(frame.workspace.tiled(), path) else {
            return false;
        };
        // The existing ratio action names a binary split. N-ary frame gestures
        // carry all shares through WorkspaceEvent instead.
        if children.len() != 2 {
            return false;
        }
        let ratio = ratio.clamp(0.05, 0.95);
        frame
            .apply(
                &TileEvent::DividerMoved {
                    split: path_indices(path),
                    fractions: vec![ratio, 1.0 - ratio],
                }
                .into(),
            )
            .is_ok_and(|outcome| outcome.changed())
    }

    pub fn split_at(&self, path: &[SplitChoice]) -> Option<(SplitAxis, f32)> {
        let frame = self.outer_workspace().ok()?;
        let TileTree::Split { axis, children } = tree_at(frame.workspace.tiled(), path)? else {
            return None;
        };
        if children.len() != 2 {
            return None;
        }
        Some((
            match axis {
                workbench::SplitAxis::Row => SplitAxis::Horizontal,
                workbench::SplitAxis::Column => SplitAxis::Vertical,
            },
            children[0].fraction,
        ))
    }

    pub fn summon_leaf(&mut self, path: &[SplitChoice], side: InsertSide, leaf: PaneNode) -> bool {
        let PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        } = leaf
        else {
            return false;
        };
        let Some(frame) = self.workspace_mut() else {
            return false;
        };
        let target=active_at(frame,path).or_else(||frame.workspace.tiled().tiles().last().map(|t|t.id)).unwrap_or(workbench::TileId(pane_id.0));
        frame.insert_beside(
            target.0,
            edge(side),
            pane_id.0,
            content.label().into(),
            (content, graph_id),
        )
    }

    pub fn reparent_leaf(
        &mut self,
        from: &[SplitChoice],
        to: &[SplitChoice],
        side: InsertSide,
    ) -> bool {
        let Some(frame) = self.workspace_mut() else {
            return false;
        };
        let (Some(source), Some(target)) = (active_at(frame, from), active_at(frame, to)) else {
            return false;
        };
        if source == target {
            return false;
        }
        frame
            .apply(
                &TileEvent::Dragged {
                    tile: source,
                    to: DropTarget::Edge {
                        tile: target,
                        edge: edge(side),
                    },
                }
                .into(),
            )
            .is_ok_and(|outcome| outcome.changed())
    }

    pub fn close_leaf(&mut self, path: &[SplitChoice]) -> bool {
        let Some(frame) = self.workspace_mut() else {
            return false;
        };
        let Some(id) = active_at(frame, path) else {
            return false;
        };
        self.close_pane(PaneId(id.0))
    }

    pub fn retag_graph_bound(&mut self, graph: GraphId) {
        if let Some(frame) = self.workspace_mut() {
            for (content, id) in frame.panes.values_mut() {
                if content.follows_active_graph() {
                    *id = graph;
                }
            }
        }
    }

    pub fn retag_graph_bound_from(&mut self, from: GraphId, to: GraphId) {
        if let Some(frame) = self.workspace_mut() {
            for (content, id) in frame.panes.values_mut() {
                if content.follows_active_graph() && *id == from {
                    *id = to;
                }
            }
        }
    }

    pub fn retag_graph_bound_invalid(
        &mut self,
        valid: &std::collections::HashSet<GraphId>,
        fallback: GraphId,
    ) {
        if let Some(frame) = self.workspace_mut() {
            for (content, id) in frame.panes.values_mut() {
                if content.follows_active_graph() && !valid.contains(id) {
                    *id = fallback;
                }
            }
        }
    }

    pub fn dedupe_graph_panes(&mut self) {
        let Some(frame) = self.workspace_mut() else {
            return;
        };
        let mut seen = std::collections::HashSet::new();
        let drop: Vec<_> = frame
            .workspace
            .tiles()
            .iter()
            .filter_map(|tile| {
                let (content, graph) = frame.panes.get(&tile.id.0)?;
                (matches!(content, PaneContent::Orrery) && !seen.insert(*graph)).then_some(tile.id)
            })
            .collect();
        for id in drop {
            frame.take(id.0);
        }
    }

    pub fn content_mut(&mut self, id: PaneId) -> Option<&mut PaneContent> {
        Some(&mut self.workspace_mut()?.panes.get_mut(&id.0)?.0)
    }

    /// Every retained pane, including inactive tabs, in custody order.
    /// Painting/input separately use only each stack's active pane.
    pub fn iter_leaves(&self) -> impl Iterator<Item = (PaneId, &PaneContent, GraphId)> {
        let mut out = Vec::new();
        fn walk<'a>(node: &'a PaneNode, out: &mut Vec<(PaneId, &'a PaneContent, GraphId)>) {
            match node {
                PaneNode::Leaf {
                    pane_id,
                    content,
                    graph_id,
                } => out.push((*pane_id, content, *graph_id)),
                PaneNode::Split { first, second, .. } => {
                    walk(first, out);
                    walk(second, out);
                },
                PaneNode::Workspace(frame) => {
                    for tile in frame.workspace.tiles() {
                        if let Some((content, graph)) = frame.panes.get(&tile.id.0) {
                            out.push((PaneId(tile.id.0), content, *graph));
                        }
                    }
                },
            }
        }
        walk(&self.root, &mut out);
        out.into_iter()
    }
}

pub(crate) fn path_for_pane_id(node: &PaneNode, pane: PaneId) -> Option<SplitPath> {
    if let PaneNode::Workspace(frame) = node {
        return path_for(frame.workspace.tiled(), TileId(pane.0));
    }
    fn walk(node: &PaneNode, pane: PaneId, path: &mut SplitPath) -> Option<SplitPath> {
        match node {
            PaneNode::Leaf { pane_id, .. } => (*pane_id == pane).then(|| path.clone()),
            PaneNode::Split { first, second, .. } => {
                path.push(SplitChoice::First);
                if let Some(hit) = walk(first, pane, path) {
                    return Some(hit);
                }
                path.pop();
                path.push(SplitChoice::Second);
                if let Some(hit) = walk(second, pane, path) {
                    return Some(hit);
                }
                path.pop();
                None
            },
            PaneNode::Workspace(_) => None,
        }
    }
    walk(node, pane, &mut Vec::new())
}
