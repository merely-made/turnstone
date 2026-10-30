// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_json_feed(source_url: &str, body: &str) -> Result<ParsedFeed, String> {
    let parsed: JsonFeed =
        serde_json::from_str(body).map_err(|error| format!("JSON Feed parse error: {error}"))?;
    let entries = parsed
        .items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let link = item.url.as_deref().or(item.external_url.as_deref());
            let summary = item
                .summary
                .or(item.content_text)
                .or_else(|| {
                    item.content_html
                        .map(|html| errand::parse::feed::strip_html_tags(&html))
                })
                .filter(|summary| !summary.trim().is_empty());
            Ok(FeedEntry {
                url: entry_url(source_url, link, &item.id, index)?,
                guid: Some(item.id.clone()),
                title: item
                    .title
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| item.id.clone()),
                date: item.date_published,
                summary,
                ..FeedEntry::default()
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ParsedFeed {
        title: parsed.title,
        entries: deduplicate(entries),
        ..ParsedFeed::default()
    })
}
