// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read content tags through the supplier's resource-aware reader when present.
//!
//! The pinned Mere predates that additive API. Rust prefers an inherent method
//! over a trait method, so the temporary legacy fallback keeps this consumer
//! buildable at its current pin and yields to Mere's reader after integration.
//! Remove the fallback when the coordinated supplier set is qualified.

use mere::kernel::graph::{Graph, NodeKey};
use std::collections::HashSet;

// Intentionally unused once Mere supplies the inherent reader.
#[allow(dead_code)]
trait LegacyContentTags {
    fn node_content_tags(&self, key: NodeKey) -> Option<HashSet<String>>;
}

impl LegacyContentTags for Graph {
    fn node_content_tags(&self, key: NodeKey) -> Option<HashSet<String>> {
        self.node_tags(key).cloned()
    }
}

pub(crate) fn content_tags(graph: &Graph, key: NodeKey) -> HashSet<String> {
    graph.node_content_tags(key).unwrap_or_default()
}
