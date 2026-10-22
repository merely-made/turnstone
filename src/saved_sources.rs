// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! App-owned identities and projections for private saved source documents.
//!
//! Admission is supplied by the current session's live node facets. This
//! module never enumerates the backing store to discover candidate sources.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const ENGINE_ID: &str = "turnstone.saved-source";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    pub session: crate::panes::SessionId,
    pub graph: crate::panes::GraphId,
    pub node: uuid::Uuid,
    pub annotation_manifest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Admission {
    pub key: Key,
    pub occurrence: usize,
    pub title: String,
    pub url: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    pub title: String,
    pub origin: String,
    pub captured_at_ms: u64,
    pub text: String,
    pub text_hash: String,
    pub raw_manifest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub admission: Admission,
    pub content: Option<Content>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Selector {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub key: Key,
    pub text_hash: String,
    pub selector: Option<Selector>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub target: Target,
    pub title: String,
    pub origin: String,
    pub captured_at_ms: u64,
    pub excerpt: String,
}

/// Clip the exact canonical version and scalar range already held by the
/// private reader. Its bytes are a derived observed representation, never a
/// fabricated raw HTTP response. Knot owns their subsequent retention.
pub fn clip(content: &Content, target: &Target) -> Result<inker::DocumentClip, String> {
    let quote = validate_selector(content, target)?
        .ok_or("Choose an exact saved passage before clipping to a research note.")?;
    let selector = target
        .selector
        .as_ref()
        .ok_or("The saved passage has no range.")?;
    let mut whole = target.clone();
    whole.selector = None;
    Ok(inker::DocumentClip {
        source_url: address(target)?,
        title: Some(content.title.clone()),
        text: quote,
        selector: Some(
            serde_json::json!({
                "type":"text-position", "start":selector.start, "end":selector.end
            })
            .to_string(),
        ),
        links: Vec::new(),
        artifacts: vec![inker::DocumentClipArtifact {
            role: inker::DocumentClipArtifactRole::ObservedRepresentation,
            media_type: "text/plain; charset=utf-8; profile=saved-source".into(),
            canonical_uri: address(&whole)?,
            bytes: content.text.as_bytes().to_vec(),
        }],
    })
}

/// Return one hit per admitted source identity, preserving first occurrence
/// order for list mode and result tie ordering. Search is exact-case substring
/// matching over canonical text; selectors count Unicode scalar values.
pub fn search(entries: &[Entry], query: &str, limit: usize) -> Vec<Hit> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut hits = Vec::new();
    for entry in entries {
        if !seen.insert(entry.admission.key.clone()) {
            continue;
        }
        let Some(content) = entry.content.as_ref() else {
            continue;
        };
        if entry.admission.error.is_some() || entry.error.is_some() {
            continue;
        }
        let Ok(actual) = eidetic::Hash::parse(&content.text_hash) else {
            continue;
        };
        if actual != eidetic::Hash::of(content.text.as_bytes()) {
            continue;
        }

        let (selector, excerpt) = if query.is_empty() {
            (None, content.title.clone())
        } else {
            if let Some(byte_start) = content.text.find(query) {
                let byte_end = byte_start + query.len();
                let start = content.text[..byte_start].chars().count();
                let end = start + content.text[byte_start..byte_end].chars().count();
                (Some(Selector { start, end }), query.to_owned())
            } else {
                let metadata = if content.title.contains(query) {
                    Some(content.title.as_str())
                } else if content.origin.contains(query) {
                    Some(content.origin.as_str())
                } else {
                    None
                };
                let Some(metadata) = metadata else {
                    continue;
                };
                (None, metadata.to_owned())
            }
        };
        hits.push(Hit {
            target: Target {
                key: entry.admission.key.clone(),
                text_hash: content.text_hash.clone(),
                selector,
            },
            title: if content.title.is_empty() {
                entry.admission.title.clone()
            } else {
                content.title.clone()
            },
            origin: content.origin.clone(),
            captured_at_ms: content.captured_at_ms,
            excerpt,
        });
        if hits.len() == limit {
            break;
        }
    }
    hits
}

/// Validate the exact content revision and optional scalar selector, returning
/// the selected quote when one is present.
pub fn validate_selector(content: &Content, target: &Target) -> Result<Option<String>, String> {
    let actual = eidetic::Hash::of(content.text.as_bytes()).to_string();
    if actual != content.text_hash || target.text_hash != content.text_hash {
        return Err("saved source text version does not match the selected target".into());
    }
    let Some(selector) = target.selector.as_ref() else {
        return Ok(None);
    };
    let chars: Vec<char> = content.text.chars().collect();
    if selector.start >= selector.end || selector.end > chars.len() {
        return Err("saved source selector is outside the retained text".into());
    }
    Ok(Some(chars[selector.start..selector.end].iter().collect()))
}

/// Build the internal saved-source address used as persisted graph-view intent.
pub fn address(target: &Target) -> Result<String, String> {
    let annotation = eidetic::Hash::parse(&target.key.annotation_manifest)
        .map_err(|_| "saved source annotation identity is malformed")?;
    if annotation.func() != eidetic::Hash::of(b"").func() {
        return Err("saved source annotation identity must use BLAKE3".into());
    }
    let text_hash = eidetic::Hash::parse(&target.text_hash)
        .map_err(|_| "saved source text hash is malformed")?;
    if text_hash.func() != eidetic::Hash::of(b"").func() {
        return Err("saved source text hash must use BLAKE3".into());
    }
    let base = format!(
        "mere://saved-source/{}/{}/{}/{}/{}",
        target.key.session.0,
        target.key.graph.0,
        target.key.node,
        annotation.to_hex(),
        text_hash.to_hex(),
    );
    match target.selector.as_ref() {
        Some(selector) if selector.start < selector.end => Ok(format!(
            "{base}?start={}&end={}",
            selector.start, selector.end
        )),
        Some(_) => Err("saved source selector must be a nonempty half-open range".into()),
        None => Ok(base),
    }
}

/// Parse only the canonical internal saved-source address form.
pub fn parse_address(value: &str) -> Result<Option<Target>, String> {
    if !value.starts_with("mere://saved-source") {
        return Ok(None);
    }
    let url = url::Url::parse(value).map_err(|_| "saved source address is malformed")?;
    if url.scheme() != "mere" || url.host_str() != Some("saved-source") {
        return Ok(None);
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err("saved source address has an unsupported authority or fragment".into());
    }
    let parts: Vec<_> = url
        .path_segments()
        .ok_or("saved source address has no path")?
        .collect();
    if parts.len() != 5 {
        return Err("saved source address has the wrong path shape".into());
    }
    let session = uuid::Uuid::parse_str(parts[0]).map_err(|_| "invalid saved session id")?;
    let graph = uuid::Uuid::parse_str(parts[1]).map_err(|_| "invalid saved graph id")?;
    let node = uuid::Uuid::parse_str(parts[2]).map_err(|_| "invalid saved node id")?;
    let annotation = parse_hex_hash(parts[3])?;
    let text_hash = parse_hex_hash(parts[4])?;
    let selector = match url.query() {
        None => None,
        Some(_) => {
            let mut start = None;
            let mut end = None;
            for (name, value) in url.query_pairs() {
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| "saved source selector is not an integer")?;
                match name.as_ref() {
                    "start" if start.is_none() => start = Some(parsed),
                    "end" if end.is_none() => end = Some(parsed),
                    _ => {
                        return Err(
                            "saved source address has unknown or duplicate parameters".into()
                        );
                    },
                }
            }
            let selector = Selector {
                start: start.ok_or("saved source selector is missing start")?,
                end: end.ok_or("saved source selector is missing end")?,
            };
            if selector.start >= selector.end {
                return Err("saved source selector must be a nonempty half-open range".into());
            }
            Some(selector)
        },
    };
    let target = Target {
        key: Key {
            session: crate::panes::SessionId(session),
            graph: crate::panes::GraphId(graph),
            node,
            annotation_manifest: annotation.to_string(),
        },
        text_hash: text_hash.to_string(),
        selector,
    };
    if address(&target)? != value {
        return Err("saved source address is not canonical".into());
    }
    Ok(Some(target))
}

fn parse_hex_hash(hex: &str) -> Result<eidetic::Hash, String> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("saved source address contains a noncanonical hash".into());
    }
    eidetic::Hash::parse(&format!("blake3:{hex}"))
        .map_err(|_| "saved source hash is malformed".into())
}

// Keep each display paragraph well below a shaper's 16-bit glyph counters.
// This is presentation segmentation only: hashes and selectors keep using the
// complete canonical stream, and concatenating the parts reproduces every byte.
const DISPLAY_BLOCK_BYTES: usize = 4096;
fn display_parts(mut text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    while !text.is_empty() {
        let mut end = text.len().min(DISPLAY_BLOCK_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end < text.len() {
            if let Some((boundary, whitespace)) = text[..end]
                .char_indices()
                .rfind(|(_, ch)| ch.is_whitespace())
                && boundary > 0
            {
                end = boundary + whitespace.len_utf8();
            }
        }
        let (part, rest) = text.split_at(end);
        parts.push(part);
        text = rest;
    }
    parts
}

/// Lower an already admitted canonical text version into the shared document
/// packet. The original URL is represented as an explicit link and is never
/// used as the document address, so opening this packet cannot fetch it.
pub fn document(content: &Content, target: &Target) -> Result<inker::EngineDocument, String> {
    let selected = validate_selector(content, target)?;
    let internal_address = address(target)?;
    let mut blocks = Vec::new();
    blocks.push(inker::Block::Heading {
        level: 1,
        spans: vec![inker::InlineSpan::Text(content.title.clone())],
    });
    for metadata in [
        format!("Saved from {}", content.origin),
        format!("Captured {}", crate::place::utc_ms(content.captured_at_ms)),
        format!(
            "Saved version: {}",
            target
                .key
                .annotation_manifest
                .strip_prefix("blake3:")
                .unwrap_or(&target.key.annotation_manifest)
                .chars()
                .take(12)
                .collect::<String>()
        ),
    ] {
        blocks.push(inker::Block::Paragraph {
            spans: vec![inker::InlineSpan::Text(metadata)],
        });
    }
    if let Some(quote) = selected {
        blocks.push(inker::Block::Quote {
            blocks: display_parts(&quote)
                .into_iter()
                .map(|part| inker::Block::Paragraph {
                    spans: vec![inker::InlineSpan::Strong(vec![inker::InlineSpan::Text(
                        part.to_owned(),
                    )])],
                })
                .collect(),
        });
    }
    blocks.extend(
        display_parts(&content.text)
            .into_iter()
            .map(|part| inker::Block::Paragraph {
                spans: vec![inker::InlineSpan::Text(part.to_owned())],
            }),
    );
    blocks.push(inker::Block::Paragraph {
        spans: vec![inker::InlineSpan::Link {
            url: content.origin.clone(),
            title: Some("Open original address".into()),
            spans: vec![inker::InlineSpan::Text("Open original address".into())],
            predicate: None,
        }],
    });
    Ok(inker::EngineDocument {
        address: internal_address.clone(),
        title: Some(content.title.clone()),
        content_type: "text/plain; profile=saved-source".into(),
        lang: None,
        provenance: inker::DocumentProvenance {
            source_kind: Some(ENGINE_ID.into()),
            canonical_uri: Some(internal_address),
            fetched_at: None,
            source_label: Some(content.origin.clone()),
        },
        trust: inker::DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        navigation: Default::default(),
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(node: uuid::Uuid, annotation: &str) -> Key {
        Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node,
            annotation_manifest: annotation.into(),
        }
    }

    fn entry(key: Key, occurrence: usize, text: &str, title: &str) -> Entry {
        Entry {
            admission: Admission {
                key,
                occurrence,
                title: title.into(),
                url: format!("https://saved.test/{title}"),
                error: None,
            },
            content: Some(Content {
                title: title.into(),
                origin: format!("https://saved.test/{title}"),
                captured_at_ms: 42,
                text: text.into(),
                text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
                raw_manifest: "raw".into(),
            }),
            error: None,
        }
    }

    #[test]
    fn search_keeps_equal_text_provenance_separate_and_deduplicates_occurrences() {
        let node = uuid::Uuid::new_v4();
        let first_key = key(
            node,
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let mut first = entry(first_key.clone(), 0, "before 🐦 target after", "First");
        first.content.as_mut().unwrap().origin = "https://first.test".into();
        let duplicate = entry(first_key, 3, "before 🐦 target after", "Duplicate");
        let second_key = key(
            uuid::Uuid::new_v4(),
            "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        let mut second = entry(second_key, 1, "before 🐦 target after", "Second");
        second.content.as_mut().unwrap().origin = "https://second.test".into();
        let hits = search(&[first, duplicate, second], "🐦 target", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0].target.selector,
            Some(Selector { start: 7, end: 15 })
        );
        assert_eq!(hits[0].origin, "https://first.test");
        assert_eq!(hits[1].origin, "https://second.test");
    }

    #[test]
    fn internal_address_round_trips_and_rejects_noncanonical_routes() {
        let target = Target {
            key: key(
                uuid::Uuid::new_v4(),
                "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            text_hash: eidetic::Hash::of("hello 🐦".as_bytes()).to_string(),
            selector: Some(Selector { start: 6, end: 7 }),
        };
        let value = address(&target).unwrap();
        assert_eq!(parse_address(&value).unwrap(), Some(target));
        assert_eq!(parse_address("https://example.test").unwrap(), None);
        assert!(parse_address(&value.replace("start=6", "start=6&extra=1")).is_err());
        assert!(parse_address(&value.replace("start=6", "start=06")).is_err());
    }

    #[test]
    fn document_keeps_original_address_as_an_explicit_link() {
        let entry = entry(
            key(
                uuid::Uuid::new_v4(),
                "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            0,
            "the full retained body",
            "Capture title",
        );
        let content = entry.content.unwrap();
        let target = Target {
            key: entry.admission.key,
            text_hash: content.text_hash.clone(),
            selector: Some(Selector { start: 4, end: 8 }),
        };
        let rendered = document(&content, &target).unwrap();
        assert_eq!(rendered.address, address(&target).unwrap());
        assert!(rendered.blocks.iter().any(|block| matches!(
            block,
            inker::Block::Paragraph { spans }
                if spans.iter().any(|span| matches!(span, inker::InlineSpan::Link { url, .. } if url == &content.origin))
        )));
    }
    #[test]
    fn long_reader_body_and_quote_are_bounded_without_truncating_canonical_unicode() {
        let text = "naïve — 東京 🐦 \\ ".repeat(10000);
        let entry = entry(
            key(
                uuid::Uuid::new_v4(),
                &eidetic::ManifestId::of_blob(b"large").to_string(),
            ),
            0,
            &text,
            "Long source",
        );
        let content = entry.content.unwrap();
        let target = Target {
            key: entry.admission.key,
            text_hash: content.text_hash.clone(),
            selector: Some(Selector {
                start: 0,
                end: text.chars().count(),
            }),
        };
        let doc = document(&content, &target).unwrap();
        let inker::Block::Quote {
            blocks: quote_blocks,
        } = &doc.blocks[4]
        else {
            panic!("selected quotation")
        };
        let quote: Vec<&str> = quote_blocks
            .iter()
            .map(|block| {
                let inker::Block::Paragraph { spans } = block else {
                    panic!("quote paragraph")
                };
                let inker::InlineSpan::Strong(spans) = &spans[0] else {
                    panic!("highlighted quote")
                };
                let inker::InlineSpan::Text(text) = &spans[0] else {
                    panic!("exact quote text")
                };
                text.as_str()
            })
            .collect();
        let body: Vec<&str> = doc.blocks[5..doc.blocks.len() - 1]
            .iter()
            .map(|block| {
                let inker::Block::Paragraph { spans } = block else {
                    panic!("body paragraph")
                };
                let inker::InlineSpan::Text(text) = &spans[0] else {
                    panic!("exact body text")
                };
                text.as_str()
            })
            .collect();
        assert!(quote.len() > 1 && body.len() > 1);
        assert!(
            quote
                .iter()
                .chain(&body)
                .all(|part| part.len() <= DISPLAY_BLOCK_BYTES)
        );
        assert_eq!(quote.concat(), text);
        assert_eq!(body.concat(), text);
        assert_eq!(doc.address, address(&target).unwrap());
        assert_eq!(
            eidetic::Hash::of(body.concat().as_bytes()).to_string(),
            target.text_hash
        );
        let unbroken = "é🐦".repeat(10000);
        let parts = display_parts(&unbroken);
        assert!(parts.iter().all(|part| part.len() <= DISPLAY_BLOCK_BYTES));
        assert_eq!(parts.concat(), unbroken);
    }
}
