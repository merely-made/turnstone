// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_gemtext_feed(source_url: &str, body: &str) -> Result<ParsedFeed, String> {
    let mut title = None;
    let mut entries = Vec::new();
    let mut preformatted = false;
    for line in body.lines() {
        if line.starts_with("```") {
            preformatted = !preformatted;
            continue;
        }
        if preformatted {
            continue;
        }
        if title.is_none()
            && let Some(heading) = line.strip_prefix("# ")
        {
            title = Some(heading.trim().to_string());
        }
        let Some(link) = line.strip_prefix("=>") else {
            continue;
        };
        let mut fields = link.split_whitespace();
        let Some(href) = fields.next() else {
            continue;
        };
        let Some(date) = fields.next().filter(|date| gemini_feed_date(date)) else {
            continue;
        };
        let label = fields.collect::<Vec<_>>().join(" ");
        let url = resolve_url(source_url, href)?;
        entries.push(FeedEntry {
            title: (!label.is_empty())
                .then_some(label)
                .unwrap_or_else(|| url.clone()),
            url,
            date: Some(date.to_string()),
            summary: None,
            ..FeedEntry::default()
        });
    }
    if entries.is_empty() {
        return Err("document contains no Gemini feed entries".to_string());
    }
    Ok(ParsedFeed {
        title,
        entries: deduplicate(entries),
        ..ParsedFeed::default()
    })
}
