// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host facet edits routed through the graph's recorded delta spine.

use std::collections::{BTreeMap, BTreeSet};

use chartulary::FacetId;
use mere::canvas::Canvas;
use mere::kernel::graph::Graph;
use mere::kernel::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};

pub(crate) fn edit_canvas_facets<R>(
    canvas: &mut Canvas,
    edit: impl FnOnce(&mut mere::kernel::graph::NodeFacetStore) -> R,
) -> Result<R, String> {
    let mut result = None;
    canvas.ingest_graph(|graph| {
        result = Some(edit_graph_facets(graph, edit));
        result.as_ref().is_some_and(Result::is_ok)
    });
    result.unwrap_or_else(|| Err("canvas facet edit did not run".into()))
}

pub(crate) fn try_edit_canvas_facets<R, E>(
    canvas: &mut Canvas,
    edit: impl FnOnce(&mut mere::kernel::graph::NodeFacetStore) -> Result<R, E>,
) -> Result<Result<R, E>, String> {
    let mut result = None;
    canvas.ingest_graph(|graph| {
        let mut candidate = graph.facets().clone();
        let edited = edit(&mut candidate);
        if edited.is_err() {
            result = Some(Ok(edited));
            return false;
        }
        let applied = edit_graph_facets(graph, |facets| *facets = candidate);
        result = Some(applied.map(|()| edited));
        result.as_ref().is_some_and(Result::is_ok)
    });
    result.unwrap_or_else(|| Err("canvas facet edit did not run".into()))
}

pub(crate) fn edit_graph_facets<R>(
    graph: &mut Graph,
    edit: impl FnOnce(&mut mere::kernel::graph::NodeFacetStore) -> R,
) -> Result<R, String> {
    let before = graph.facets().clone();
    let mut after = before.clone();
    let result = edit(&mut after);
    if before == after {
        return Ok(result);
    }
    let deltas = facet_deltas(graph, &before, &after);
    if deltas.is_empty() {
        graph
            .install_facets(after)
            .map_err(|error| format!("owner graph refused facet store: {error}"))?;
        return Ok(result);
    }

    // Validate the complete edit before touching the owner graph. The live
    // application below replays the same deltas through its recorder.
    let mut candidate = graph.clone();
    candidate.set_recorder(None);
    for delta in &deltas {
        if let GraphDeltaResult::Refused(reason) = apply_graph_delta(&mut candidate, delta.clone())
        {
            return Err(format!("graph refused facet edit: {reason:?}"));
        }
    }
    candidate
        .install_facets(after.clone())
        .map_err(|error| format!("candidate graph refused facet store: {error}"))?;
    for delta in deltas {
        if let GraphDeltaResult::Refused(reason) = apply_graph_delta(graph, delta) {
            return Err(format!(
                "owner graph refused validated facet edit: {reason:?}"
            ));
        }
    }
    // The checked complete sidecar boundary also carries graph-container
    // facets whose key is a session GraphId rather than a member node id.
    graph
        .install_facets(after)
        .map_err(|error| format!("owner graph refused facet store: {error}"))?;
    Ok(result)
}

fn facet_deltas(
    graph: &Graph,
    before: &mere::kernel::graph::NodeFacetStore,
    after: &mere::kernel::graph::NodeFacetStore,
) -> Vec<GraphDelta> {
    let mut node_ids = BTreeSet::new();
    node_ids.extend(before.iter().map(|(node, _)| *node));
    node_ids.extend(after.iter().map(|(node, _)| *node));
    let mut deltas = Vec::new();
    for node_id in node_ids {
        if graph.get_node_by_id(node_id).is_none() {
            continue;
        }
        let old: BTreeMap<FacetId, serde_json::Value> = before
            .facets_of(&node_id)
            .into_iter()
            .flat_map(|facets| facets.iter())
            .map(|(facet, value)| (facet.clone(), value.clone()))
            .collect();
        let new: BTreeMap<FacetId, serde_json::Value> = after
            .facets_of(&node_id)
            .into_iter()
            .flat_map(|facets| facets.iter())
            .map(|(facet, value)| (facet.clone(), value.clone()))
            .collect();
        let mut facets = BTreeSet::new();
        facets.extend(old.keys().cloned());
        facets.extend(new.keys().cloned());
        for facet in facets {
            match (old.get(&facet), new.get(&facet)) {
                (Some(old), Some(new)) if old == new => {},
                (_, Some(value)) => deltas.push(GraphDelta::ReplaySetNodeFacetById {
                    node_id,
                    facet: facet.as_str().into(),
                    value: value.clone(),
                }),
                (Some(_), None) => deltas.push(GraphDelta::ReplayRemoveNodeFacetById {
                    node_id,
                    facet: facet.as_str().into(),
                }),
                (None, None) => unreachable!("facet key came from the old or new store"),
            }
        }
    }
    deltas
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
    use uuid::Uuid;

    #[test]
    fn detached_facet_edits_replay_through_the_graph_recorder() {
        let mut graph = Graph::new();
        let id = Uuid::new_v4();
        apply_graph_delta(
            &mut graph,
            GraphDelta::AddNode {
                id: Some(id),
                url: "turnstone://facet-edit".into(),
                position: Default::default(),
            },
        );
        let mut canvas = Canvas::new();
        canvas.set_graph(graph);
        let result = edit_canvas_facets(&mut canvas, |facets| {
            facets.set(
                id,
                FacetId::new("test.owner"),
                serde_json::json!("value"),
                &chartulary::AcceptAll,
            )
        })
        .unwrap();
        assert!(result.is_ok());
        let graph = canvas.graph();
        assert_eq!(
            graph.facets().get(&id, &FacetId::new("test.owner")),
            Some(&serde_json::json!("value"))
        );
    }
}
