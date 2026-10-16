// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Promoted link Resources: journaled captures and read-only live comparison.
use crate::app::GraphshellApp;
use mere::kernel::graph::{Author, FrozenLink, FrozenLinkSource, Graph, GraphStratum};
use mere::kernel::persistence::PersistedSemanticStatement;
use mere::kernel::types::{GraphScope, NodeProperty};
use muniment::Backend;
use scenograph::link::{LinkKey, LinkSource, LinkStratum};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const REVIEW_COMMENT: &str = "https://mere.computer/ns/rel#reviewComment";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkDrift {
    pub status: String,
    pub added: Vec<String>,
    pub withdrawn: Vec<String>,
    pub changed: Vec<String>,
    pub missing: Vec<String>,
    pub reason: Option<String>,
}

impl LinkDrift {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            status: "unavailable".into(),
            added: vec![],
            withdrawn: vec![],
            changed: vec![],
            missing: vec![],
            reason: Some(reason.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromotedLinkInspection {
    pub owner: Uuid,
    pub frozen: FrozenLink,
    pub coverage: mere::kernel::graph::CoverageNote,
    pub drift: LinkDrift,
    pub comments: Vec<NodeProperty>,
}

fn source(source: &LinkSource) -> Result<FrozenLinkSource, String> {
    serde_json::from_value(serde_json::to_value(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn resolve(graph: &Graph, key: &LinkKey) -> Result<(GraphStratum, Uuid, Uuid), String> {
    if *key != LinkKey::new(key.stratum, key.first.clone(), key.second.clone()) {
        return Err("Reopen the canonical source link before freezing".into());
    }
    let endpoint = |value: &str| -> Result<Uuid, String> {
        match key.stratum {
            LinkStratum::Surface => {
                let id = value
                    .parse::<Uuid>()
                    .map_err(|_| "Invalid Surface endpoint")?;
                if id.to_string() != value || graph.get_node_by_id(id).is_none() {
                    return Err("Surface endpoint is unavailable".into());
                }
                Ok(id)
            },
            LinkStratum::Resource => graph
                .resource_nodes()
                .find(|resource| resource.canonical_iri() == value)
                .map(|resource| resource.id())
                .ok_or_else(|| "Resource endpoint is unavailable".into()),
        }
    };
    Ok((
        match key.stratum {
            LinkStratum::Surface => GraphStratum::Surface,
            LinkStratum::Resource => GraphStratum::Resource,
        },
        endpoint(&key.first)?,
        endpoint(&key.second)?,
    ))
}

pub fn freeze<B: Backend + Clone>(
    app: &mut GraphshellApp<B>,
    key: &LinkKey,
    expected_source: &LinkSource,
    nonce: Uuid,
) -> Result<Uuid, String> {
    if app.host.link_source() != *expected_source {
        return Err("The source changed; reopen the link before freezing".into());
    }
    let (stratum, first, second) = resolve(app.host.graph(), key)?;
    let source = source(expected_source)?;
    // Preflight on a clone before the journal owns any captures.
    let owner = app.host.graph().clone().freeze_link_with_nonce(
        stratum,
        first,
        second,
        source.clone(),
        nonce,
    )?;
    app.host
        .mutate_product_graph(|graph| {
            graph.freeze_link_with_nonce(stratum, first, second, source, nonce)
        })
        .map_err(|e| e.to_string())??;
    Ok(owner)
}

pub fn list(graph: &Graph) -> Vec<Uuid> {
    let mut owners: Vec<_> = graph
        .resource_nodes()
        .filter(|resource| graph.open_frozen_link(resource.id()).is_ok())
        .map(|resource| resource.id())
        .collect();
    owners.sort();
    owners
}

type Copies = BTreeMap<String, (String, String, PersistedSemanticStatement)>;
fn copies(frozen: &FrozenLink) -> Result<Copies, String> {
    let mut copies = BTreeMap::new();
    for edge in frozen.statements() {
        for claim in edge.retained_assertions() {
            if copies
                .insert(
                    claim.statement_id.clone(),
                    (
                        edge.from_node_id.clone(),
                        edge.to_node_id.clone(),
                        claim.clone(),
                    ),
                )
                .is_some()
            {
                return Err("The source repeats an assertion ID; comparison is unavailable".into());
            }
        }
    }
    Ok(copies)
}

pub fn inspect(
    graph: &Graph,
    owner: Uuid,
    current_source: &LinkSource,
) -> Result<PromotedLinkInspection, String> {
    let opened = graph.open_frozen_link(owner)?;
    let frozen = opened.link.clone();
    let coverage = opened.coverage;
    let source_now = source(current_source)?;
    let drift = if !coverage.limits.is_empty() || !frozen.coverage().limits.is_empty() {
        LinkDrift::unavailable("The referenced source material has incomplete coverage")
    } else if !same_owner(frozen.source(), &source_now) {
        LinkDrift::unavailable(
            "The current graph is a different source; load the original source to compare",
        )
    } else {
        match graph.read_link_snapshot(
            frozen.stratum(),
            frozen.first().id,
            frozen.second().id,
            source_now,
        ) {
            Ok(current) => compare(&frozen, &current),
            Err(_) => LinkDrift::unavailable(
                "The current source link is unavailable; its absence is not an empty reading",
            ),
        }
    };
    let comments = graph
        .resource_properties(owner)
        .into_iter()
        .filter(|property| property.predicate == REVIEW_COMMENT)
        .collect();
    Ok(PromotedLinkInspection {
        owner,
        frozen,
        coverage,
        drift,
        comments,
    })
}

fn same_owner(left: &FrozenLinkSource, right: &FrozenLinkSource) -> bool {
    match (left, right) {
        (
            FrozenLinkSource::Available { binding: left, .. },
            FrozenLinkSource::Available { binding: right, .. },
        ) => left == right,
        _ => false,
    }
}

fn compare(frozen: &FrozenLink, current: &FrozenLink) -> LinkDrift {
    let (before, after) = match (copies(frozen), copies(current)) {
        (Ok(before), Ok(after)) => (before, after),
        _ => {
            return LinkDrift::unavailable(
                "The source repeats an assertion ID; comparison is unavailable",
            );
        },
    };
    let mut drift = LinkDrift {
        status: "unchanged".into(),
        added: vec![],
        withdrawn: vec![],
        changed: vec![],
        missing: vec![],
        reason: None,
    };
    for (id, original) in &before {
        match after.get(id) {
            None => drift.missing.push(id.clone()),
            Some(now) => {
                let newly_withdrawn = original.2.withdrawn.is_none() && now.2.withdrawn.is_some();
                if newly_withdrawn {
                    drift.withdrawn.push(id.clone());
                }
                let mut metadata = now.clone();
                if newly_withdrawn {
                    metadata.2.withdrawn = original.2.withdrawn.clone();
                }
                if &metadata != original {
                    drift.changed.push(id.clone());
                }
            },
        }
    }
    drift.added = after
        .keys()
        .filter(|id| !before.contains_key(*id))
        .cloned()
        .collect();
    if !drift.added.is_empty()
        || !drift.withdrawn.is_empty()
        || !drift.changed.is_empty()
        || !drift.missing.is_empty()
    {
        drift.status = "changed".into();
    }
    drift
}

pub fn comment<B: Backend + Clone>(
    app: &mut GraphshellApp<B>,
    owner: Uuid,
    text: &str,
    nonce: Uuid,
    at_ms: u64,
) -> Result<(), String> {
    app.host.graph().open_frozen_link(owner)?;
    if nonce.is_nil() || text.trim().is_empty() || text.len() > 16_384 {
        return Err("Enter a review comment of at most 16384 bytes".into());
    }
    let mut property = NodeProperty::new(REVIEW_COMMENT.into(), text.trim().into());
    property.statement_id = format!("urn:uuid:{nonce}");
    property.graph_scope = GraphScope::User;
    property.asserted_at_ms = Some(at_ms);
    // Attribution comes from the actual journal author inside the edit.
    app.host
        .mutate_product_graph(|graph| graph.append_resource_properties(owner, vec![property]))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn can_withdraw<B: Backend + Clone>(
    app: &GraphshellApp<B>,
    target: &cambium::LinkMemberTarget,
) -> bool {
    if app.host.link_source() != target.source {
        return false;
    }
    let graph = app.host.graph();
    let Some((_, claim)) = graph.find_held_assertion(&target.assertion_id) else {
        return false;
    };
    let actor = Author::person(app.host.selected_persona().persona.clone()).asserter_iri();
    if claim.provenance_iri.as_deref() != Some(actor.as_str())
        || claim.graph_scope != GraphScope::User
        || claim.basis.method.is_some()
    {
        return false;
    }
    let Ok((stratum, a, b)) = resolve(graph, &target.link) else {
        return false;
    };
    let Ok(from) = target.from.parse::<Uuid>() else {
        return false;
    };
    let Ok(to) = target.to.parse::<Uuid>() else {
        return false;
    };
    if BTreeSet::from([a, b]) != BTreeSet::from([from, to]) {
        return false;
    }
    crate::link_inspection::inspect_statement_at_owner(
        graph,
        &target.assertion_id,
        stratum,
        from,
        to,
    )
    .is_ok()
}

pub fn withdraw<B: Backend + Clone>(
    app: &mut GraphshellApp<B>,
    target: &cambium::LinkMemberTarget,
    at_ms: u64,
) -> Result<(), String> {
    if !can_withdraw(app, target) {
        return Err("Only a personal assertion authored by the current persona can be withdrawn at its exact source".into());
    }
    app.host
        .mutate_product_graph(|graph| {
            graph.withdraw_semantic_assertion(&target.assertion_id, Some(at_ms), None)
        })
        .map_err(|e| e.to_string())?
        .then_some(())
        .ok_or_else(|| "The assertion is no longer held".into())
}

#[cfg(test)]
#[path = "promoted_links/tests.rs"]
mod tests;
