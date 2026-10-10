// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Typed local detail edits, independent of a browser form or presenter.
use mere::canvas::Canvas;
use muniment::Backend;
use uuid::Uuid;

use crate::app::GraphshellApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeMetadata {
    pub member: Uuid,
    pub address: String,
    pub title: String,
    pub tags: Vec<String>,
}

pub fn metadata<B: Backend>(app: &GraphshellApp<B>, member: Uuid) -> Result<NodeMetadata, String> {
    let (key, node) = app
        .host
        .graph()
        .get_node_by_id(member)
        .ok_or_else(|| format!("selected object {member} no longer exists"))?;
    let mut tags = app
        .host
        .graph()
        .node_content_tags(key)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    tags.sort();
    Ok(NodeMetadata {
        member,
        address: node.url().to_string(),
        title: node.title.clone(),
        tags,
    })
}

/// Save the selected object's metadata. Errors leave pending in-memory edits
/// available for retry; callers must not describe a failed write as saved.
pub async fn save_metadata<B: Backend>(
    app: &mut GraphshellApp<B>,
    member: Uuid,
    title: &str,
    tags: &str,
    saved_at_secs: u64,
) -> Result<NodeMetadata, String> {
    metadata(app, member)?;
    app.host
        .edit_node(
            member,
            title,
            tags.split(',').map(str::trim).map(str::to_owned),
        )
        .map_err(|error| error.to_string())?;
    // Refresh the portable projection before the durable write. A projection
    // failure must not become a misleading save failure after bytes committed.
    app.mount_local().map_err(|error| error.to_string())?;
    let saved = metadata(app, member)?;
    app.host
        .persist(saved_at_secs)
        .await
        .map_err(|error| error.to_string())?;
    Ok(saved)
}

/// Stamp the saved metadata into the canvas projection through its existing
/// metadata seams. Geometry, camera, selection and live physics remain owned
/// by this canvas; this does not switch it to a different graph.
pub fn sync_canvas_metadata(canvas: &mut Canvas, saved: &NodeMetadata) -> Result<(), String> {
    let key = canvas
        .graph()
        .get_node_by_id(saved.member)
        .ok_or_else(|| "saved object is no longer in the canvas".to_string())?
        .0;
    let old = canvas
        .graph()
        .node_content_tags(key)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    canvas.set_node_title_for(saved.member, saved.title.clone());
    for tag in old.iter().filter(|tag| !saved.tags.contains(tag)) {
        canvas.untag_node(saved.member, tag);
    }
    for tag in &saved.tags {
        canvas.tag_node(saved.member, tag);
    }
    Ok(())
}

/// Copy hosted tagging claims exactly instead of minting claims from display labels.
pub fn sync_canvas_metadata_from_graph(
    canvas: &mut Canvas,
    source: &mere::kernel::graph::Graph,
    saved: &NodeMetadata,
) -> Result<(), String> {
    use mere::kernel::graph::CapturedDelta;
    use mere::kernel::graph::resource::TAGGED_WITH_IRI;
    use mere::kernel::persistence::{PersistedEdge, PersistedEdgeFamily};
    use std::collections::BTreeSet;
    let (key, node) = source
        .get_node_by_id(saved.member)
        .ok_or("saved object is absent from its source")?;
    if node.title != saved.title {
        return Err("saved title does not match its hosted source".into());
    }
    canvas
        .graph()
        .get_node_by_id(saved.member)
        .ok_or("saved object is no longer in the canvas")?;
    let resource = source.shown_resource_id(key);
    let from = source.to_snapshot();
    let current = canvas.graph().to_snapshot();
    let is_tag = |edge: &PersistedEdge| {
        edge.semantic.as_ref().is_some_and(|bucket| {
            bucket
                .statements
                .iter()
                .any(|claim| claim.predicate == TAGGED_WITH_IRI)
        })
    };
    let mut deltas = vec![CapturedDelta::ReplaySetNodeTitleById {
        node_id: saved.member.to_string(),
        title: saved.title.clone(),
    }];
    if let Some(resource) = resource {
        let resource_id = resource.to_string();
        let targets: BTreeSet<_> = from
            .resource_edges
            .iter()
            .chain(&current.resource_edges)
            .filter(|edge| edge.from_node_id == resource_id && is_tag(edge))
            .map(|edge| edge.to_node_id.clone())
            .collect();
        for record in &from.resources {
            let id = chartulary::resource_id_from_canonical_iri(&record.canonical_iri).to_string();
            if id == resource_id || targets.contains(&id) {
                deltas.push(CapturedDelta::ReplaySetResourceRecordById {
                    resource_id: id,
                    record: Some(record.clone()),
                });
            }
        }
        let select = |mut edge: PersistedEdge, tags: bool| -> Option<PersistedEdge> {
            if tags {
                edge.traversal = None;
                edge.containment = None;
                edge.arrangement = None;
                edge.imported = None;
                edge.provenance = None;
                edge.families
                    .retain(|family| *family == PersistedEdgeFamily::Semantic);
            }
            if let Some(bucket) = &mut edge.semantic {
                bucket
                    .statements
                    .retain(|claim| (claim.predicate == TAGGED_WITH_IRI) == tags);
                bucket.sub_kinds = bucket
                    .statements
                    .iter()
                    .filter_map(|claim| claim.recognized_sub_kind)
                    .collect();
                bucket.predicate = bucket
                    .statements
                    .first()
                    .map(|claim| claim.predicate.clone());
                bucket.label = bucket
                    .statements
                    .iter()
                    .find_map(|claim| claim.label.clone());
                if bucket.statements.is_empty() {
                    edge.semantic = None;
                    edge.families
                        .retain(|family| *family != PersistedEdgeFamily::Semantic);
                }
            }
            (!edge.families.is_empty()).then_some(edge)
        };
        for target in targets {
            let mut edges: Vec<_> = current
                .resource_edges
                .iter()
                .filter(|edge| edge.from_node_id == resource_id && edge.to_node_id == target)
                .cloned()
                .filter_map(|edge| select(edge, false))
                .collect();
            edges.extend(
                from.resource_edges
                    .iter()
                    .filter(|edge| edge.from_node_id == resource_id && edge.to_node_id == target)
                    .cloned()
                    .filter_map(|edge| select(edge, true)),
            );
            deltas.push(CapturedDelta::ReplaySetResourceEdgesByIds {
                from_resource_id: resource_id.clone(),
                to_resource_id: target,
                edges,
            });
        }
    }
    deltas.push(CapturedDelta::ReplaySetShownResourceById {
        surface_id: saved.member.to_string(),
        resource_id: resource.map(|id| id.to_string()),
    });
    canvas.refresh_recorded_metadata(&deltas)?;
    canvas.refresh_semantic_context(source);
    Ok(())
}

#[cfg(test)]
mod tests;
