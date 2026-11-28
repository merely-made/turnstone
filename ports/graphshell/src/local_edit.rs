// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Typed local detail edits, independent of a browser form or presenter.
use crate::product::ProductHost;
use cartography::canvas::Canvas;
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

/// A local detail edit expressed without a presenter or DOM control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetailAction {
    Facet {
        id: String,
        json: String,
    },
    Relation {
        target: String,
        relation: crate::product::EditableRelation,
    },
}

pub fn relation_target<B: Backend>(
    app: &GraphshellApp<B>,
    member: Uuid,
    target: &str,
) -> Result<Uuid, String> {
    metadata(app, member)?;
    app.host
        .graph()
        .get_node_by_url(target.trim())
        .map(|(_, node)| node.id)
        .ok_or_else(|| format!("unknown relation target {}", target.trim()))
}

/// Validate and apply through the existing product owner, then acknowledge its
/// durable write. After an I/O refusal the caller retries persistence of the
/// pending owner state rather than asserting the relation a second time.
pub async fn save_detail_action<B: pandect::graph_session::LegacyTransactionBackend>(
    app: &mut GraphshellApp<B>,
    member: Uuid,
    action: DetailAction,
    saved_at_secs: u64,
) -> Result<NodeMetadata, String> {
    apply_detail_action(app, member, action)?;
    crate::local_intake::persist_intake(app, member, saved_at_secs).await
}

pub fn apply_detail_action<B: Backend>(
    app: &mut GraphshellApp<B>,
    member: Uuid,
    action: DetailAction,
) -> Result<(), String> {
    metadata(app, member)?;
    match action {
        DetailAction::Facet { id, json } => {
            if id.trim().is_empty() {
                return Err("A facet id is required".into());
            }
            app.host
                .set_product_facet(member, &id, &json)
                .map_err(|error| error.to_string())?;
        },
        DetailAction::Relation { target, relation } => {
            let target = relation_target(app, member, &target)?;
            app.host
                .assert_product_relation(member, target, relation)
                .map_err(|error| error.to_string())?;
        },
    }
    Ok(())
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
pub async fn save_metadata<B: pandect::graph_session::LegacyTransactionBackend>(
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
    source: &kernel::graph::Graph,
    saved: &NodeMetadata,
) -> Result<(), String> {
    use kernel::graph::CapturedDelta;
    use mere::model::PersistedEdge;
    use mere::model::resource::TAGGED_WITH_IRI;
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
        edge.retained_assertions()
            .iter()
            .any(|claim| claim.predicate == TAGGED_WITH_IRI)
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
            let id = portolan::resource_id_from_canonical_iri(&record.canonical_iri).to_string();
            if id == resource_id || targets.contains(&id) {
                deltas.push(CapturedDelta::ReplaySetResourceRecordById {
                    resource_id: id,
                    record: Some(record.clone()),
                });
            }
        }
        for target in targets {
            let mut edges: Vec<_> = current
                .resource_edges
                .iter()
                .filter(|edge| edge.from_node_id == resource_id && edge.to_node_id == target)
                .cloned()
                .filter_map(|edge| select_metadata_edge(edge, false))
                .collect();
            edges.extend(
                from.resource_edges
                    .iter()
                    .filter(|edge| edge.from_node_id == resource_id && edge.to_node_id == target)
                    .cloned()
                    .filter_map(|edge| select_metadata_edge(edge, true)),
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

fn select_metadata_edge(
    edge: mere::model::PersistedEdge,
    tags: bool,
) -> Option<mere::model::PersistedEdge> {
    use mere::model::resource::TAGGED_WITH_IRI;

    let mut assertions = edge.retained_assertions().to_vec();
    assertions.retain(|claim| (claim.predicate == TAGGED_WITH_IRI) == tags);
    // `assertions` is the canonical all-family record list. Do not carry
    // legacy per-family summaries beside a filtered copy: they may contain
    // stale sibling kinds or duplicate the same record.
    let edge = mere::model::PersistedEdge::of(edge.from_node_id, edge.to_node_id, assertions);
    (!edge.retained_assertions().is_empty()).then_some(edge)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod exact_record_adapter_tests {
    use super::*;
    use kernel::workspace;
    use mere::model::{AssertionBasis, GraphScope};
    use mere::model::{PersistedEdge, PersistedSemanticStatement};
    use mere::profile::core::ids as core_ids;
    use std::collections::BTreeSet;

    fn assertion(id: &str, predicate: &str) -> PersistedSemanticStatement {
        PersistedSemanticStatement {
            statement_id: id.to_string(),
            predicate: predicate.to_string(),
            label: None,
            graph_scope: GraphScope::Default,
            provenance_iri: Some("urn:test:source".into()),
            asserted_at_ms: None,
            first_asserted_ms: None,
            withdrawn: None,
            basis: AssertionBasis::default(),
        }
    }

    #[test]
    fn metadata_partition_preserves_same_kind_containment_handles() {
        let containment = workspace::iri(core_ids::REL_CONTAINMENT_COLLECTION_MEMBER);
        let families = |edge: &PersistedEdge| -> BTreeSet<_> {
            edge.retained_assertions()
                .iter()
                .map(|claim| workspace::family_of(&claim.predicate))
                .collect()
        };
        let edge = PersistedEdge::of(
            "from-resource".into(),
            "tag-resource".into(),
            vec![
                assertion("tag-1", mere::model::resource::TAGGED_WITH_IRI),
                assertion("containment-1", containment),
                assertion("containment-2", containment),
            ],
        );

        let retained = select_metadata_edge(edge.clone(), false).unwrap();
        assert_eq!(
            retained
                .retained_assertions()
                .iter()
                .map(|assertion| assertion.statement_id.as_str())
                .collect::<Vec<_>>(),
            ["containment-1", "containment-2"]
        );
        assert_eq!(families(&retained), [core_ids::FAMILY_CONTAINMENT].into());
        assert_eq!(retained.assertions.as_ref().unwrap().len(), 2);
        assert!(retained.semantic.is_none());

        let tag_only = select_metadata_edge(edge, true).unwrap();
        assert_eq!(tag_only.retained_assertions()[0].statement_id, "tag-1");
        assert_eq!(families(&tag_only), [core_ids::FAMILY_SEMANTIC].into());
        assert!(tag_only.containment.is_none());

        assert_eq!(
            workspace::id(containment),
            Some(core_ids::REL_CONTAINMENT_COLLECTION_MEMBER)
        );
    }
}
