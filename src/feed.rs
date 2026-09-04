// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable, graph-native feed subscriptions.
//!
//! A subscription belongs to a kept source node. The sidecar stores scheduling
//! and duplicate-suppression state; source and entry identity, relations,
//! titles, bodies, and unread tags remain ordinary graph truth.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::action::FetchedPage;

pub const FEED_TAG: &str = "feed";
pub const FEED_ENTRY_TAG: &str = "feed-entry";
pub const KEEP_TAG: &str = "keep";
pub const UNREAD_TAG: &str = "unread";

const FILE_NAME: &str = "feed_subscriptions.json";
const FORMAT_VERSION: u16 = 1;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct FeedEnclosure {
    pub url: String,
    pub media_type: Option<String>,
    pub byte_length: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PodcastResource {
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PodcastTranscript {
    pub url: String,
    pub media_type: Option<String>,
    pub language: Option<String>,
    pub rel: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeedEntry {
    pub url: String,
    pub guid: Option<String>,
    pub title: String,
    pub date: Option<String>,
    pub summary: Option<String>,
    pub enclosures: Vec<FeedEnclosure>,
    pub duration: Option<String>,
    pub artwork: Option<String>,
    pub chapters: Vec<PodcastResource>,
    pub transcripts: Vec<PodcastTranscript>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedFeed {
    pub title: Option<String>,
    pub artwork: Option<String>,
    pub entries: Vec<FeedEntry>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryProjection {
    pub entry: FeedEntry,
    pub member: Option<Uuid>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeedMerge {
    pub title: Option<String>,
    pub entries: Vec<EntryProjection>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedMemberInfo {
    Source {
        period: servitor::Period,
        last_checked_ms: Option<u64>,
        last_error: Option<String>,
        unread: usize,
    },
    Entry {
        source_url: String,
        date: Option<String>,
        unread: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredEntry {
    member: Option<Uuid>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    guid: Option<String>,
    title: String,
    date: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    enclosures: Vec<FeedEnclosure>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    artwork: Option<String>,
    #[serde(default)]
    chapters: Vec<PodcastResource>,
    #[serde(default)]
    transcripts: Vec<PodcastTranscript>,
    unread: bool,
}

impl StoredEntry {
    fn same_document(&self, entry: &FeedEntry) -> bool {
        self.url == entry.url
            && self.guid == entry.guid
            && self.title == entry.title
            && self.date == entry.date
            && self.summary == entry.summary
            && self.enclosures == entry.enclosures
            && self.duration == entry.duration
            && self.artwork == entry.artwork
            && self.chapters == entry.chapters
            && self.transcripts == entry.transcripts
    }

    fn replace_document(&mut self, entry: &FeedEntry) {
        self.url = entry.url.clone();
        self.guid = entry.guid.clone();
        self.title = entry.title.clone();
        self.date = entry.date.clone();
        self.summary = entry.summary.clone();
        self.enclosures = entry.enclosures.clone();
        self.duration = entry.duration.clone();
        self.artwork = entry.artwork.clone();
        self.chapters = entry.chapters.clone();
        self.transcripts = entry.transcripts.clone();
        self.unread = true;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Subscription {
    source: Uuid,
    url: String,
    #[serde(with = "period_serde")]
    period: servitor::Period,
    last_checked_ms: Option<u64>,
    last_success_ms: Option<u64>,
    last_error: Option<String>,
    #[serde(default)]
    artwork: Option<String>,
    #[serde(default)]
    diagnostics: Vec<String>,
    #[serde(default)]
    entries: BTreeMap<String, StoredEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedSubscriptions {
    #[serde(default = "format_version")]
    version: u16,
    #[serde(default)]
    subscriptions: BTreeMap<Uuid, Subscription>,
    #[serde(skip)]
    in_flight: BTreeSet<Uuid>,
}

impl Default for FeedSubscriptions {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            subscriptions: BTreeMap::new(),
            in_flight: BTreeSet::new(),
        }
    }
}

fn format_version() -> u16 {
    FORMAT_VERSION
}

impl FeedSubscriptions {
    pub fn load(session_dir: &Path) -> Self {
        let path = session_dir.join(FILE_NAME);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(subscriptions) if subscriptions.version == FORMAT_VERSION => subscriptions,
            Ok(_) => {
                tracing::warn!(path = ?path, "unsupported feed subscription version");
                Self::default()
            }
            Err(error) => {
                tracing::warn!(%error, path = ?path, "failed to read feed subscriptions");
                Self::default()
            }
        }
    }

    pub fn save(&self, session_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(session_dir)?;
        let target = session_dir.join(FILE_NAME);
        let temporary = target.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        if let Err(error) = (|| -> io::Result<()> {
            std::fs::write(&temporary, bytes)?;
            if target.exists() {
                std::fs::remove_file(&target)?;
            }
            std::fs::rename(&temporary, &target)
        })() {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }

    pub fn subscribe(&mut self, source: Uuid, url: String, period: servitor::Period) -> bool {
        let replacement = Subscription {
            source,
            url: url.clone(),
            period,
            last_checked_ms: None,
            last_success_ms: None,
            last_error: None,
            artwork: None,
            diagnostics: Vec::new(),
            entries: BTreeMap::new(),
        };
        match self.subscriptions.get_mut(&source) {
            Some(subscription) if subscription.url == url => {
                let changed = subscription.period != period;
                subscription.period = period;
                changed
            }
            Some(subscription) => {
                *subscription = replacement;
                self.in_flight.remove(&source);
                true
            }
            None => {
                self.subscriptions.insert(source, replacement);
                true
            }
        }
    }

    pub fn unsubscribe(&mut self, source: Uuid) -> bool {
        self.in_flight.remove(&source);
        self.subscriptions.remove(&source).is_some()
    }

    pub fn entry_members(&self, source: Uuid) -> Vec<Uuid> {
        self.subscriptions
            .get(&source)
            .into_iter()
            .flat_map(|subscription| subscription.entries.values())
            .filter_map(|entry| entry.member)
            .collect()
    }

    pub fn is_current(&self, source: Uuid, url: &str) -> bool {
        self.subscriptions
            .get(&source)
            .is_some_and(|subscription| subscription.url == url)
    }

    pub fn start(&mut self, source: Uuid) -> Option<String> {
        let subscription = self.subscriptions.get(&source)?;
        if !self.in_flight.insert(source) {
            return None;
        }
        Some(subscription.url.clone())
    }

    pub fn start_all(&mut self) -> Vec<(Uuid, String)> {
        let sources: Vec<Uuid> = self.subscriptions.keys().copied().collect();
        sources
            .into_iter()
            .filter_map(|source| self.start(source).map(|url| (source, url)))
            .collect()
    }

    pub fn start_due(&mut self, now_ms: u64) -> Vec<(Uuid, String)> {
        let due: Vec<Uuid> = self
            .subscriptions
            .values()
            .filter(|subscription| {
                !self.in_flight.contains(&subscription.source)
                    && subscription.last_checked_ms.is_none_or(|last| {
                        now_ms.saturating_sub(last) >= subscription.period.millis()
                    })
            })
            .map(|subscription| subscription.source)
            .collect();
        due.into_iter()
            .filter_map(|source| self.start(source).map(|url| (source, url)))
            .collect()
    }

    pub fn fetched_error(&mut self, source: Uuid, error: String, now_ms: u64) -> bool {
        self.in_flight.remove(&source);
        let Some(subscription) = self.subscriptions.get_mut(&source) else {
            return false;
        };
        subscription.last_checked_ms = Some(now_ms);
        subscription.last_error = Some(error);
        true
    }

    pub fn merge(&mut self, source: Uuid, parsed: ParsedFeed, now_ms: u64) -> Option<FeedMerge> {
        self.in_flight.remove(&source);
        let subscription = self.subscriptions.get_mut(&source)?;
        subscription.last_checked_ms = Some(now_ms);
        subscription.last_success_ms = Some(now_ms);
        subscription.last_error = None;

        let ParsedFeed {
            title,
            artwork,
            entries,
            diagnostics,
        } = parsed;
        subscription.artwork = artwork;
        subscription.diagnostics = diagnostics.clone();
        let mut merge = FeedMerge {
            title,
            entries: Vec::new(),
            diagnostics,
        };
        for entry in entries {
            let identity = entry_identity(&entry);
            if !subscription.entries.contains_key(&identity)
                && entry.guid.is_some()
                && let Some(old_identity) = subscription
                    .entries
                    .iter()
                    .find(|(stored_identity, stored)| {
                        stored.url == entry.url || stored_identity.as_str() == entry.url
                    })
                    .map(|(identity, _)| identity.clone())
                && let Some(stored) = subscription.entries.remove(&old_identity)
            {
                subscription.entries.insert(identity.clone(), stored);
            }
            match subscription.entries.get_mut(&identity) {
                Some(stored) if stored.same_document(&entry) => {}
                Some(stored) => {
                    stored.replace_document(&entry);
                    merge.entries.push(EntryProjection {
                        member: stored.member,
                        entry,
                    });
                }
                None => {
                    subscription.entries.insert(
                        identity,
                        StoredEntry {
                            member: None,
                            url: entry.url.clone(),
                            guid: entry.guid.clone(),
                            title: entry.title.clone(),
                            date: entry.date.clone(),
                            summary: entry.summary.clone(),
                            enclosures: entry.enclosures.clone(),
                            duration: entry.duration.clone(),
                            artwork: entry.artwork.clone(),
                            chapters: entry.chapters.clone(),
                            transcripts: entry.transcripts.clone(),
                            unread: true,
                        },
                    );
                    merge.entries.push(EntryProjection {
                        entry,
                        member: None,
                    });
                }
            }
        }
        Some(merge)
    }

    pub fn bind_entry(&mut self, source: Uuid, url: &str, member: Uuid) {
        if let Some(entry) = self
            .subscriptions
            .get_mut(&source)
            .and_then(|subscription| {
                subscription
                    .entries
                    .iter_mut()
                    .find(|(identity, entry)| identity.as_str() == url || entry.url == url)
                    .map(|(_, entry)| entry)
            })
        {
            entry.member = Some(member);
        }
    }

    pub fn mark_read(&mut self, member: Uuid) -> bool {
        let mut changed = false;
        for subscription in self.subscriptions.values_mut() {
            for entry in subscription.entries.values_mut() {
                if entry.member == Some(member) && entry.unread {
                    entry.unread = false;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn forget_member(&mut self, member: Uuid) -> bool {
        let removed_source = self.unsubscribe(member);
        let mut changed = removed_source;
        for subscription in self.subscriptions.values_mut() {
            for entry in subscription.entries.values_mut() {
                if entry.member == Some(member) {
                    entry.member = None;
                    entry.unread = false;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn reconcile(&mut self, graph: &mere::kernel::graph::Graph) {
        self.in_flight.clear();
        self.subscriptions.retain(|source, subscription| {
            graph
                .get_node_by_id(*source)
                .is_some_and(|(_, node)| node.url() == subscription.url)
        });
        for subscription in self.subscriptions.values_mut() {
            for (identity, entry) in &mut subscription.entries {
                let url = if entry.url.is_empty() {
                    identity.as_str()
                } else {
                    entry.url.as_str()
                };
                entry.member = entry
                    .member
                    .filter(|member| {
                        graph
                            .get_node_by_id(*member)
                            .is_some_and(|(_, node)| node.url() == url)
                    })
                    .or_else(|| graph.get_node_by_url(url).map(|(_, node)| node.id));
                if entry.member.is_none() {
                    entry.unread = false;
                }
            }
        }
    }

    pub fn member_info(&self, member: Uuid) -> Option<FeedMemberInfo> {
        if let Some(subscription) = self.subscriptions.get(&member) {
            return Some(FeedMemberInfo::Source {
                period: subscription.period,
                last_checked_ms: subscription.last_checked_ms,
                last_error: subscription.last_error.clone(),
                unread: subscription
                    .entries
                    .values()
                    .filter(|entry| entry.unread)
                    .count(),
            });
        }
        self.subscriptions.values().find_map(|subscription| {
            subscription
                .entries
                .values()
                .find(|entry| entry.member == Some(member))
                .map(|entry| FeedMemberInfo::Entry {
                    source_url: subscription.url.clone(),
                    date: entry.date.clone(),
                    unread: entry.unread,
                })
        })
    }

    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn unread_count(&self) -> usize {
        self.subscriptions
            .values()
            .flat_map(|subscription| subscription.entries.values())
            .filter(|entry| entry.unread)
            .count()
    }
}

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

fn parse_xml_feed(source_url: &str, body: &str) -> Result<ParsedFeed, String> {
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

#[derive(Deserialize)]
struct JsonFeed {
    title: Option<String>,
    items: Vec<JsonFeedItem>,
}

#[derive(Deserialize)]
struct JsonFeedItem {
    id: String,
    url: Option<String>,
    external_url: Option<String>,
    title: Option<String>,
    date_published: Option<String>,
    summary: Option<String>,
    content_text: Option<String>,
    content_html: Option<String>,
}

fn parse_json_feed(source_url: &str, body: &str) -> Result<ParsedFeed, String> {
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

fn parse_gemtext_feed(source_url: &str, body: &str) -> Result<ParsedFeed, String> {
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

fn gemini_feed_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn entry_url(
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

fn resolve_url(source_url: &str, link: &str) -> Result<String, String> {
    let base = url::Url::parse(source_url).map_err(|error| error.to_string())?;
    base.join(link)
        .map(|url| url.to_string())
        .map_err(|error| format!("feed entry URL {link:?}: {error}"))
}

fn deduplicate(entries: Vec<FeedEntry>) -> Vec<FeedEntry> {
    let mut seen = BTreeSet::new();
    entries
        .into_iter()
        .filter(|entry| seen.insert(entry_identity(entry)))
        .collect()
}

fn entry_identity(entry: &FeedEntry) -> String {
    entry
        .guid
        .as_deref()
        .filter(|guid| !guid.trim().is_empty())
        .map(|guid| format!("guid:{guid}"))
        .unwrap_or_else(|| entry.url.clone())
}

mod period_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(period: &servitor::Period, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(period.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<servitor::Period, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        servitor::Period::parse(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown feed period {raw:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemtext_feed_preserves_relative_urls_dates_and_dedupes() {
        let parsed = parse_document(
            "gemini://capsule.test/log/index.gmi",
            &FetchedPage::text(
                Some("text/gemini; charset=utf-8".into()),
                "# Log\n=> entry-one.gmi 2026-08-17 First\n=> entry-one.gmi 2026-08-17 Duplicate\n```\n=> hidden.gmi 2026-08-18 Hidden\n```\n=> /two.gmi 2026-08-18 Second\n",
            ),
        )
        .unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Log"));
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(
            parsed.entries[0].url,
            "gemini://capsule.test/log/entry-one.gmi"
        );
        assert_eq!(parsed.entries[0].date.as_deref(), Some("2026-08-17"));
        assert_eq!(parsed.entries[1].url, "gemini://capsule.test/two.gmi");
    }

    #[test]
    fn rss_and_json_feed_share_the_entry_shape() {
        let rss = parse_document(
            "https://example.test/feed.xml",
            &FetchedPage::text(
                Some("application/rss+xml".into()),
                "<rss><channel><title>Notes</title><item><title>One</title><link>/one</link><pubDate>today</pubDate><description>Hello</description></item></channel></rss>",
            ),
        )
        .unwrap();
        assert_eq!(rss.entries[0].url, "https://example.test/one");

        let json = parse_document(
            "https://example.test/feed.json",
            &FetchedPage::text(
                Some("application/feed+json".into()),
                r#"{"title":"Notes","items":[{"id":"one","url":"/one","title":"One"}]}"#,
            ),
        )
        .unwrap();
        assert_eq!(json.entries[0].url, rss.entries[0].url);
    }

    #[test]
    fn schedule_survives_restart_and_suppresses_unchanged_entries() {
        let dir = tempfile::tempdir().unwrap();
        let source = Uuid::new_v4();
        let mut feeds = FeedSubscriptions::default();
        assert!(feeds.subscribe(
            source,
            "gemini://capsule.test/feed".into(),
            servitor::Period::Hour
        ));
        assert_eq!(feeds.start_due(0).len(), 1);
        let parsed = ParsedFeed {
            title: Some("Log".into()),
            entries: vec![FeedEntry {
                url: "gemini://capsule.test/one".into(),
                title: "One".into(),
                date: Some("2026-08-18".into()),
                summary: None,
                ..FeedEntry::default()
            }],
            ..ParsedFeed::default()
        };
        let merge = feeds.merge(source, parsed.clone(), 10).unwrap();
        assert_eq!(merge.entries.len(), 1);
        assert!(feeds.merge(source, parsed, 20).unwrap().entries.is_empty());
        feeds.save(dir.path()).unwrap();

        let mut reopened = FeedSubscriptions::load(dir.path());
        assert!(reopened.start_due(3_599_999).is_empty());
        assert_eq!(reopened.start_due(3_600_020).len(), 1);
    }

    #[test]
    fn podcast_facts_resolve_against_the_caller_base_and_guid_wins_deduplication() {
        let parsed = parse_document(
            "https://example.test/shows/feed.xml",
            &FetchedPage::text(
                Some("application/rss+xml".into()),
                r#"<rss xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
                    xmlns:podcast="https://podcastindex.org/namespace/1.0"
                    xmlns:mystery="https://example.test/mystery">
                  <channel>
                    <title>Field Notes</title>
                    <itunes:image href="art/feed.jpg"/>
                    <item>
                      <guid>episode-42</guid><title>Wetland</title><link>episodes/42</link>
                      <enclosure url="audio/42.mp3" type="audio/mpeg" length="123456"/>
                      <itunes:duration>01:02:03</itunes:duration>
                      <itunes:image href="art/42.jpg"/>
                      <podcast:chapters url="chapters/42.json" type="application/json+chapters"/>
                      <podcast:transcript url="transcripts/42.vtt" type="text/vtt" language="en"/>
                      <mystery:waveform bins="32"/>
                    </item>
                    <item><guid>episode-42</guid><title>Duplicate</title><link>elsewhere</link></item>
                  </channel>
                </rss>"#,
            ),
        )
        .unwrap();

        assert_eq!(
            parsed.artwork.as_deref(),
            Some("https://example.test/shows/art/feed.jpg")
        );
        assert_eq!(parsed.entries.len(), 1);
        let entry = &parsed.entries[0];
        assert_eq!(entry.guid.as_deref(), Some("episode-42"));
        assert_eq!(entry.url, "https://example.test/shows/episodes/42");
        assert_eq!(
            entry.enclosures[0].url,
            "https://example.test/shows/audio/42.mp3"
        );
        assert_eq!(entry.enclosures[0].byte_length, Some(123_456));
        assert_eq!(entry.duration.as_deref(), Some("01:02:03"));
        assert_eq!(
            entry.artwork.as_deref(),
            Some("https://example.test/shows/art/42.jpg")
        );
        assert_eq!(
            entry.chapters[0].url,
            "https://example.test/shows/chapters/42.json"
        );
        assert_eq!(
            entry.transcripts[0].url,
            "https://example.test/shows/transcripts/42.vtt"
        );
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("mystery:waveform"))
        );
    }

    #[test]
    fn guid_identity_survives_an_enclosure_or_link_change() {
        let source = Uuid::new_v4();
        let member = Uuid::new_v4();
        let mut feeds = FeedSubscriptions::default();
        feeds.subscribe(
            source,
            "https://example.test/feed.xml".into(),
            servitor::Period::Hour,
        );
        let entry = |url: &str| FeedEntry {
            url: url.into(),
            guid: Some("stable-guid".into()),
            title: "Episode".into(),
            enclosures: vec![FeedEnclosure {
                url: format!("{url}.mp3"),
                media_type: Some("audio/mpeg".into()),
                byte_length: None,
            }],
            ..FeedEntry::default()
        };
        let first = feeds
            .merge(
                source,
                ParsedFeed {
                    entries: vec![entry("https://example.test/first")],
                    ..ParsedFeed::default()
                },
                10,
            )
            .unwrap();
        assert_eq!(first.entries.len(), 1);
        feeds.bind_entry(source, "https://example.test/first", member);

        let changed = feeds
            .merge(
                source,
                ParsedFeed {
                    entries: vec![entry("https://example.test/revised")],
                    ..ParsedFeed::default()
                },
                20,
            )
            .unwrap();
        assert_eq!(changed.entries.len(), 1);
        assert_eq!(changed.entries[0].member, Some(member));
        assert_eq!(feeds.subscriptions[&source].entries.len(), 1);
    }
}
