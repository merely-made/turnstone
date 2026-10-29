// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's owner adapter for Alembic's shared evidence-brief workshop.
//!
//! The owner admits public fixtures and current sealed saved-source versions.
//! Legacy and unverified private captures remain unavailable.

use alembic::workshop::{Owner, Passage, Source};
use alembic::workshop_surface::SourceChoice;
use mere::kernel::graph::{Graph, apply::GraphDelta};
use mere::kernel::persistence::PersistedResourceRecord;
use crate::saved_sources::{self, Admission, Entry, Key, Target};
use athanor::workshop_seal::Protection;
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
                    position: Default::default(),
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
    node.url() == expected_url
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

const MAX_PRIVATE_SOURCE_BYTES: usize = 2048;

fn saved_target(key: &Key, text_hash: &str) -> Target {
    Target {
        key: key.clone(),
        text_hash: text_hash.to_owned(),
        selector: None,
    }
}

fn saved_source_choice_key(entry: &Entry) -> Option<String> {
    let content = entry.content.as_ref()?;
    saved_sources::address(&saved_target(
        &entry.admission.key,
        &content.text_hash,
    ))
    .ok()
}

pub(crate) fn private_source_choices(
    owner: &Owner,
    entries: &[Entry],
    current_admissions: &[Admission],
    custody_open: bool,
) -> Vec<SourceChoice> {
    entries
        .iter()
        .filter(|entry| {
            entry.admission.key.session.as_uuid().to_string() == owner.session
                && entry.admission.key.graph.as_uuid().to_string() == owner.workspace
        })
        .map(|entry| {
            let key = saved_source_choice_key(entry)
                .unwrap_or_else(|| {
                    format!(
                        "private-unavailable:{}:{}",
                        entry.admission.key.node, entry.admission.occurrence
                    )
                });
            let content = entry.content.as_ref();
            let current = current_admissions.iter().any(|admission| {
                admission == &entry.admission && admission.error.is_none()
            });
            let sealed = entry.admission.key.is_sealed();
            let valid_hash = content.is_some_and(|content| {
                eidetic::Hash::parse(&content.text_hash)
                    .is_ok_and(|hash| hash == eidetic::Hash::of(content.text.as_bytes()))
            });
            let bounded = content.is_some_and(|content| {
                !content.text.is_empty() && content.text.len() <= MAX_PRIVATE_SOURCE_BYTES
            });
            let refusal = if !custody_open {
                Some("Unlock the profile to select a sealed saved source.".into())
            } else if !sealed {
                Some("Legacy unsealed saved sources cannot enter a private evidence brief.".into())
            } else if !current || entry.error.is_some() || entry.admission.error.is_some() {
                Some("This saved source is no longer admitted in the current graph.".into())
            } else if !valid_hash {
                Some("The retained source version failed its content hash check.".into())
            } else if !bounded {
                Some("Choose a shorter retained passage (up to 2048 UTF-8 bytes).".into())
            } else {
                None
            };
            let disclose_metadata = custody_open
                && sealed
                && current
                && entry.error.is_none()
                && entry.admission.error.is_none()
                && valid_hash
                && bounded;
            SourceChoice {
                key,
                title: if disclose_metadata {
                    entry
                        .content
                        .as_ref()
                        .map(|content| content.title.clone())
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| entry.admission.title.clone())
                } else {
                    "Private saved source".into()
                },
                version: if disclose_metadata {
                    content
                        .map(|content| {
                            let short_hash: String = content.text_hash.chars().take(12).collect();
                            format!(
                                "Full retained passage · {} bytes · {}",
                                content.text.len(),
                                short_hash
                            )
                        })
                        .unwrap_or_else(|| "unavailable".into())
                } else {
                    "locked".into()
                },
                selected: false,
                available: refusal.is_none(),
                refusal,
            }
        })
        .collect()
}

fn capture_private_saved_source(
    owner: &Owner,
    entry: &Entry,
    current_admissions: &[Admission],
    protection: &dyn Protection,
) -> Result<Passage, String> {
    let key = &entry.admission.key;
    if key.session.as_uuid().to_string() != owner.session
        || key.graph.as_uuid().to_string() != owner.workspace
        || !key.is_sealed()
        || !current_admissions
            .iter()
            .any(|admission| admission == &entry.admission && admission.error.is_none())
        || entry.admission.error.is_some()
        || entry.error.is_some()
    {
        return Err("The selected sealed source is not admitted by this graph and session.".into());
    }
    let content = entry
        .content
        .as_ref()
        .ok_or("The selected saved source has no retained text.")?;
    let parsed_hash = eidetic::Hash::parse(&content.text_hash)
        .map_err(|_| "The retained source version hash is malformed.")?;
    if parsed_hash != eidetic::Hash::of(content.text.as_bytes()) {
        return Err("The retained source version failed its content hash check.".into());
    }
    if content.text.is_empty() || content.text.len() > MAX_PRIVATE_SOURCE_BYTES {
        return Err("Choose a shorter retained passage (up to 2048 UTF-8 bytes).".into());
    }
    let target = saved_target(key, &content.text_hash);
    let resource = saved_sources::address(&target)?;
    let source = Source {
        owner: owner.clone(),
        resource,
        version: content.text_hash.clone(),
        content_digest: [0; 32],
        start: 0,
        end: content.text.len() as u64,
        title: content.title.clone(),
    };
    athanor::workshop::capture_private(source, &content.text, protection)
        .map_err(|error| error.to_string())
}

pub(crate) fn capture_selected_sources(
    graph: &Graph,
    owner: &Owner,
    selected: &[String],
    entries: &[Entry],
    current_admissions: &[Admission],
    protection: &dyn Protection,
) -> Result<Vec<Passage>, String> {
    if !(2..=4).contains(&selected.len()) {
        return Err("Select two to four retained passages.".into());
    }
    let custody_epoch = protection
        .epoch()
        .ok_or("Profile custody is unavailable.")?;
    if custody_epoch == 0 {
        return Err("Profile custody is unavailable.".into());
    }
    let mut seen = std::collections::HashSet::new();
    selected
        .iter()
        .map(|selected| {
            if !seen.insert(selected.as_str()) {
                return Err("A retained source was selected more than once.".into());
            }
            if let Ok(surface) = Uuid::parse_str(selected) {
                let example = EXAMPLES
                    .iter()
                    .find(|example| example.id == surface)
                    .ok_or("The selected source is not in Turnstone's public corpus.")?;
                return athanor::workshop_destination::capture_graph_public_source(
                    graph,
                    owner,
                    surface,
                    0,
                    example.body.len() as u64,
                    |candidate, resource| fixture_resource(graph, candidate, resource),
                )
                .map_err(|error| error.to_string());
            }
            let target = saved_sources::parse_address(selected)?
                .ok_or("The selected source identity is malformed.")?;
            if !target.key.is_sealed() || target.selector.is_some() {
                return Err("Only complete sealed source identities are accepted here.".into());
            }
            let entry = entries
                .iter()
                .find(|entry| {
                    entry.admission.key == target.key
                        && entry.content.as_ref().is_some_and(|content| {
                            content.text_hash == target.text_hash
                                && saved_source_choice_key(entry).as_deref() == Some(selected)
                        })
                })
                .ok_or("The selected saved source version is no longer loaded.")?;
            capture_private_saved_source(owner, entry, current_admissions, protection)
        })
        .collect::<Result<Vec<_>, _>>()
        .and_then(|passages| {
            if protection.epoch() == Some(custody_epoch) {
                Ok(passages)
            } else {
                Err("Profile custody changed while capturing saved sources.".into())
            }
        })
}

pub(crate) fn private_passage_current(
    app: &crate::app::App,
    owner: &Owner,
    protection: &dyn Protection,
    passage: &Passage,
) -> bool {
    let Some(custody_epoch) = protection.epoch() else {
        return false;
    };
    if passage.public
        || passage.source.owner != *owner
        || owner.session != app.session_id.as_uuid().to_string()
        || owner.workspace != app.graph_runtimes.active_graph().as_uuid().to_string()
    {
        return false;
    }
    let Ok(Some(target)) = saved_sources::parse_address(&passage.source.resource) else {
        return false;
    };
    if !target.key.is_sealed()
        || target.selector.is_some()
        || !app.saved_source_admitted(&target.key)
    {
        return false;
    }
    let current_admissions = app.saved_source_admissions();
    let Some(entry) = app.saved_sources.entries.iter().find(|entry| {
        entry.admission.key == target.key
            && current_admissions.iter().any(|admission| {
                admission == &entry.admission && admission.error.is_none()
            })
    }) else {
        return false;
    };
    let Some(content) = entry.content.as_ref() else {
        return false;
    };
    let content_hash = eidetic::Hash::parse(&content.text_hash).ok();
    if entry.error.is_some()
        || entry.admission.error.is_some()
        || content.text_hash != target.text_hash
        || content_hash != Some(eidetic::Hash::of(content.text.as_bytes()))
        || content.text.is_empty()
        || content.text.len() > MAX_PRIVATE_SOURCE_BYTES
        || passage.source.version != content.text_hash
        || passage.source.start != 0
        || passage.source.end != content.text.len() as u64
        || passage.source.title != content.title
        || passage.source.content_digest != *blake3::hash(content.text.as_bytes()).as_bytes()
        || passage.text != content.text
        || passage.text_digest != *blake3::hash(content.text.as_bytes()).as_bytes()
    {
        return false;
    }
    protection.epoch() == Some(custody_epoch)
}

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

/// Resolve a citation from the current persisted proposal rather than looking
/// up the source's latest live bytes. The UI request must name both the exact
/// proposal digest and one of its owner-routed immutable source identities.
pub(crate) fn proposal_reading(
    owner: &Owner,
    view: &alembic::workshop::WorkshopView,
    requested_proposal: alembic::workshop::Digest,
    requested_source: &alembic::workshop::Source,
    protection: Option<&dyn Protection>,
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
    if requested_source.owner != *owner {
        return Err("The citation source belongs to a different owner.".into());
    }
    if proposal.sources.iter().any(|passage| !passage.public) {
        athanor::workshop::validate_private_proposal(proposal)
            .map_err(|error| error.to_string())?;
    } else {
        athanor::workshop::validate_proposal(proposal).map_err(|error| error.to_string())?;
    }
    let passage = proposal
        .sources
        .iter()
        .find(|passage| passage.source == *requested_source)
        .ok_or("The citation source is not in this exact proposal.")?;
    if passage.public {
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
    } else {
        let protection = protection.ok_or("Unlock the profile to open this private citation.")?;
        let epoch = protection
            .epoch()
            .ok_or("Profile custody is unavailable while opening this citation.")?;
        let target = saved_sources::parse_address(&requested_source.resource)?
            .ok_or("The private citation has no saved-source identity.")?;
        let bytes = passage.text.as_bytes();
        if !target.key.is_sealed()
            || target.selector.is_some()
            || requested_source.version != target.text_hash
            || requested_source.start != 0
            || requested_source.end != bytes.len() as u64
            || requested_source.content_digest != *blake3::hash(bytes).as_bytes()
            || passage.text_digest != *blake3::hash(bytes).as_bytes()
            || protection.epoch() != Some(epoch)
        {
            return Err("The retained private citation failed identity or custody validation.".into());
        }
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
    use crate::saved_sources::Content;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestProtection(AtomicU64);

    impl Protection for TestProtection {
        fn epoch(&self) -> Option<u64> {
            match self.0.load(Ordering::SeqCst) {
                0 => None,
                epoch => Some(epoch),
            }
        }
        fn seal(&self, _: &[u8], clear: &[u8]) -> Result<Vec<u8>, athanor::workshop::Error> {
            Ok(clear.to_vec())
        }
        fn open(&self, _: &[u8], sealed: &[u8]) -> Result<Vec<u8>, athanor::workshop::Error> {
            Ok(sealed.to_vec())
        }
    }

    fn sealed_entry(owner: &Owner, manifest: &str, text: &str) -> Entry {
        let annotation = eidetic::Hash::of(manifest.as_bytes()).to_string();
        let key = Key {
            session: crate::panes::SessionId(Uuid::parse_str(&owner.session).unwrap()),
            graph: crate::panes::GraphId::from_uuid(Uuid::parse_str(&owner.workspace).unwrap()),
            node: Uuid::new_v4(),
            annotation_manifest: format!("sealed-v1:{annotation}"),
        };
        let admission = Admission {
            key,
            occurrence: 0,
            title: "Saved title".into(),
            url: "https://private.test/source".into(),
            error: None,
        };
        Entry {
            admission,
            content: Some(Content {
                title: "Café retained copy".into(),
                origin: "https://private.test/source".into(),
                captured_at_ms: 4,
                text: text.into(),
                text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
                raw_manifest: format!("sealed-v1:{annotation}"),
            }),
            error: None,
        }
    }

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

        let reading = proposal_reading(&test_owner, &view, proposal.digest, &source, None).unwrap();
        assert_eq!(reading.text, passages[0].text);
        assert_eq!(reading.version, source.version);
        assert!(proposal_reading(&test_owner, &view, [8; 32], &source, None).is_err());

        let wrong_owner = owner("other-host", Uuid::new_v4(), Uuid::new_v4());
        assert!(proposal_reading(&wrong_owner, &view, proposal.digest, &source, None).is_err());
        let mut wrong_source = source;
        wrong_source.version = "999".into();
        assert!(proposal_reading(&test_owner, &view, proposal.digest, &wrong_source, None).is_err());
    }

    #[test]
    fn public_source_freshness_is_bound_to_exact_session_and_graph() {
        let mut app = crate::app::App::test_stub();
        let graph_id = app.graph_runtimes.active_graph();
        app.graph_runtimes
            .get_mut(graph_id)
            .unwrap()
            .canvas
            .ingest_graph(|graph| {
                for delta in example_seed_deltas(graph) {
                    assert!(!matches!(
                        apply_graph_delta(graph, delta),
                        GraphDeltaResult::Refused(_)
                    ));
                }
                true
            });
        let test_owner = owner(
            "turnstone-public-example",
            *graph_id.as_uuid(),
            *app.session_id.as_uuid(),
        );
        let graph = app.graph_runtimes.get(graph_id).unwrap().canvas.graph();
        let passage = athanor::workshop_destination::capture_graph_public_source(
            graph,
            &test_owner,
            EXAMPLES[0].id,
            0,
            EXAMPLES[0].body.len() as u64,
            |surface, resource| fixture_resource(graph, surface, resource),
        )
        .unwrap();
        assert!(passage_current(&app, &test_owner, &passage));

        let wrong_session = owner(
            &test_owner.host,
            *graph_id.as_uuid(),
            Uuid::new_v4(),
        );
        let mut wrong_session_passage = passage.clone();
        wrong_session_passage.source.owner = wrong_session.clone();
        assert!(!passage_current(
            &app,
            &wrong_session,
            &wrong_session_passage
        ));

        let wrong_graph = owner(
            &test_owner.host,
            Uuid::new_v4(),
            *app.session_id.as_uuid(),
        );
        let mut wrong_graph_passage = passage.clone();
        wrong_graph_passage.source.owner = wrong_graph.clone();
        assert!(!passage_current(&app, &wrong_graph, &wrong_graph_passage));
    }

    #[test]
    fn sealed_saved_source_capture_keeps_exact_utf8_version_and_selection_order() {
        let mut graph = Graph::new();
        for delta in example_seed_deltas(&graph) {
            assert!(!matches!(
                apply_graph_delta(&mut graph, delta),
                GraphDeltaResult::Refused(_)
            ));
        }
        let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        let private_text = "Café 🌱 — exact retained bytes.";
        let entry = sealed_entry(&test_owner, "manifest-one", private_text);
        let admission = entry.admission.clone();
        let private_key = saved_source_choice_key(&entry).unwrap();
        let public_key = EXAMPLES[0].id.to_string();
        let protection = TestProtection(AtomicU64::new(1));
        let passages = capture_selected_sources(
            &graph,
            &test_owner,
            &[private_key.clone(), public_key.clone()],
            std::slice::from_ref(&entry),
            std::slice::from_ref(&admission),
            &protection,
        )
        .unwrap();
        assert_eq!(passages.len(), 2);
        assert!(!passages[0].public);
        assert_eq!(passages[0].text, private_text);
        assert_eq!(passages[0].source.start, 0);
        assert_eq!(passages[0].source.end, private_text.len() as u64);
        assert_eq!(passages[0].source.version, eidetic::Hash::of(private_text.as_bytes()).to_string());
        assert!(passages[1].public);
        assert_eq!(passages[1].source.title, EXAMPLES[0].title);

        assert!(capture_selected_sources(
            &graph,
            &test_owner,
            &[private_key.clone(), private_key.clone()],
            std::slice::from_ref(&entry),
            std::slice::from_ref(&admission),
            &protection,
        )
        .is_err());
        assert!(capture_selected_sources(
            &graph,
            &test_owner,
            &[private_key.clone(), public_key.clone()],
            std::slice::from_ref(&entry),
            &[],
            &protection,
        )
        .is_err());
        let wrong_owner = owner(
            "turnstone-public-example",
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        assert!(capture_selected_sources(
            &graph,
            &wrong_owner,
            &[private_key, public_key],
            std::slice::from_ref(&entry),
            std::slice::from_ref(&admission),
            &protection,
        )
        .is_err());
    }

    #[test]
    fn private_source_choices_refuse_legacy_tampered_and_oversized_entries() {
        let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        let good = sealed_entry(&test_owner, "good", "Short café passage.");
        let legacy = {
            let mut entry = sealed_entry(&test_owner, "legacy", "Legacy text.");
            entry.admission.key.annotation_manifest =
                eidetic::Hash::of(b"legacy").to_string();
            entry
        };
        let mut tampered = sealed_entry(&test_owner, "tampered", "Original text.");
        tampered.content.as_mut().unwrap().text = "Altered text.".into();
        let oversized = sealed_entry(&test_owner, "oversized", &"x".repeat(2049));
        let mut malformed_hash = sealed_entry(&test_owner, "malformed", "Hash text.");
        malformed_hash.content.as_mut().unwrap().text_hash = "ééééééééééééé".into();
        let entries = vec![good.clone(), legacy, tampered, oversized, malformed_hash];
        let admissions: Vec<_> = entries
            .iter()
            .map(|entry| entry.admission.clone())
            .collect();
        let choices = private_source_choices(&test_owner, &entries, &admissions, true);
        assert_eq!(choices.len(), 5);
        assert!(choices[0].available);
        assert!(!choices[1].available);
        assert!(choices[1]
            .refusal
            .as_deref()
            .is_some_and(|reason| reason.contains("Legacy unsealed")));
        assert!(!choices[2].available);
        assert_eq!(choices[2].title, "Private saved source");
        assert!(!choices[3].available);
        assert_eq!(choices[3].title, "Private saved source");
        assert!(choices[3]
            .refusal
            .as_deref()
            .is_some_and(|reason| reason.contains("shorter retained passage")));
        assert!(!choices[4].available);
        let mut different_version = good.clone();
        let content = different_version.content.as_mut().unwrap();
        content.text = "A different version.".into();
        content.text_hash = eidetic::Hash::of(content.text.as_bytes()).to_string();
        assert_eq!(
            saved_source_choice_key(&good) == saved_source_choice_key(&different_version),
            false,
            "different immutable text versions have distinct source identities"
        );
    }

    #[test]
    fn private_proposal_citation_is_exact_and_requires_live_custody() {
        let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        let entry = sealed_entry(&test_owner, "cited", "Café citation text.");
        let protection = TestProtection(AtomicU64::new(1));
        let source_key = saved_source_choice_key(&entry).unwrap();
        let mut graph = Graph::new();
        for delta in example_seed_deltas(&graph) {
            assert!(!matches!(
                apply_graph_delta(&mut graph, delta),
                GraphDeltaResult::Refused(_)
            ));
        }
        let passages = capture_selected_sources(
            &graph,
            &test_owner,
            &[source_key, EXAMPLES[0].id.to_string()],
            std::slice::from_ref(&entry),
            std::slice::from_ref(&entry.admission),
            &protection,
        )
        .unwrap();
        let private_passage = passages[0].clone();
        let public_passage = passages[1].clone();
        let proposal = proposal_for_test([9; 16], 1, &passages);
        athanor::workshop::validate_private_proposal(&proposal).unwrap();
        let view = WorkshopView {
            installation: proposal.installation,
            run: Some(proposal.run),
            generation: 1,
            lifecycle: InstallationStatus::Active,
            phase: RunStatus::Completed,
            review: None,
            proposal: Some(proposal.clone()),
            usage: Usage::default(),
            limits: None,
            controls: Vec::new(),
        };
        let reading = proposal_reading(
            &test_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            Some(&protection),
        )
        .unwrap();
        assert_eq!(reading.text, private_passage.text);
        assert!(proposal_reading(
            &test_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            None,
        )
        .is_err());
        let public_reading = proposal_reading(
            &test_owner,
            &view,
            proposal.digest,
            &public_passage.source,
            None,
        )
        .unwrap();
        assert_eq!(public_reading.text, public_passage.text);
        let wrong_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        assert!(proposal_reading(
            &wrong_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            Some(&protection),
        )
        .is_err());
        protection.0.store(0, Ordering::SeqCst);
        assert!(proposal_reading(
            &test_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            Some(&protection),
        )
        .is_err());
    }

    #[test]
    fn private_passage_freshness_requires_the_current_saved_source_admission() {
        let mut app = crate::app::App::test_stub();
        let graph_id = app.graph_runtimes.active_graph();
        let member = app.graph_runtimes.visit("https://private.test/retained-source");
        let node = app
            .graph_runtimes
            .graph()
            .get_node(member)
            .unwrap()
            .id;
        let mut entry = sealed_entry(
            &owner(
                "turnstone-public-example",
                *graph_id.as_uuid(),
                *app.session_id.as_uuid(),
            ),
            "attached",
            "Private café text.",
        );
        entry.admission.key.session = app.session_id;
        entry.admission.key.graph = graph_id;
        entry.admission.key.node = node;
        let manifest = entry.admission.key.annotation_manifest.clone();
        crate::graph_facets::edit_canvas_facets(
            &mut app.graph_runtimes.get_mut(graph_id).unwrap().canvas,
            |facets| crate::source_capture::append_reference(facets, node, manifest),
        )
        .unwrap()
        .unwrap();
        let graph_node = app
            .graph_runtimes
            .graph()
            .get_node(member)
            .unwrap();
        entry.admission = Admission {
            key: entry.admission.key.clone(),
            occurrence: 0,
            title: graph_node.title.clone(),
            url: graph_node.url().to_owned(),
            error: None,
        };
        app.saved_sources.entries = vec![entry.clone()];
        let current_admissions = app.saved_source_admissions();
        assert_eq!(current_admissions, vec![entry.admission.clone()]);
        let test_owner = owner(
            "turnstone-public-example",
            *graph_id.as_uuid(),
            *app.session_id.as_uuid(),
        );
        let protection = TestProtection(AtomicU64::new(1));
        let passage = capture_private_saved_source(
            &test_owner,
            &entry,
            &current_admissions,
            &protection,
        )
        .unwrap();
        assert!(private_passage_current(
            &app,
            &test_owner,
            &protection,
            &passage
        ));

        let wrong_graph = owner(
            "turnstone-public-example",
            Uuid::new_v4(),
            *app.session_id.as_uuid(),
        );
        assert!(!private_passage_current(
            &app,
            &wrong_graph,
            &protection,
            &passage
        ));
        let wrong_session = owner(
            "turnstone-public-example",
            *graph_id.as_uuid(),
            Uuid::new_v4(),
        );
        assert!(!private_passage_current(
            &app,
            &wrong_session,
            &protection,
            &passage
        ));

        app.saved_sources.entries[0]
            .content
            .as_mut()
            .unwrap()
            .text = "Changed text.".into();
        assert!(!private_passage_current(
            &app,
            &test_owner,
            &protection,
            &passage
        ));
        app.saved_sources.entries = vec![entry];
        crate::graph_facets::edit_canvas_facets(
            &mut app.graph_runtimes.get_mut(graph_id).unwrap().canvas,
            |facets| {
                facets.remove(
                    &node,
                    &chartulary::FacetId::new(
                        crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET,
                    ),
                );
                Ok::<(), String>(())
            },
        )
        .unwrap()
        .unwrap();
        assert!(!private_passage_current(
            &app,
            &test_owner,
            &protection,
            &passage
        ));
        protection.0.store(0, Ordering::SeqCst);
        assert!(!private_passage_current(
            &app,
            &test_owner,
            &protection,
            &passage
        ));
    }
}
