// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Every containment ancestry of `id`, each as a root-first scope path ending
/// in the node itself.
///
/// A node with no container is its own single-segment scope, which is also
/// what a node the graph no longer holds falls back to.
pub fn ancestry_scopes(graph: &Graph, id: &str) -> Vec<ScopePath> {
    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        return Vec::new();
    };
    let Some(key) = graph.get_node_key_by_id(uuid) else {
        // Removed, or never present: the bare id is all there is to say.
        return ScopePath::parse(id).into_iter().collect();
    };

    // Containment runs member -> container, so a node's containers are its
    // outgoing containment targets.
    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut frontier: Vec<(Vec<String>, mere::kernel::graph::NodeKey, HashSet<String>)> = {
        let mut seen = HashSet::new();
        seen.insert(id.to_string());
        vec![(vec![id.to_string()], key, seen)]
    };

    while let Some((path, at, seen)) = frontier.pop() {
        // URL/domain/filesystem containment belongs to Resource relations;
        // user folders and collections remain Surface relations. Project both
        // into the Surface vocabulary watches already use.
        let mut containers: Vec<_> = graph
            .projected_outgoing_relations(at)
            .filter(|(_, _, payload)| {
                payload
                    .containment_data()
                    .is_some_and(|data| !data.sub_kinds.is_empty())
            })
            .map(|(container, _, _)| container)
            .collect();
        containers.sort_unstable_by_key(|key| graph.get_node(*key).map(|node| node.id));
        containers.dedup();
        let mut grew = false;
        for container in containers {
            let Some(node) = graph.get_node(container) else {
                continue;
            };
            let container_id = node.id.to_string();
            if seen.contains(&container_id) || path.len() >= MAX_ANCESTRY_DEPTH {
                // A cycle, or deeper than a scope is worth. The path so far is
                // still a real region, so it is kept rather than dropped.
                continue;
            }
            let mut next = path.clone();
            next.push(container_id.clone());
            let mut seen = seen.clone();
            seen.insert(container_id);
            frontier.push((next, container, seen));
            grew = true;
        }
        if !grew {
            paths.push(path);
        }
    }

    // Built leaf-first while walking; a scope reads outermost-first.
    paths
        .into_iter()
        .filter_map(|mut path| {
            path.reverse();
            ScopePath::parse(&path.join("/")).ok()
        })
        .collect()
}
