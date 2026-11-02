// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Flatten any already-composed tree to the lines scenario `assert a11y`
/// reads. The shell uses this overload so live contributed DOM semantics and
/// the platform adapter are checked from the same projection.
pub fn tree_lines(tree: &UxTree) -> Vec<String> {
    forest_lines(&[tree.to_tree_update(None)])
}
