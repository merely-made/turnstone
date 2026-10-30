// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The at-rest "where am I" caption: the focused node's display caption (its
/// label, plus its host when that adds information), or `None` with nothing
/// focused.
pub fn focused_caption(canvas: &mere::canvas::Canvas) -> Option<String> {
    let url = canvas.focused_url()?.to_string();
    let graph = canvas.graph();
    let (key, _) = graph.get_node_by_url(&url)?;
    Some(graph.node_display_caption(key))
}
