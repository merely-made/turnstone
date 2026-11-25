// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{linked_queries::Destination, resident_graph::Share};
use mere_overmap::saved_views::SourceMaterial;
use std::sync::Arc;
use turnstone_ontology::SavedView;

pub(super) fn prepare(
    outcome: &mut Outcome,
    request: &Request,
    view: &SavedView,
    sources: &BTreeMap<String, (String, RecordIndex)>,
    bridge: &Bridge,
) -> Result<(), String> {
    let material = sources
        .iter()
        .map(|(id, (revision, index))| SourceMaterial {
            id: id.clone(),
            revision: revision.clone(),
            statements: index
                .statements()
                .map(|r| [r.subject, r.predicate, r.object])
                .collect(),
        })
        .collect::<Vec<_>>();
    let digest =
        eidetic::Hash::of(&serde_json::to_vec(&material).map_err(|e| e.to_string())?).to_string();
    let (domain, expected) = match &request.operation {
        Operation::Preview(domain) => (domain, None),
        Operation::Share {
            domain,
            session,
            destination,
            expected,
        } => (
            domain,
            Some(Destination {
                domain: domain.clone(),
                scope: destination.clone(),
                session: session.clone(),
                digest: expected.clone(),
                count: 0,
            }),
        ),
        _ => return Ok(()),
    };
    let guard = bridge.clone();
    let stamp = request.stamp.clone();
    let target = bridge.1.send(Share {
        domain: domain.clone(),
        view: view.clone(),
        material,
        digest,
        expected,
        permitted: Arc::new(move || Stamp::current(&guard).ok().as_ref() == Some(&stamp)),
    })?;
    if matches!(request.operation, Operation::Share { .. }) {
        outcome.message = format!(
            "Shared {} to {domain} · session {}",
            view.name, target.session
        );
    } else {
        outcome.destination = Some(target);
        outcome.message = format!(
            "Review {} sources before sharing. Their saved source addresses, revisions and selected vocabulary statements will be copied to the resident.",
            sources.len()
        );
    }
    Ok(())
}
