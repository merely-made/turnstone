// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A generated graph for native frame profiles: `n` nodes, each linked to a
//! random earlier one. Built through one snapshot load; minting a node at a
//! time through the canvas re-syncs physics per node and takes minutes at
//! 2,000.

use kernel::geometry::PortablePoint;
use kernel::graph::RelationClaim;
use kernel::graph::apply::{add_node, assert_relation};
use kernel::graph::{Graph, NodeKey};
use mere::profile::core::ids as core_ids;
use uuid::Uuid;

/// A random tree of `n` nodes (at least two), the same for a given `n`.
pub fn tree_graph(n: usize) -> Graph {
    let mut graph = Graph::new();
    let mut keys: Vec<NodeKey> = Vec::with_capacity(n);
    let mut state = 0x9e37_79b9_u32;
    for i in 0..n.max(2) {
        let key = add_node(
            &mut graph,
            Some(Uuid::from_u128(i as u128 + 1)),
            format!("https://tree.example/{i}"),
            PortablePoint::new(0.0, 0.0),
        );
        if i > 0 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let parent = keys[(state >> 8) as usize % i];
            assert_relation(
                &mut graph,
                parent,
                key,
                RelationClaim::of(core_ids::REL_HYPERLINK),
            );
        }
        keys.push(key);
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_tree_has_every_node_and_a_parent_edge_each() {
        let graph = tree_graph(300);
        assert_eq!(graph.nodes().count(), 300);
        assert_eq!(graph.to_snapshot_at(0).resource_edges.len(), 299);
    }
}
