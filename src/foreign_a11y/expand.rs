// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn expand(
    updates: Vec<TreeUpdate>,
    children: &HashMap<TreeId, Vec<TreeUpdate>>,
) -> Vec<TreeUpdate> {
    let mut result = Vec::new();
    for update in updates {
        match children.get(&update.tree_id) {
            Some(frame) => result.extend(frame.iter().cloned()),
            None => result.push(update),
        }
    }
    result
}
