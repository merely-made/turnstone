// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Exact retained private sources in the canonical graph's encrypted Resources.
//! Titles, selected ranges and source bytes are sealed together. Graph journals
//! receive ciphertext deltas only; no unsealed Resource is a private fallback.

use alembic::workshop::{Owner, Passage, Source};
use athanor::workshop::{self, Error};
use athanor::workshop_seal::Protection;
use mere::kernel::{
    graph::{Graph, apply::GraphDelta},
    persistence::{PersistedResourceFacet, PersistedResourceRecord},
};
use uuid::Uuid;

const FACET: &str = "mere.alembic.retained-private-source/v1";
const TITLE: &str = "Private retained passage";
const BODY: &str = "Private source. Open through the authorized workshop reader.";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    title: String,
    text: String,
    start: u64,
    end: u64,
}

fn identity(owner: &Owner, surface: Uuid) -> (String, Uuid, Vec<u8>) {
    let namespace = blake3::hash(&serde_json::to_vec(owner).expect("owner serializes"));
    let iri = format!(
        "mere://alembic-private-source/{}/{surface}",
        namespace.to_hex()
    );
    let canonical = chartulary::canonical_url(&iri);
    let resource = chartulary::resource_id_from_canonical_iri(&canonical);
    let context =
        serde_json::to_vec(&(FACET, owner, surface, resource)).expect("identity serializes");
    (canonical, resource, context)
}

fn passage(
    owner: &Owner,
    resource: Uuid,
    held: &Held,
    protection: &dyn Protection,
) -> Result<Passage, Error> {
    if held.title.len() > 1024 || held.text.len() > 128 * 1024 {
        return Err(Error::Invalid("private source bound"));
    }
    workshop::capture_private(
        Source {
            owner: owner.clone(),
            resource: resource.to_string(),
            version: blake3::hash(held.text.as_bytes()).to_hex().to_string(),
            content_digest: [0; 32],
            start: held.start,
            end: held.end,
            title: held.title.clone(),
        },
        &held.text,
        protection,
    )
}

/// The owner admits this exact source before calling its serialized graph writer.
/// A source identity is immutable; replacing its bytes requires a new surface.
pub(super) fn prepare(
    graph: &Graph,
    owner: &Owner,
    surface: Uuid,
    title: String,
    text: String,
    start: u64,
    end: u64,
    protection: &dyn Protection,
) -> Result<(Vec<GraphDelta>, u64), Error> {
    let epoch = protection.epoch().ok_or(Error::Unauthorized)?;
    let (iri, resource, context) = identity(owner, surface);
    if graph.get_node_by_id(surface).is_some() || graph.resource(resource).is_some() {
        return Err(Error::Conflict);
    }
    let held = Held {
        title,
        text,
        start,
        end,
    };
    passage(owner, resource, &held, protection)?;
    let clear = serde_json::to_vec(&held).map_err(|_| Error::Invalid("private source encoding"))?;
    let sealed = protection.seal(&context, &clear)?;
    if protection.epoch() != Some(epoch) {
        return Err(Error::Unauthorized);
    }
    Ok((
        vec![
            GraphDelta::AddNode {
                id: Some(surface),
                url: iri.clone(),
                position: Default::default(),
            },
            GraphDelta::ReplaySetNodeTitleById {
                node_id: surface,
                title: TITLE.into(),
            },
            GraphDelta::ReplaySetNodeBodyById {
                node_id: surface,
                body: Some(BODY.into()),
            },
            GraphDelta::ReplaySetResourceRecordById {
                resource_id: resource,
                record: Some(PersistedResourceRecord {
                    canonical_iri: iri,
                    facets: vec![PersistedResourceFacet {
                        facet: FACET.into(),
                        value_json: serde_json::to_string(&sealed)
                            .map_err(|_| Error::Invalid("private source encoding"))?,
                    }],
                }),
            },
            GraphDelta::ReplaySetShownResourceById {
                surface_id: surface,
                resource_id: Some(resource),
            },
        ],
        epoch,
    ))
}

pub(super) fn capture(
    graph: &Graph,
    owner: &Owner,
    surface: Uuid,
    protection: &dyn Protection,
) -> Result<Passage, Error> {
    let epoch = protection.epoch().ok_or(Error::Unauthorized)?;
    let (iri, resource, context) = identity(owner, surface);
    let (key, node) = graph.get_node_by_id(surface).ok_or(Error::Missing)?;
    if node.url() != iri || graph.shown_resource_id(key) != Some(resource) {
        return Err(Error::Conflict);
    }
    let snapshot = graph.to_snapshot_at(0);
    let held_node = snapshot
        .nodes
        .iter()
        .find(|n| n.node_id == surface.to_string())
        .ok_or(Error::Missing)?;
    if held_node.title != TITLE || held_node.body.as_deref() != Some(BODY) {
        return Err(Error::Conflict);
    }
    let record = snapshot
        .resources
        .iter()
        .find(|r| r.canonical_iri == iri)
        .ok_or(Error::Missing)?;
    if record.facets.len() != 1
        || record.facets[0].facet != FACET
        || record.facets[0].value_json.len() > 4 * 1024 * 1024
    {
        return Err(Error::Conflict);
    }
    let sealed: Vec<u8> =
        serde_json::from_str(&record.facets[0].value_json).map_err(|_| Error::Conflict)?;
    let clear = protection.open(&context, &sealed)?;
    let held: Held = serde_json::from_slice(&clear).map_err(|_| Error::Conflict)?;
    let result = passage(owner, resource, &held, protection)?;
    if protection.epoch() != Some(epoch) {
        return Err(Error::Unauthorized);
    }
    Ok(result)
}

pub(super) fn surfaces(graph: &Graph) -> Vec<Uuid> {
    graph
        .nodes()
        .filter_map(|(_, node)| {
            node.url()
                .starts_with("mere://alembic-private-source/")
                .then_some(node.id)
        })
        .collect()
}
