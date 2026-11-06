// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Page text the person chose to keep, recorded into Eidetic's page-text store.
//!
//! The extension's optional "Keep page text" control extracts readable text
//! from pages the person loads, bounds it, and queues it beside the visit
//! queue; the writer view hands each delivery here. Records are exactly
//! `eidetic::PageTextStore`'s: the text blob at `blob/<blake3 of text>`, the
//! current-address slot at `page-text/<blake3 of the canonical URL>`, and the
//! version reference at `resource-capture/<resource id>/page-text/<blake3>`
//! (`ResourceCaptureStore`). The version *is* the BLAKE3 of the text, so
//! re-keeping an unchanged page writes nothing and a changed page adds a
//! version. Nothing here fetches a destination; the text arrives from the page
//! the person already loaded.

use std::collections::BTreeSet;

use eidetic::PageTextStore;
use muniment::Backend;
use serde::{Deserialize, Serialize};
use url::Url;

/// The most text one page may keep, whatever the extension asked for.
pub const MAX_KEPT_BYTES: usize = 256 * 1024;
/// The extension's default per-page bound.
pub const DEFAULT_KEPT_BYTES: usize = 128 * 1024;
/// Pages accepted from one delivery; the extension sends fewer.
pub const MAX_PAGES_PER_DELIVERY: usize = 32;

/// The person's keep-text settings as the extension last saved them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepTextPolicy {
    pub enabled: bool,
    #[serde(default)]
    pub max_bytes: usize,
    #[serde(default)]
    pub excluded_origins: BTreeSet<String>,
}

/// One loaded page's extracted text.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeptPage {
    pub url: String,
    pub text: String,
    pub at_ms: u64,
    #[serde(default)]
    pub private: bool,
}

/// A delivery of kept pages with the policy they were queued under.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptBatch {
    pub policy: KeepTextPolicy,
    pub pages: Vec<KeptPage>,
}

/// Why a page's text was not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeptRefusal {
    Disabled,
    Private,
    Scheme,
    Excluded,
    Empty,
    TooLarge,
    Excess,
}

/// What a delivery did. `error` is a store failure (quota included): the
/// pages stay queued and a retry is safe because recording is idempotent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeptSummary {
    pub active: bool,
    pub kept: usize,
    pub unchanged: usize,
    pub refused: usize,
    pub error: Option<String>,
}

/// Keeping page text natively: the resident's two owner-checked intents on a
/// mere's attached graph session. The store belongs to the reservoir, so the
/// native capture never opens it; it sends the text it extracted and the
/// resident writes it through [`record_kept_pages`] if the mere's own setting
/// is on and its custody allows (`pandect::RetainedText`).
pub const KEEP_PAGES_INTENT: &str = "retained.keep/v1";
/// Change the mere's "Keep page text" setting. Off until a person turns it on.
pub const KEEP_SETTING_INTENT: &str = "retained.setting/v1";

/// The payload of [`KEEP_PAGES_INTENT`]. The resident stamps its own clock.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeepPagesV1 {
    pub pages: Vec<KeptPage>,
    /// How each page was acquired, when it was fetched natively on a person's
    /// request. The extension's pages arrive without one.
    #[serde(default)]
    pub fetches: Vec<FetchRecordV1>,
}

/// The record of one explicit fetch: the typed address, what answered, and
/// what was kept of it. One address, fetched once, on the person's action;
/// no link is followed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FetchRecordV1 {
    /// The address the person typed.
    pub requested: String,
    /// The address after redirects, which is the page's identity.
    pub final_url: String,
    /// Milliseconds since the epoch when the viewer fetched it.
    pub fetched_at_ms: u64,
    /// The HTTP status of the answer that was kept (a non-success answer is
    /// refused before any record exists).
    pub status: u16,
    pub content_type: Option<String>,
    /// Bytes of HTML read.
    pub html_bytes: u64,
    /// Bytes of text kept, and their BLAKE3 hex (the version).
    pub text_bytes: u64,
    pub text_version: String,
}

/// The version of a kept text: its BLAKE3 hex.
pub fn text_version(text: &str) -> String {
    eidetic::Hash::of(text.as_bytes()).to_hex()
}

/// Where fetch records live in the retained store: by final address, then time.
pub fn fetch_record_key(final_url: &str, fetched_at_ms: u64) -> String {
    format!(
        "retained-text/fetch/v1/{}/{fetched_at_ms}",
        eidetic::Hash::of(final_url.as_bytes()).to_hex()
    )
}

/// The payload of [`KEEP_SETTING_INTENT`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeepSettingV1 {
    pub enabled: bool,
    #[serde(default)]
    pub max_bytes: usize,
    #[serde(default)]
    pub excluded_origins: BTreeSet<String>,
}

impl KeptRefusal {
    /// What to tell the person.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Disabled => "Keep page text is off for this mere.",
            Self::Private => "Private pages are never kept.",
            Self::Scheme => "Only http and https pages can be kept.",
            Self::Excluded => "That site is excluded from kept page text.",
            Self::Empty => "The page had no text to keep.",
            Self::TooLarge => "The page text is larger than the per-page bound.",
            Self::Excess => "Too many pages in one delivery.",
        }
    }
}

/// Whether `page` may be kept under `policy`.
pub fn judge(policy: &KeepTextPolicy, page: &KeptPage) -> Result<(), KeptRefusal> {
    if !policy.enabled {
        return Err(KeptRefusal::Disabled);
    }
    if page.private {
        return Err(KeptRefusal::Private);
    }
    let url = Url::parse(&page.url).map_err(|_| KeptRefusal::Scheme)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(KeptRefusal::Scheme);
    }
    if policy
        .excluded_origins
        .contains(&url.origin().ascii_serialization())
    {
        return Err(KeptRefusal::Excluded);
    }
    if page.text.trim().is_empty() {
        return Err(KeptRefusal::Empty);
    }
    let bound = match policy.max_bytes {
        0 => DEFAULT_KEPT_BYTES,
        asked => asked.min(MAX_KEPT_BYTES),
    };
    if page.text.len() > bound {
        return Err(KeptRefusal::TooLarge);
    }
    Ok(())
}

/// Record a delivery into `PageTextStore` over `backend`. A store error stops
/// the delivery and is reported in the summary; earlier pages stay recorded.
pub async fn record_kept_pages<B: Backend + Clone>(backend: &B, batch: &KeptBatch) -> KeptSummary {
    let mut summary = KeptSummary {
        active: batch.policy.enabled,
        ..KeptSummary::default()
    };
    let store = PageTextStore::new(backend.clone());
    for (index, page) in batch.pages.iter().enumerate() {
        if index >= MAX_PAGES_PER_DELIVERY || judge(&batch.policy, page).is_err() {
            summary.refused += 1;
            continue;
        }
        let version = eidetic::Hash::of(page.text.as_bytes()).to_hex();
        let recorded = async {
            let held = store
                .reference(&page.url)
                .await
                .map_err(|e| e.to_string())?;
            let unchanged = held.is_some_and(|entry| entry.blob == version);
            store
                .put(&page.url, &page.text, page.at_ms)
                .await
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(unchanged)
        }
        .await;
        match recorded {
            Ok(true) => summary.unchanged += 1,
            Ok(false) => summary.kept += 1,
            Err(error) => {
                summary.error = Some(error);
                return summary;
            },
        }
    }
    summary
}

/// Forget the kept text for `url`, with the history for it.
pub async fn forget_kept_text<B: Backend + Clone>(backend: &B, url: &str) -> Result<(), String> {
    PageTextStore::new(backend.clone())
        .forget(url)
        .await
        .map_err(|error| error.to_string())
}
