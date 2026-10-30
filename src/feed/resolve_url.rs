// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn resolve_url(source_url: &str, link: &str) -> Result<String, String> {
    let base = url::Url::parse(source_url).map_err(|error| error.to_string())?;
    base.join(link)
        .map(|url| url.to_string())
        .map_err(|error| format!("feed entry URL {link:?}: {error}"))
}
