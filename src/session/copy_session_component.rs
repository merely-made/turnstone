// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Copy the displayed component across Surface and Resource relations. The
/// supplier's older cross-graph helper walks only raw Surface edges. Fresh
/// Surface identities retain copy provenance; Resource identities, records and
/// held assertions travel verbatim within this component. The result is quiet.
pub(crate) fn copy_session_component(
    source: &Graph,
    seed: uuid::Uuid,
    source_graph: Option<String>,
) -> Result<(Graph, mere::kernel::graph::ComponentCopy), String> {
    use std::collections::{HashMap, HashSet};
    let members = source.component_members(seed, &[]);
    let mut graph = Graph::new();
    let mut copy = mere::kernel::graph::ComponentCopy::default();
    let mut remap = HashMap::new();
    let mut resources = HashSet::new();
    for member in members {
        let (key, node) = source
            .get_node_by_id(member)
            .ok_or("component member disappeared")?;
        if let Some(resource) = source.shown_resource_id(key) {
            resources.insert(resource);
        }
        let copied = graph.copy_node_from_xy(node, source_graph.clone(), 0.0, 0.0);
        let id = graph
            .get_node(copied)
            .ok_or("copied member disappeared")?
            .id;
        copy.new_keys.push(copied);
        copy.id_remap.push((member, id));
        remap.insert(member.to_string(), id.to_string());
    }
    // Preserve unshown Resource intermediates and tag concepts too. This is
    // graph closure, with no fetches or changes to Keep/subscription policy.
    let mut neighbors: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for (from, to, _) in source.resource_edges() {
        neighbors.entry(from.id()).or_default().push(to.id());
        neighbors.entry(to.id()).or_default().push(from.id());
    }
    let mut pending: Vec<_> = resources.iter().copied().collect();
    while let Some(resource) = pending.pop() {
        for neighbor in neighbors.get(&resource).into_iter().flatten() {
            if resources.insert(*neighbor) {
                pending.push(*neighbor);
            }
        }
    }
    let original = source.to_snapshot();
    let mut snapshot = graph.to_snapshot();
    snapshot.edges = original
        .edges
        .into_iter()
        .filter_map(|mut edge| {
            edge.from_node_id = remap.get(&edge.from_node_id)?.clone();
            edge.to_node_id = remap.get(&edge.to_node_id)?.clone();
            Some(edge)
        })
        .collect();
    snapshot.resources = original
        .resources
        .into_iter()
        .filter(|record| {
            resources.contains(&chartulary::resource_id_from_canonical_iri(
                &record.canonical_iri,
            ))
        })
        .collect();
    snapshot.resource_edges = original
        .resource_edges
        .into_iter()
        .filter(|edge| {
            edge.from_node_id
                .parse()
                .is_ok_and(|id| resources.contains(&id))
                && edge
                    .to_node_id
                    .parse()
                    .is_ok_and(|id| resources.contains(&id))
        })
        .collect();
    snapshot.shown_resources = original
        .shown_resources
        .into_iter()
        .filter_map(|mut shown| {
            shown.surface_id = remap.get(&shown.surface_id)?.clone();
            Some(shown)
        })
        .collect();
    let facets = graph.facets().clone();
    let mut graph =
        Graph::try_from_recorded_snapshot(&snapshot).map_err(|error| error.to_string())?;
    graph
        .overlay_facets(facets)
        .map_err(|error| error.to_string())?;
    Ok((graph, copy))
}
