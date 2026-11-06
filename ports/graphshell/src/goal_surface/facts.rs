// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What the goal surface reads off the graph, copied out in one synchronous
//! pass: object titles and addresses, the links between them, and the tag
//! concepts with who holds them. A browser task owns these across a store
//! read, where it cannot hold the graph.

use chartulary::Labeled;
use mere::kernel::graph::resource::TAGGED_WITH_IRI;
use mere::kernel::graph::{Graph, RelationKind};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// An outgoing link between two shown resources, ready to rank candidates by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRow {
    pub from: Uuid,
    pub to: Uuid,
    pub label: Option<String>,
    pub to_url: String,
    pub to_title: Option<String>,
}

/// A tag concept the graph holds: its declared owner, its IRI and its label.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConceptFact {
    pub owner: String,
    pub iri: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphFacts {
    records: BTreeMap<Uuid, (String, Option<String>)>,
    links: Vec<LinkRow>,
    concepts: BTreeSet<ConceptFact>,
    /// (resource, concept IRI) pairs some asserter holds.
    tagged: BTreeSet<(Uuid, String)>,
}

impl GraphFacts {
    pub fn read(graph: &Graph) -> Self {
        let mut facts = Self::default();
        for (key, node) in graph.nodes() {
            if let Some(resource) = graph.shown_resource_id(key) {
                facts
                    .records
                    .entry(resource)
                    .or_insert_with(|| (node.url().to_owned(), Labeled::title(node).map(str::to_owned)));
            }
        }
        facts.read_links(graph);
        facts.read_tags(graph);
        facts
    }

    fn read_links(&mut self, graph: &Graph) {
        // Projected relations are surface edges plus resource-level assertions
        // lifted to the surfaces showing their endpoints.
        for (relation, row) in graph.projected_relations() {
            if !matches!(
                row.kind,
                RelationKind::Semantic(_) | RelationKind::OpenPredicate | RelationKind::Traversal
            ) {
                continue;
            }
            let (Some(from), Some(to), Some(target)) = (
                graph.shown_resource_id(row.from),
                graph.shown_resource_id(row.to),
                graph.get_node(row.to),
            ) else {
                continue;
            };
            if to == from {
                continue;
            }
            let edge_label = graph.get_relation(relation).and_then(|payload| {
                payload.held_semantic_statements().find_map(|statement| match row.kind {
                    RelationKind::Semantic(sub) if statement.recognized_sub_kind == Some(sub) => {
                        Some(statement.label.clone().unwrap_or_else(|| format!("{sub:?}").to_lowercase()))
                    },
                    RelationKind::OpenPredicate if statement.recognized_sub_kind.is_none() => {
                        Some(statement.label.clone().unwrap_or_else(|| statement.predicate.clone()))
                    },
                    _ => None,
                })
            });
            let to_title = Labeled::title(target).map(str::to_owned);
            let label = match (to_title.clone(), edge_label) {
                (Some(t), Some(e)) => Some(format!("{t} {e}")),
                (t, e) => t.or(e),
            };
            self.links.push(LinkRow {
                from,
                to,
                label,
                to_url: target.url().to_owned(),
                to_title,
            });
        }
    }

    fn read_tags(&mut self, graph: &Graph) {
        for resource in graph.resource_nodes() {
            if let Some(concept) = graph.resource_tag_concept(resource.id()) {
                self.concepts.insert(ConceptFact {
                    owner: concept.owner_iri,
                    iri: resource.canonical_iri().to_owned(),
                    label: concept.label,
                });
            }
        }
        for (from, to, payload) in graph.resource_edges() {
            if payload
                .held_semantic_statements()
                .any(|statement| statement.predicate == TAGGED_WITH_IRI)
                && graph.resource_tag_concept(to.id()).is_some()
            {
                self.tagged.insert((from.id(), to.canonical_iri().to_owned()));
            }
        }
    }

    pub fn title(&self, resource: &Uuid) -> Option<String> {
        self.records.get(resource)?.1.clone()
    }

    /// Every shown resource's address and title.
    pub fn records(&self) -> impl Iterator<Item = (&Uuid, &(String, Option<String>))> {
        self.records.iter()
    }

    pub fn links(&self) -> &[LinkRow] {
        &self.links
    }

    pub fn concepts(&self) -> &BTreeSet<ConceptFact> {
        &self.concepts
    }

    /// Whether any asserter holds `concept` on `resource`.
    pub fn is_tagged(&self, resource: &Uuid, concept: &str) -> bool {
        self.tagged.contains(&(*resource, concept.to_owned()))
    }

    pub fn tagged(&self) -> &BTreeSet<(Uuid, String)> {
        &self.tagged
    }

    /// The concept with this IRI and owner, if the graph holds it.
    pub fn concept(&self, owner: &str, iri: &str) -> Option<&ConceptFact> {
        self.concepts.iter().find(|c| c.owner == owner && c.iri == iri)
    }
}
