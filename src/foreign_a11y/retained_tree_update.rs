// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RetainedTree {
    pub(super) fn update(
        previous: Option<&Self>,
        update: TreeUpdate,
    ) -> Result<Self, MaterializationError> {
        let mut retained = match previous {
            Some(previous) => previous.clone(),
            None => Self {
                tree: update.tree.clone().ok_or_else(|| {
                    MaterializationError::Incomplete(
                        "initial foreign update has no tree data".into(),
                    )
                })?,
                nodes: std::sync::Arc::new(BTreeMap::new()),
                focus: update.focus,
                publication: Grafts::new(),
            },
        };
        if let Some(tree) = update.tree {
            retained.tree = tree;
        }
        retained.focus = update.focus;
        let mut changed = HashSet::new();
        for (id, node) in update.nodes {
            if !changed.insert(id) {
                return Err(MaterializationError::Invalid(format!(
                    "duplicate local node {id:?}"
                )));
            }
            std::sync::Arc::make_mut(&mut retained.nodes).insert(id, node);
        }
        let mut live = HashSet::new();
        let mut pending = vec![retained.tree.root];
        while let Some(id) = pending.pop() {
            if !live.insert(id) {
                return Err(MaterializationError::Invalid(
                    "local node graph has a cycle or multiple parents".into(),
                ));
            }
            let node = retained.nodes.get(&id).ok_or_else(|| {
                MaterializationError::Incomplete(format!("missing local child {id:?}"))
            })?;
            if node.tree_id().is_some() && !node.children().is_empty() {
                return Err(MaterializationError::Invalid(
                    "foreign graft node also has local children".into(),
                ));
            }
            pending.extend(node.children().iter().rev().copied());
        }
        if retained.nodes[&retained.tree.root].tree_id().is_some() {
            return Err(MaterializationError::Invalid(
                "foreign tree root cannot itself be a graft".into(),
            ));
        }
        if !live.contains(&retained.focus) {
            return Err(MaterializationError::Incomplete(
                "foreign focus is absent or unreachable".into(),
            ));
        }
        if changed.iter().any(|id| !live.contains(id)) {
            return Err(MaterializationError::Invalid(
                "foreign update contains unreachable changed nodes".into(),
            ));
        }
        // Nodes omitted after their parent's child-list change are retired,
        // including their action masks. They are never retained as targets.
        std::sync::Arc::make_mut(&mut retained.nodes).retain(|id, _| live.contains(id));
        Ok(retained)
    }

    pub(super) fn children(&self) -> Vec<TreeId> {
        self.nodes.values().filter_map(Node::tree_id).collect()
    }

    pub(super) fn snapshot(&self, id: TreeId) -> TreeUpdate {
        TreeUpdate {
            nodes: self
                .nodes
                .iter()
                .map(|(id, node)| (*id, node.clone()))
                .collect(),
            tree: Some(self.tree.clone()),
            tree_id: id,
            focus: self.focus,
        }
    }
}
