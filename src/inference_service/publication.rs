// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn publication(
    owner: &str,
    sources: Vec<RetainedSourceText>,
) -> Result<Vec<u8>, String> {
    for source in &sources {
        let target = crate::research::target(&source.source, &source.version)?;
        if source.source.owner.as_deref() != Some(owner)
            || target.text_hash() != eidetic::Hash::of(source.text.as_bytes()).to_string()
        {
            return Err("Saved-source publication failed identity or text validation.".into());
        }
    }
    let bytes = serde_json::to_vec(&PublishCorpusRequestV1 {
        owner: owner.into(),
        sources,
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PUBLISHED_CORPUS_BYTES {
        return Err("This session has too much saved text for Answers. Offline source reading remains available.".into());
    }
    Ok(bytes)
}
