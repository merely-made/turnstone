// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only access / Resource inspection of the owner's currently held graph.
//! Addresses are labels, never a substitute for the recorded association.
use mere::kernel::graph::{CoverageNote, Graph};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessSummary {
    pub member: Uuid,
    pub title: String,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFact {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceInspection {
    pub id: Uuid,
    pub address: String,
    pub tags: Vec<String>,
    /// Held Resource values, including unknown keys, without schema invention.
    pub facts: Vec<ResourceFact>,
    /// Other held accesses with the same recorded Resource association.
    pub other_accesses: Vec<AccessSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessInspection {
    pub access: AccessSummary,
    /// Retain the association even when the Resource record is unavailable.
    pub resource_id: Option<Uuid>,
    pub resource: Option<ResourceInspection>,
    /// Known limits relevant to this access, its Resource or their held accesses.
    /// Empty never means the listed accesses are globally complete.
    pub coverage: CoverageNote,
}

pub fn inspect_access(graph: &Graph, member: Uuid) -> Result<AccessInspection, String> {
    let (key, node) = graph
        .get_node_by_id(member)
        .ok_or_else(|| format!("The inspected access {member} is no longer held here"))?;
    let access = AccessSummary {
        member,
        title: node.title.clone(),
        address: node.url().to_owned(),
    };
    let resource_id = graph.shown_resource_id(key);
    let held_accesses = resource_id
        .map(|id| graph.surface_ids_showing_resource(id))
        .unwrap_or_else(|| vec![member]);
    let resource = resource_id.and_then(|id| {
        let source = graph.resource(id)?;
        let mut tags: Vec<_> = graph.resource_tag_labels(id).into_iter().collect();
        tags.sort();
        let mut facts: Vec<_> = graph
            .resource_facets()
            .facets_of(&id)
            .into_iter()
            .flat_map(|values| values.iter())
            .map(|(key, value)| ResourceFact {
                key: key.as_str().to_owned(),
                value: value.clone(),
            })
            .collect();
        facts.sort_by(|a, b| a.key.cmp(&b.key));
        let mut other_accesses: Vec<_> = held_accesses
            .iter()
            .filter(|id| **id != member)
            .filter_map(|id| graph.get_node_by_id(*id))
            .map(|(_, node)| AccessSummary {
                member: node.id,
                title: node.title.clone(),
                address: node.url().to_owned(),
            })
            .collect();
        other_accesses.sort_by_key(|access| access.member);
        Some(ResourceInspection {
            id,
            address: source.canonical_iri().to_owned(),
            tags,
            facts,
            other_accesses,
        })
    });
    let mut coverage = graph.coverage_note();
    coverage.limits.retain(|limit| {
        (limit.resources.is_empty() && limit.surfaces.is_empty())
            || resource_id.is_some_and(|id| limit.resources.contains(&id))
            || limit.surfaces.iter().any(|id| held_accesses.contains(id))
    });
    Ok(AccessInspection {
        access,
        resource_id,
        resource,
        coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
    use mere::kernel::graph::{CoverageLayer, CoverageLimit, ResourceNode};
    use mere::kernel::persistence::{PersistedResourceFacet, PersistedResourceRecord};

    fn graph() -> (Graph, Uuid, Uuid) {
        let mut graph = Graph::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        for (member, title) in [(a, "First access"), (b, "Second access")] {
            let key = add_node(
                &mut graph,
                Some(member),
                "https://example.org/shared".into(),
                Default::default(),
            );
            apply_graph_delta(
                &mut graph,
                GraphDelta::SetNodeTitle {
                    key,
                    title: title.into(),
                },
            );
        }
        (graph, a, b)
    }

    #[test]
    fn shared_source_keeps_distinct_accesses_and_inspection_is_read_only() {
        let (graph, a, b) = graph();
        let before = serde_json::to_value(graph.to_snapshot()).unwrap();
        let first = inspect_access(&graph, a).unwrap();
        let second = inspect_access(&graph, b).unwrap();
        assert_eq!(first.resource_id, second.resource_id);
        assert_ne!(first.access, second.access);
        assert_eq!(first.resource.unwrap().other_accesses, vec![second.access]);
        assert_eq!(second.resource.unwrap().other_accesses, vec![first.access]);
        assert_eq!(serde_json::to_value(graph.to_snapshot()).unwrap(), before);
    }

    #[test]
    fn equal_address_does_not_invent_a_missing_association() {
        let (mut graph, a, b) = graph();
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetShownResourceById {
                surface_id: a,
                resource_id: None,
            },
        );
        let inspected = inspect_access(&graph, a).unwrap();
        assert!(inspected.resource_id.is_none());
        assert!(inspected.resource.is_none());
        assert!(inspect_access(&graph, b).unwrap().resource.is_some());
    }

    #[test]
    fn reinspection_follows_live_binding_and_preserves_the_access_target() {
        let (mut graph, a, b) = graph();
        let before = inspect_access(&graph, a).unwrap();
        let key = graph.get_node_by_id(a).unwrap().0;
        apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key,
                url: "https://example.org/new-source".into(),
            },
        );
        let after = inspect_access(&graph, a).unwrap();
        assert_eq!(before.access.member, after.access.member);
        assert_ne!(before.resource_id, after.resource_id);
        assert!(after.resource.unwrap().other_accesses.is_empty());
        assert!(
            inspect_access(&graph, b)
                .unwrap()
                .resource
                .unwrap()
                .other_accesses
                .is_empty()
        );
    }

    #[test]
    fn coverage_is_scoped_and_never_turns_held_accesses_into_a_complete_world() {
        let (mut graph, a, b) = graph();
        let resource = inspect_access(&graph, a).unwrap().resource_id.unwrap();
        let mut related =
            CoverageLimit::new(CoverageLayer::Residency, "more source accesses may exist");
        related.resources.push(resource);
        let mut sibling =
            CoverageLimit::new(CoverageLayer::Disclosure, "sibling is partially disclosed");
        sibling.surfaces.push(b);
        let mut unrelated = CoverageLimit::new(CoverageLayer::Residency, "unrelated source");
        unrelated.resources.push(Uuid::from_u128(900));
        let global = CoverageLimit::new(CoverageLayer::Synchronization, "offline");
        graph.set_known_coverage(CoverageNote {
            limits: vec![related.clone(), sibling.clone(), unrelated, global.clone()],
        });
        assert_eq!(
            inspect_access(&graph, a).unwrap().coverage.limits,
            vec![related, sibling, global]
        );
    }

    #[test]
    fn resource_facts_roundtrip_and_refresh_across_both_accesses() {
        let (mut graph, a, b) = graph();
        let resource = ResourceNode::new("https://example.org/shared").id();
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetResourceRecordById {
                resource_id: resource,
                record: Some(PersistedResourceRecord {
                    canonical_iri: "https://example.org/shared".into(),
                    facets: vec![PersistedResourceFacet {
                        facet: "example.author".into(),
                        value_json: "{\"name\":\"Ada\"}".into(),
                    }],
                }),
            },
        );
        let first = inspect_access(&graph, a).unwrap();
        let second = inspect_access(&graph, b).unwrap();
        assert_eq!(
            first.resource.as_ref().unwrap().facts,
            second.resource.as_ref().unwrap().facts
        );
        assert_eq!(
            first.resource.as_ref().unwrap().facts[0].value,
            serde_json::json!({"name":"Ada"})
        );
        let reopened = Graph::from_snapshot(&graph.to_snapshot());
        assert_eq!(inspect_access(&reopened, a).unwrap(), first);
        let serialized = serde_json::to_string(&first).unwrap();
        assert_eq!(
            serde_json::from_str::<AccessInspection>(&serialized).unwrap(),
            first
        );
    }

    #[test]
    fn absent_target_never_resolves_to_an_equal_address() {
        let (mut graph, a, _) = graph();
        let key = graph.get_node_by_id(a).unwrap().0;
        apply_graph_delta(&mut graph, GraphDelta::RemoveNode { key });
        add_node(
            &mut graph,
            Some(Uuid::from_u128(99)),
            "https://example.org/shared".into(),
            Default::default(),
        );
        assert!(inspect_access(&graph, a).is_err());
    }
}
