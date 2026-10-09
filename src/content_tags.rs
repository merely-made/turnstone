// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read descriptive tags from the resource shown by a surface.

use mere::kernel::graph::{Graph, NodeKey};
use std::collections::HashSet;

pub(crate) fn content_tags(graph: &Graph, key: NodeKey) -> HashSet<String> {
    graph.node_content_tags(key).unwrap_or_default()
}
