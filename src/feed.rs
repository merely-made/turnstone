// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable, graph-native feed subscriptions.
//!
//! A subscription belongs to a kept source node. The sidecar stores scheduling
//! and duplicate-suppression state, including read state keyed by Surface ID.
//! Host-owned Surface facets project those controls. Descriptive tags belong to
//! the shown Resource independently of subscription and retention state.

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

/// The playable facts one subscribed episode carries, projected for a
/// listening tile. Nothing here opens a feed, a cache, or a device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeedEpisodeProjection {
    pub feed_url: String,
    pub guid: String,
    pub enclosure_url: String,
    pub media_type: Option<String>,
    pub title: String,
    pub published: Option<String>,
    pub artwork: Option<String>,
    pub member: Option<Uuid>,
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
#[path = "feed/tests.rs"]
mod tests;

#[path = "feed/is_podcast_audio.rs"]
mod is_podcast_audio;
pub use is_podcast_audio::is_podcast_audio;
#[path = "feed/format_version.rs"]
mod format_version;
#[path = "feed/impl_default_for_feed_subscriptions.rs"]
mod impl_default_for_feed_subscriptions;
#[path = "feed/stored_entry_same_document.rs"]
mod stored_entry_same_document;
use format_version::format_version;
#[path = "feed/feed_subscriptions_load.rs"]
mod feed_subscriptions_load;
#[path = "feed/parse_document.rs"]
mod parse_document;
pub use parse_document::parse_document;
#[path = "feed/parse_xml_feed.rs"]
mod parse_xml_feed;
use parse_xml_feed::parse_xml_feed;
#[path = "feed/parse_json_feed.rs"]
mod parse_json_feed;
use parse_json_feed::parse_json_feed;
#[path = "feed/parse_gemtext_feed.rs"]
mod parse_gemtext_feed;
use parse_gemtext_feed::parse_gemtext_feed;
#[path = "feed/gemini_feed_date.rs"]
mod gemini_feed_date;
use gemini_feed_date::gemini_feed_date;
#[path = "feed/entry_url.rs"]
mod entry_url;
use entry_url::entry_url;
#[path = "feed/resolve_url.rs"]
mod resolve_url;
use resolve_url::resolve_url;
#[path = "feed/deduplicate.rs"]
mod deduplicate;
use deduplicate::deduplicate;
#[path = "feed/entry_identity.rs"]
mod entry_identity;
use entry_identity::entry_identity;
