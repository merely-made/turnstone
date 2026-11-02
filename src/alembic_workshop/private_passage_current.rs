// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
            && current_admissions
                .iter()
                .any(|admission| admission == &entry.admission && admission.error.is_none())
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
