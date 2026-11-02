// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn body_slots(
    dom: &ScriptedDom,
    root: NodeId,
) -> Vec<(TileId, cambium::SlotKind, NodeId)> {
    nodes_at(dom, root)
        .into_iter()
        .filter_map(|node| {
            let class = attr(dom, node, "class")?;
            if !class.split_whitespace().any(|c| c == "frisket-content") {
                return None;
            }
            let id = attr(dom, node, cambium::FRISKET_TILE_ATTR)?.parse().ok()?;
            Some((TileId(id), cambium::slot_kind(dom, node)?, node))
        })
        .collect()
}
