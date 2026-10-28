// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's owner adapter for Alembic's shared evidence-brief workshop.
//!
//! This first product corpus is deliberately public and synthetic. Existing
//! saved-source captures are private and are shown as unavailable until a
//! qualified private-custody contract exists.

use alembic::workshop::{Owner, Passage};
use alembic::workshop_surface::SourceChoice;
use mere::kernel::graph::{Graph, apply::GraphDelta};
use mere::kernel::persistence::PersistedResourceRecord;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Example {
    id: Uuid,
    family: &'static str,
    version: &'static str,
    title: &'static str,
    body: &'static str,
}

const EXAMPLES: [Example; 3] = [
    Example {
        id: Uuid::from_u128(0xa1eb_1c00_0000_0000_0000_0000_0000_0001),
        family: alembic::workshop::PUBLIC_EXAMPLE[0].0,
        version: alembic::workshop::PUBLIC_EXAMPLE[0].1,
        title: alembic::workshop::PUBLIC_EXAMPLE[0].2,
        body: alembic::workshop::PUBLIC_EXAMPLE[0].3,
    },
    Example {
        id: Uuid::from_u128(0xa1eb_1c00_0000_0000_0000_0000_0000_0002),
        family: alembic::workshop::PUBLIC_EXAMPLE[1].0,
        version: alembic::workshop::PUBLIC_EXAMPLE[1].1,
        title: alembic::workshop::PUBLIC_EXAMPLE[1].2,
        body: alembic::workshop::PUBLIC_EXAMPLE[1].3,
    },
    Example {
        id: Uuid::from_u128(0xa1eb_1c00_0000_0000_0000_0000_0000_0003),
        family: alembic::workshop::PUBLIC_EXAMPLE[2].0,
        version: alembic::workshop::PUBLIC_EXAMPLE[2].1,
        title: alembic::workshop::PUBLIC_EXAMPLE[2].2,
        body: alembic::workshop::PUBLIC_EXAMPLE[2].3,
    },
];

pub(crate) fn example_choices(graph: &Graph) -> Vec<SourceChoice> {
    let snapshot = graph.to_snapshot_at(0);
    EXAMPLES
        .iter()
        .map(|example| {
            let present = graph.get_node_by_id(example.id).is_some_and(|(key, _)| {
                graph.shown_resource_id(key).is_some()
                    && snapshot.nodes.iter().any(|persisted| {
                        persisted.node_id == example.id.to_string()
                            && persisted.title == example.title
                            && persisted.body.as_deref() == Some(example.body)
                    })
            });
            SourceChoice {
                key: example.id.to_string(),
                title: example.title.into(),
                version: example.version.into(),
                selected: false,
                available: present,
                refusal: (!present).then(|| {
                    "Load the public example to retain this version in the current graph.".into()
                }),
            }
        })
        .collect()
}

/// A deterministic set of ordinary graph edits. The owner must apply these to
/// a detached graph, persist the candidate through its existing session writer,
/// and install it only after that save succeeds.
pub(crate) fn example_seed_deltas(graph: &Graph) -> Vec<GraphDelta> {
    EXAMPLES
        .iter()
        .flat_map(|example| {
            let url = format!(
                "turnstone://alembic-example/{}/{}",
                example.family, example.version
            );
            let canonical = chartulary::canonical_url(&url);
            let resource = chartulary::resource_id_from_canonical_iri(&canonical);
            let add = graph
                .get_node_by_id(example.id)
                .is_none()
                .then(|| GraphDelta::AddNode {
                    id: Some(example.id),
                    url,
                    position: euclid::point2(0.0, 0.0),
                });
            std::iter::once(add).flatten().chain([
                GraphDelta::ReplaySetNodeTitleById {
                    node_id: example.id,
                    title: example.title.into(),
                },
                GraphDelta::ReplaySetNodeBodyById {
                    node_id: example.id,
                    body: Some(example.body.into()),
                },
                GraphDelta::ReplaySetNodeMimeHintById {
                    node_id: example.id,
                    mime_hint: Some("text/plain; charset=utf-8".into()),
                },
                GraphDelta::ReplaySetResourceRecordById {
                    resource_id: resource,
                    record: Some(PersistedResourceRecord {
                        canonical_iri: canonical,
                        facets: Vec::new(),
                    }),
                },
                GraphDelta::ReplaySetShownResourceById {
                    surface_id: example.id,
                    resource_id: Some(resource),
                },
            ])
        })
        .collect()
}

pub(crate) fn example_seed_conflict(graph: &Graph) -> bool {
    let snapshot = graph.to_snapshot_at(0);
    EXAMPLES.iter().any(|example| {
        let url = format!(
            "turnstone://alembic-example/{}/{}",
            example.family, example.version
        );
        let canonical = chartulary::canonical_url(&url);
        let resource = chartulary::resource_id_from_canonical_iri(&canonical);
        snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == example.id.to_string())
            .is_some_and(|node| {
                node.url != url
                    || node.title != example.title
                    || node.body.as_deref() != Some(example.body)
            })
            || snapshot
                .resources
                .iter()
                .find(|record| record.canonical_iri == canonical)
                .is_some_and(|record| !record.facets.is_empty())
            || snapshot.shown_resources.iter().any(|shown| {
                shown.resource_id == resource.to_string()
                    && shown.surface_id != example.id.to_string()
            })
    })
}

pub(crate) fn fixture_surface(id: Uuid) -> bool {
    EXAMPLES.iter().any(|example| example.id == id)
}

fn fixture_resource(graph: &Graph, surface: Uuid, resource: Uuid) -> bool {
    let Some(example) = EXAMPLES.iter().find(|example| example.id == surface) else {
        return false;
    };
    let Some((key, node)) = graph.get_node_by_id(surface) else {
        return false;
    };
    let expected_url = format!(
        "turnstone://alembic-example/{}/{}",
        example.family, example.version
    );
    node.url == expected_url
        && graph.shown_resource_id(key) == Some(resource)
        && graph.to_snapshot_at(0).nodes.iter().any(|persisted| {
            persisted.node_id == surface.to_string()
                && persisted.title == example.title
                && persisted.body.as_deref() == Some(example.body)
        })
}

pub(crate) fn owner(host: &str, workspace: Uuid, session: Uuid) -> Owner {
    Owner {
        host: host.into(),
        workspace: workspace.to_string(),
        session: session.to_string(),
    }
}

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

pub(crate) fn passage_current(app: &crate::app::App, owner: &Owner, passage: &Passage) -> bool {
    if passage.source.owner != *owner || !passage.public {
        return false;
    }
    let Some(runtime) = app
        .graph_runtimes
        .iter()
        .find(|runtime| runtime.session == Some(app.session_id))
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

/// Resolve a citation from the current persisted proposal rather than looking
/// up the source's latest live bytes. The UI request must name both the exact
/// proposal digest and one of its owner-routed immutable source identities.
pub(crate) fn proposal_reading(
    owner: &Owner,
    view: &alembic::workshop::WorkshopView,
    requested_proposal: alembic::workshop::Digest,
    requested_source: &alembic::workshop::Source,
) -> Result<alembic::workshop_surface::RetainedReading, String> {
    let proposal = view
        .proposal
        .as_ref()
        .ok_or("No retained proposal is open for citation.")?;
    if view.installation != proposal.installation || view.run != Some(proposal.run) {
        return Err("The retained proposal does not belong to the current run.".into());
    }
    if proposal.digest != requested_proposal {
        return Err("The citation request names a different proposal.".into());
    }
    athanor::workshop::validate_proposal(proposal).map_err(|error| error.to_string())?;
    if requested_source.owner != *owner {
        return Err("The citation source belongs to a different owner.".into());
    }
    let passage = proposal
        .sources
        .iter()
        .find(|passage| passage.source == *requested_source)
        .ok_or("The citation source is not in this exact proposal.")?;
    if !passage.public {
        return Err("This proposal does not contain an admitted public source.".into());
    }
    let expected = EXAMPLES
        .iter()
        .find(|example| {
            let url = format!(
                "turnstone://alembic-example/{}/{}",
                example.family, example.version
            );
            let canonical = chartulary::canonical_url(&url);
            chartulary::resource_id_from_canonical_iri(&canonical).to_string()
                == requested_source.resource
        })
        .ok_or("The proposal source is outside Turnstone's public example corpus.")?;
    let expected_digest = *blake3::hash(expected.body.as_bytes()).as_bytes();
    let expected_version = blake3::hash(expected.body.as_bytes()).to_hex().to_string();
    if requested_source.version != expected_version
        || requested_source.title != expected.title
        || requested_source.content_digest != expected_digest
        || requested_source.start != 0
        || requested_source.end != expected.body.len() as u64
        || passage.text != expected.body
    {
        return Err("The proposal passage does not match its public example version.".into());
    }
    Ok(alembic::workshop_surface::RetainedReading {
        title: passage.source.title.clone(),
        version: passage.source.version.clone(),
        text: passage.text.clone(),
    })
}

fn helper_subject(owner: &Owner) -> servitor::Subject {
    servitor::Subject(
        *blake3::hash(format!("turnstone-public-example-helper-v1/{owner:?}").as_bytes())
            .as_bytes(),
    )
}

fn fixture_authority(
    owner: &Owner,
    installation: [u8; 16],
    human: [u8; 32],
    sources: &[alembic::workshop::Source],
    destination: Option<&alembic::workshop::Destination>,
    now_ms: u64,
) -> servitor::GrantTable {
    use servitor::{AuthorityProvider, Grant, GrantTable, Mode, Subject};
    let helper = helper_subject(owner);
    let mut table = GrantTable::new();
    for source in sources {
        table.grant(Grant::new(
            helper,
            athanor::workshop::read_cap(source),
            Mode::Read,
        ));
    }
    table.grant(Grant::new(
        helper,
        athanor::workshop::proposal_cap(owner, installation),
        Mode::Write,
    ));
    if let Some(destination) = destination {
        table.grant(Grant::new(
            Subject(human),
            athanor::workshop::destination_cap(destination),
            Mode::Write,
        ));
    }
    table.set_now(now_ms);
    table
}

pub(crate) fn authority(
    owner: &Owner,
    installation: [u8; 16],
    human: [u8; 32],
    passages: &[Passage],
    now_ms: u64,
) -> servitor::GrantTable {
    fixture_authority(
        owner,
        installation,
        human,
        &passages
            .iter()
            .map(|p| p.source.clone())
            .collect::<Vec<_>>(),
        None,
        now_ms,
    )
}

pub(crate) fn authority_for_destination(
    owner: &Owner,
    installation: [u8; 16],
    human: [u8; 32],
    destination: &alembic::workshop::Destination,
    sources: &[alembic::workshop::Source],
    now_ms: u64,
) -> servitor::GrantTable {
    fixture_authority(
        owner,
        installation,
        human,
        sources,
        Some(destination),
        now_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alembic::workshop::{InstallationStatus, Proposal, RunStatus, Usage, WorkshopView};
    use mere::kernel::graph::apply::{GraphDeltaResult, apply_graph_delta};

    fn proposal_for_test(installation: [u8; 16], run: u64, sources: &[Passage]) -> Proposal {
        let mut djot = String::from("# Evidence brief\n\n");
        for (index, passage) in sources.iter().enumerate() {
            djot.push_str(&format!("## {}. {}\n\n", index + 1, passage.source.title));
            djot.push_str("> ```\n");
            for line in passage.text.split('\n') {
                djot.push_str(&format!("> {line}\n"));
            }
            djot.push_str("> ```\n\n");
            let source_bytes = serde_json::to_vec(&passage.source).unwrap();
            let source_digest = blake3::hash(&source_bytes);
            let source_hex = source_digest
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            djot.push_str(&format!(
                "[Open retained source {}](alembic-source:{source_hex})\n\n",
                index + 1
            ));
        }
        let mut proposal = Proposal {
            schema: 1,
            definition: athanor::workshop::definition(),
            installation,
            run,
            sources: sources.to_vec(),
            djot,
            digest: [0; 32],
        };
        proposal.digest = *blake3::hash(&serde_json::to_vec(&proposal).unwrap()).as_bytes();
        proposal
    }

    #[test]
    fn changed_example_body_is_refused_by_fresh_public_source_gate() {
        let mut graph = Graph::new();
        for delta in example_seed_deltas(&graph) {
            assert!(!matches!(
                apply_graph_delta(&mut graph, delta),
                GraphDeltaResult::Refused(_)
            ));
        }
        let example = EXAMPLES[0];
        let owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        assert!(fixture_resource(
            &graph,
            example.id,
            graph
                .shown_resource_id(graph.get_node_by_id(example.id).unwrap().0)
                .unwrap(),
        ));

        assert!(!matches!(
            apply_graph_delta(
                &mut graph,
                GraphDelta::ReplaySetNodeBodyById {
                    node_id: example.id,
                    body: Some("private replacement bytes".into()),
                },
            ),
            GraphDeltaResult::Refused(_),
        ));
        let captured = athanor::workshop_destination::capture_graph_public_source(
            &graph,
            &owner,
            example.id,
            0,
            example.body.len() as u64,
            |surface, resource| fixture_resource(&graph, surface, resource),
        );
        assert!(
            captured.is_err(),
            "changed retained bytes cannot keep fixture-public status"
        );
    }

    #[test]
    fn citation_reads_exact_saved_proposal_passage_after_live_source_changes() {
        let mut graph = Graph::new();
        for delta in example_seed_deltas(&graph) {
            assert!(!matches!(
                apply_graph_delta(&mut graph, delta),
                GraphDeltaResult::Refused(_)
            ));
        }
        let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        let selected = [EXAMPLES[0].id.to_string(), EXAMPLES[1].id.to_string()];
        let passages = capture_public_examples(&graph, &test_owner, &selected).unwrap();
        let installation = [9; 16];
        let proposal = proposal_for_test(installation, 1, &passages);
        athanor::workshop::validate_proposal(&proposal).unwrap();
        let source = passages[0].source.clone();
        let view = WorkshopView {
            installation,
            run: Some(1),
            generation: 1,
            lifecycle: InstallationStatus::Active,
            phase: RunStatus::Completed,
            review: None,
            proposal: Some(proposal.clone()),
            usage: Usage::default(),
            limits: None,
            controls: Vec::new(),
        };

        let mut changed_graph = graph.clone();
        let mut changed_body = EXAMPLES[0].body.to_string();
        changed_body.replace_range(..1, "X");
        assert!(!matches!(
            apply_graph_delta(
                &mut changed_graph,
                GraphDelta::ReplaySetNodeBodyById {
                    node_id: EXAMPLES[0].id,
                    body: Some(changed_body),
                },
            ),
            GraphDeltaResult::Refused(_),
        ));
        let stale_capture = athanor::workshop_destination::capture_graph_public_source(
            &changed_graph,
            &test_owner,
            EXAMPLES[0].id,
            source.start,
            source.end,
            |surface, resource| fixture_resource(&changed_graph, surface, resource),
        );
        assert!(stale_capture.is_err());

        let reading = proposal_reading(&test_owner, &view, proposal.digest, &source).unwrap();
        assert_eq!(reading.text, passages[0].text);
        assert_eq!(reading.version, source.version);
        assert!(proposal_reading(&test_owner, &view, [8; 32], &source).is_err());

        let wrong_owner = owner("other-host", Uuid::new_v4(), Uuid::new_v4());
        assert!(proposal_reading(&wrong_owner, &view, proposal.digest, &source).is_err());
        let mut wrong_source = source;
        wrong_source.version = "999".into();
        assert!(proposal_reading(&test_owner, &view, proposal.digest, &wrong_source).is_err());
    }
}
