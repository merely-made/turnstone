// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn inference_source(
    store: Option<&FjallStore>,
    dir: &Path,
    source: &eidetic::models::recall::RecallSourceId,
    version: &str,
) -> Result<String, String> {
    use eidetic::models::recall::RecallSourceKind;
    if source.owner.as_deref() != Some(inference_owner(dir)?.as_str())
        || source.kind != RecallSourceKind::PageText
        || source.node.is_some()
    {
        return Err("The cited source belongs to another owner.".into());
    }
    let hash = eidetic::Hash::parse(version).map_err(|_| "The cited source version is invalid.")?;
    let store = store.ok_or("The retained-text owner is not open.")?;
    let reference = eidetic::browsing::text::PageTextRef {
        url: source.source.clone(),
        blob: hash.to_hex(),
        stored_at_ms: 0,
    };
    pollster::block_on(PageTextStore::new(store).text_at(&reference))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "The cited retained version is unavailable.".into())
}
