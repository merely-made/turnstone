// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The favicon URL for a fetched page: the document's declared
/// `<link rel=icon>` href resolved against the page URL, else the well-known
/// `/favicon.ico` for web pages. `None` when neither applies.
pub(super) fn favicon_url_for(
    page_url: &str,
    doc: &genet_static_dom::StaticDocument,
) -> Option<String> {
    let base = url::Url::parse(page_url).ok()?;
    if let Some(href) = linked_icon_href(doc) {
        if let Ok(resolved) = base.join(&href) {
            return Some(resolved.to_string());
        }
    }
    if matches!(base.scheme(), "http" | "https") {
        if let Ok(fallback) = base.join("/favicon.ico") {
            return Some(fallback.to_string());
        }
    }
    None
}
