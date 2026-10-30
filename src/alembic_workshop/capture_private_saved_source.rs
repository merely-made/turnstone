// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn capture_private_saved_source(
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
