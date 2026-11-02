// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn parse_document(source_url: &str, fetched: &FetchedPage) -> Result<ParsedFeed, String> {
    let media_type = fetched
        .content_type
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let sniffed = fetched.body.trim_start();
    if media_type == "application/feed+json"
        || media_type == "application/json"
        || sniffed.starts_with('{')
    {
        return parse_json_feed(source_url, &fetched.body);
    }
    if media_type.contains("rss")
        || media_type.contains("atom")
        || media_type.ends_with("+xml")
        || sniffed.starts_with("<rss")
        || sniffed.starts_with("<feed")
        || sniffed.starts_with("<?xml")
    {
        return parse_xml_feed(source_url, &fetched.body);
    }
    parse_gemtext_feed(source_url, &fetched.body)
}
