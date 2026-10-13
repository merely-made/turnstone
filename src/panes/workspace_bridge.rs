// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Read legacy pane trees once; new edits use the shared Workspace reducer.
use super::{FrisketLayout, GraphId, InsertSide, PaneContent, PaneId, PaneNode, SplitChoice};
use std::collections::BTreeMap;
use workbench::{
    Edge, TabStack, TileBranch, TileId, TilePath, TileTree, WorkbenchOutcome, Workspace,
    WorkspaceEvent,
};

pub type OuterWorkspace = super::workspace::PaneWorkspace<(PaneContent, GraphId)>;

pub fn path_indices(path: &[SplitChoice]) -> TilePath {
    TilePath(
        path.iter()
            .map(|choice| match choice {
                SplitChoice::First => 0,
                SplitChoice::Second => 1,
                SplitChoice::Child(i) => *i,
            })
            .collect(),
    )
}

pub fn choices(path: &[usize]) -> Vec<SplitChoice> {
    path.iter()
        .map(|i| match i {
            0 => SplitChoice::First,
            1 => SplitChoice::Second,
            i => SplitChoice::Child(*i),
        })
        .collect()
}

pub fn tree_at<'a>(tree: &'a TileTree, path: &[SplitChoice]) -> Option<&'a TileTree> {
    let mut node = tree;
    for i in path_indices(path).0 {
        let TileTree::Split { children, .. } = node else {
            return None;
        };
        node = &children.get(i)?.tree;
    }
    Some(node)
}

pub fn path_for(tree: &TileTree, id: TileId) -> Option<Vec<SplitChoice>> {
    fn walk(tree: &TileTree, id: TileId, path: &mut Vec<usize>) -> Option<Vec<SplitChoice>> {
        match tree {
            TileTree::Stack(stack) => stack
                .tabs
                .iter()
                .any(|tile| tile.id == id)
                .then(|| choices(path)),
            TileTree::Split { children, .. } => {
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    if let Some(found) = walk(&child.tree, id, path) {
                        return Some(found);
                    }
                    path.pop();
                }
                None
            },
        }
    }
    walk(tree, id, &mut Vec::new())
}

pub fn active_at(frame: &OuterWorkspace, path: &[SplitChoice]) -> Option<TileId> {
    let TileTree::Stack(stack) = tree_at(frame.workspace.tiled(), path)? else {
        return None;
    };
    stack.tabs.get(stack.active).map(|tile| tile.id)
}

pub fn edge(side: InsertSide) -> Edge {
    match side {
        InsertSide::Left => Edge::Left,
        InsertSide::Right => Edge::Right,
        InsertSide::Above => Edge::Top,
        InsertSide::Below => Edge::Bottom,
    }
}

fn convert(
    node: &PaneNode,
    panes: &mut BTreeMap<u64, (PaneContent, GraphId)>,
) -> Result<TileTree, String> {
    match node {
        PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        } => {
            if panes
                .insert(pane_id.0, (content.clone(), *graph_id))
                .is_some()
            {
                return Err("Duplicate legacy pane identity".into());
            }
            Ok(TileTree::Stack(TabStack {
                tabs: vec![OuterWorkspace::tile(pane_id.0, content.tag().into())],
                active: 0,
            }))
        },
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            if !ratio.is_finite() || *ratio <= 0.0 || *ratio >= 1.0 {
                return Err("Invalid legacy split ratio".into());
            }
            Ok(TileTree::Split {
                axis: match axis {
                    super::SplitAxis::Horizontal => workbench::SplitAxis::Row,
                    super::SplitAxis::Vertical => workbench::SplitAxis::Column,
                },
                children: vec![
                    TileBranch::new(*ratio, convert(first, panes)?),
                    TileBranch::new(1.0 - *ratio, convert(second, panes)?),
                ],
            })
        },
        PaneNode::Workspace(_) => Err("Workspace nested inside a legacy tree".into()),
    }
}

impl FrisketLayout {
    pub fn outer_workspace(&self) -> Result<std::borrow::Cow<'_, OuterWorkspace>, String> {
        if let PaneNode::Workspace(frame) = &self.root {
            frame.validate()?;
            return Ok(std::borrow::Cow::Borrowed(frame));
        }
        let mut panes = BTreeMap::new();
        let tree = convert(&self.root, &mut panes)?;
        let frame = OuterWorkspace {
            workspace: Workspace::new(tree),
            panes,
        };
        frame.validate()?;
        Ok(std::borrow::Cow::Owned(frame))
    }

    pub fn adopt_workspace(&mut self) -> Result<(), String> {
        if !matches!(self.root, PaneNode::Workspace(_)) {
            let frame = self.outer_workspace()?.into_owned();
            self.root = PaneNode::Workspace(Box::new(frame));
        }
        Ok(())
    }

    pub fn workspace_mut(&mut self) -> Option<&mut OuterWorkspace> {
        self.adopt_workspace().ok()?;
        match &mut self.root {
            PaneNode::Workspace(frame) => Some(frame),
            _ => None,
        }
    }

    pub fn apply_workspace(&mut self, event: &WorkspaceEvent) -> Result<WorkbenchOutcome, String> {
        self.adopt_workspace()?;
        self.workspace_mut()
            .ok_or("Pane workspace unavailable")?
            .apply(event)
    }

    pub fn close_pane(&mut self, pane: PaneId) -> bool {
        let Some(frame) = self.workspace_mut() else {
            return false;
        };
        if frame.workspace.tiles().len() <= 1 {
            return false;
        }
        frame.take(pane.0).is_some()
    }
}
