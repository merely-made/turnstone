// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Whether `node` still lives at `url` — the staleness gate. Enrichment
/// belongs to the page that was fetched; a node that has navigated away (or
/// died) since the request drops the late result explicitly.
pub(crate) fn still_current(canvas: &Canvas, node: Uuid, url: &str) -> bool {
    canvas
        .graph()
        .get_node_by_id(node)
        .is_some_and(|(_, n)| n.url() == url)
}
