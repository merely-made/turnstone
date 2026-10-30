// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn tree_at<'a>(mut tree: &'a TileTree, path: &TilePath) -> Option<&'a TileTree> {
    for index in &path.0 {
        let TileTree::Split { children, .. } = tree else {
            return None;
        };
        tree = &children.get(*index)?.tree;
    }
    Some(tree)
}
