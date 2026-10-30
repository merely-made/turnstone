// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Build the scene Turnstone discloses: the live graph projected through the
/// spiral score, solved, and carried into routed relations.
///
/// Extracted from the endpoint's `snapshot` so the frozen-projection pane can
/// freeze exactly what a remote peer would be served, rather than a second
/// recipe that drifts from the first.
pub(crate) fn disclose_scene(
    graph: &mere::kernel::graph::Graph,
    focused: Option<NodeKey>,
    card_extent: (f32, f32),
    spiral: sceno::Spiral,
) -> cartography::CoveredScene {
    let extents: HashMap<NodeKey, (f32, f32)> =
        graph.nodes().map(|(key, _)| (key, card_extent)).collect();
    // The spiral reads keyed signals, most recent first (Mere F132). A
    // disclosure is a one-off projection with no canvas behind it, so it
    // takes a fresh registry for this graph.
    let signals = mere::canvas::ChannelRegistry::new().disclose(
        graph,
        &[cartography::ORDER_RECENCY, cartography::WEIGHT_RECENCY],
        focused,
    );
    let mut mapped = cartography::project_spiral_score(
        graph,
        &signals,
        cartography::ORDER_RECENCY,
        Some(&extents),
        focused,
    );
    mapped.score.arrangement = Arrangement::Spiral(spiral);
    let solved = scenomise::solve(&mapped.score);
    let cartography::CoveredScene {
        mut scene,
        coverage,
    } = cartography::scene_from_projection(
        &mapped.projection,
        |key| {
            graph
                .get_node(key)
                .expect("projection key came from the live graph")
                .id
                .to_string()
        },
        |key| extents.get(&key).copied(),
    );
    for ((item, score_item), solved_item) in scene
        .items
        .iter_mut()
        .zip(mapped.score.items.iter())
        .zip(solved.items.iter())
    {
        item.transform = solved_item.transform;
        item.representation = score_item.representation.clone();
        item.layer = score_item.layer;
        item.visible = score_item.visible;
    }
    for relation in &mut scene.relations {
        let from = scene.items[relation.from.0 as usize].transform.translate;
        let to = scene.items[relation.to.0 as usize].transform.translate;
        relation.points = vec![from, to];
    }
    scene.bounds = solved.bounds;
    scene.generation = mapped.score.generation;
    cartography::CoveredScene { scene, coverage }
}
