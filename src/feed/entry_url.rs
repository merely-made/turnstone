// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn entry_url(
    source_url: &str,
    link: Option<&str>,
    identity: &str,
    index: usize,
) -> Result<String, String> {
    if let Some(link) = link.filter(|link| !link.trim().is_empty()) {
        return resolve_url(source_url, link);
    }
    let mut base = url::Url::parse(source_url).map_err(|error| error.to_string())?;
    let digest = blake3::hash(format!("{index}\0{identity}").as_bytes());
    base.set_fragment(Some(&format!("feed-entry-{}", &digest.to_hex()[..16])));
    Ok(base.to_string())
}
