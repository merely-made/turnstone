// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
            return Err(
                "The retained private citation failed identity or custody validation.".into(),
            );
        }
    }
    Ok(alembic::workshop_surface::RetainedReading {
        title: passage.source.title.clone(),
        version: passage.source.version.clone(),
        text: passage.text.clone(),
    })
}
