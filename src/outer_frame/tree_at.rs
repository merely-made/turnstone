// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn tree_at<'a>(mut tree: &'a TileTree, path: &[usize]) -> Option<&'a TileTree> {
    for i in path {
        let TileTree::Split { children, .. } = tree else {
            return None;
        };
        tree = &children.get(*i)?.tree;
    }
    Some(tree)
}
