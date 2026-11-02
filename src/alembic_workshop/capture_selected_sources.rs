// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
