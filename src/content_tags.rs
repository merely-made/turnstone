// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read content tags through the graph's resource-aware reader.
//!
//! Since mere r44 a tag is an assertion on the shown resource, not a flag on the
//! Surface node, so `Graph::node_tags` (the raw Surface set) no longer sees tags
//! written through `Canvas::tag_node`. Every read goes through here.

use mere::kernel::graph::{Graph, NodeKey};
use std::collections::HashSet;

pub(crate) fn content_tags(graph: &Graph, key: NodeKey) -> HashSet<String> {
    graph.node_content_tags(key).unwrap_or_default()
}
