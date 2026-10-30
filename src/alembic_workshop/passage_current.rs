// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn passage_current(app: &crate::app::App, owner: &Owner, passage: &Passage) -> bool {
    if passage.source.owner != *owner
        || !passage.public
        || owner.session != app.session_id.as_uuid().to_string()
    {
        return false;
    }
    let Ok(workspace) = Uuid::parse_str(&owner.workspace) else {
        return false;
    };
    let graph = crate::panes::GraphId::from_uuid(workspace);
    let Some(runtime) = app
        .graph_runtimes
        .get(graph)
        .filter(|runtime| runtime.session == Some(app.session_id))
    else {
        return false;
    };
    EXAMPLES.iter().any(|example| {
        athanor::workshop_destination::capture_graph_public_source(
            runtime.canvas.graph(),
            owner,
            example.id,
            passage.source.start,
            passage.source.end,
            |surface, resource| fixture_resource(runtime.canvas.graph(), surface, resource),
        )
        .is_ok_and(|current| current == *passage)
    })
}
