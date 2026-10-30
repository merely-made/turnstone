// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_xml_feed(source_url: &str, body: &str) -> Result<ParsedFeed, String> {
    let parsed = errand::parse::feed::parse(body).map_err(|error| error.to_string())?;
    let artwork = parsed
        .artwork
        .as_deref()
        .map(|url| resolve_url(source_url, url))
        .transpose()?;
    let entries = parsed
        .entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let identity = entry
                .link
                .clone()
                .or_else(|| entry.title.clone())
                .or_else(|| entry.date.clone())
                .unwrap_or_else(|| "entry".to_string());
            Ok(FeedEntry {
                url: entry_url(source_url, entry.link.as_deref(), &identity, index)?,
                guid: entry.guid,
                title: entry
                    .title
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or(identity),
                date: entry.date,
                summary: entry.summary.filter(|summary| !summary.trim().is_empty()),
                enclosures: entry
                    .enclosures
                    .into_iter()
                    .map(|enclosure| {
                        Ok(FeedEnclosure {
                            url: resolve_url(source_url, &enclosure.url)?,
                            media_type: enclosure.media_type,
                            byte_length: enclosure.byte_length,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                duration: entry.duration,
                artwork: entry
                    .artwork
                    .as_deref()
                    .map(|url| resolve_url(source_url, url))
                    .transpose()?,
                chapters: entry
                    .chapters
                    .into_iter()
                    .map(|resource| {
                        Ok(PodcastResource {
                            url: resolve_url(source_url, &resource.url)?,
                            media_type: resource.media_type,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                transcripts: entry
                    .transcripts
                    .into_iter()
                    .map(|transcript| {
                        Ok(PodcastTranscript {
                            url: resolve_url(source_url, &transcript.url)?,
                            media_type: transcript.media_type,
                            language: transcript.language,
                            rel: transcript.rel,
                        })
                    })
                    .collect::<Result<_, String>>()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ParsedFeed {
        title: parsed.title,
        artwork,
        entries: deduplicate(entries),
        diagnostics: parsed.diagnostics,
    })
}
