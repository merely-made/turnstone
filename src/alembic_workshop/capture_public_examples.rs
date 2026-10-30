// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn capture_public_examples(
    graph: &Graph,
    owner: &Owner,
    selected: &[String],
) -> Result<Vec<Passage>, String> {
    if !(2..=4).contains(&selected.len()) {
        return Err("Select two to four public retained passages.".into());
    }
    let mut seen = std::collections::HashSet::new();
    selected
        .iter()
        .map(|key| {
            let surface = Uuid::parse_str(key).map_err(|_| "Unknown public example source")?;
            if !seen.insert(surface) || !fixture_surface(surface) {
                return Err(
                    "The selected source is not in Turnstone's public example corpus.".into(),
                );
            }
            let example = EXAMPLES
                .iter()
                .find(|e| e.id == surface)
                .ok_or("Unknown public example source")?;
            athanor::workshop_destination::capture_graph_public_source(
                graph,
                owner,
                surface,
                0,
                example.body.len() as u64,
                |candidate, resource| fixture_resource(graph, candidate, resource),
            )
            .map_err(|error| error.to_string())
        })
        .collect()
}
