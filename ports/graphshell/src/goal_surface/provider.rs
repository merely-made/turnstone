// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The retained-source provider over what Graphshell already holds.
//!
//! # Identity mapping
//!
//! A retained page is one common resource (`chartulary::resource_id` of the
//! canonical address, the same identity `ResourceCaptureStore` keys versions
//! by). Its [`RecallSourceId`] is
//!
//! ```text
//! { kind: PageText, owner: Some(<owner label>), source: <resource uuid>, node: None }
//! ```
//!
//! The owner label names this Graphshell session, so a source minted by
//! another owner never resolves here. The URL is an address shown to the
//! person, never part of the identity. The retained version is the blake3 hex
//! of the page text, the same blob address `PageTextStore` and
//! `ResourceCaptureStore` use, so a citation names exactly the bytes read.
//!
//! # Links
//!
//! A link is an outgoing semantic, open-predicate or traversal relation from
//! any graph surface showing the resource, to the resource its target surface
//! shows. The label is the target surface's title plus the relation's label,
//! used only to rank candidates. A target nothing was retained for is still
//! offered; the traversal then reports it as missing rather than hiding it.
//!
//! The snapshot is read from the stores once ([`RetainedPages::load`]); the
//! provider itself holds no store handle and cannot fetch.

use super::facts::GraphFacts;
use eidetic::{PageTextRef, PageTextStore, ResourceCaptureStore};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
use fleece::retained_traversal::{RetainedLink, RetainedObservation, RetainedProvider};
use mere::kernel::graph::Graph;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// What a retained page's text is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    /// Page text stored by `PageTextStore`.
    #[default]
    PageText,
    /// The title and address the graph holds for a captured object. A browser
    /// capture keeps no page body, so this is all the text there is to read.
    Record,
}

/// One retained page: the address it was captured at and every version held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedPage {
    pub origin: Origin,
    pub url: String,
    pub title: Option<String>,
    /// Version (blake3 hex) of the page's current text.
    pub current: String,
    /// Every held version's text, by version. Always contains `current`.
    pub versions: BTreeMap<String, String>,
}

/// A snapshot of retained pages and the links between them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetainedPages {
    owner: String,
    pages: BTreeMap<Uuid, RetainedPage>,
    links: BTreeMap<Uuid, Vec<(Uuid, Option<String>)>>,
    /// Addresses and titles of link targets nothing is retained for.
    unretained: BTreeMap<Uuid, (String, Option<String>)>,
    /// Page versions left out because the load byte or version cap was reached.
    pub skipped_versions: usize,
    /// Retained pages left out: past the page cap, or whose current text did
    /// not fit the byte cap.
    pub skipped_pages: usize,
}

/// What a bounded load may read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadBounds {
    /// Pages to read, most recently stored first.
    pub max_pages: usize,
    /// Bytes of text across every page and version.
    pub max_bytes: usize,
    /// Versions per page, the current one first.
    pub max_versions: usize,
}

impl LoadBounds {
    pub const fn bytes(max_bytes: usize) -> Self {
        Self {
            max_pages: usize::MAX,
            max_bytes,
            max_versions: usize::MAX,
        }
    }
}

/// The source identity of a retained resource under `owner`.
pub fn source_id(owner: &str, resource: Uuid) -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::PageText,
        owner: Some(owner.to_owned()),
        source: resource.to_string(),
        node: None,
    }
}

/// The resource a source names under `owner`, or `None` for another kind,
/// owner, node qualifier or a malformed id.
pub fn resource_of(owner: &str, source: &RecallSourceId) -> Option<Uuid> {
    (source.kind == RecallSourceKind::PageText
        && source.owner.as_deref() == Some(owner)
        && source.node.is_none())
    .then(|| Uuid::parse_str(&source.source).ok())
    .flatten()
}

impl RetainedPages {
    /// Read every retained page, its held versions and the graph's links.
    /// Texts beyond `max_bytes` in total are left out and counted in
    /// [`Self::skipped_versions`]. Reads stores only.
    pub async fn load<B: muniment::Backend + Clone>(
        owner: &str,
        backend: &B,
        graph: &Graph,
        max_bytes: usize,
    ) -> Result<Self, String> {
        Self::load_facts(owner, backend, &GraphFacts::read(graph), max_bytes, false).await
    }

    /// [`Self::load`] from facts already read off the graph, so a task need
    /// not hold the graph across a store read. With `records`, every object
    /// the graph shows that has no stored page text is a page of its own
    /// title and address ([`Origin::Record`]).
    pub async fn load_facts<B: muniment::Backend + Clone>(
        owner: &str,
        backend: &B,
        facts: &GraphFacts,
        max_bytes: usize,
        records: bool,
    ) -> Result<Self, String> {
        Self::load_bounded(owner, backend, facts, LoadBounds::bytes(max_bytes), records).await
    }

    /// [`Self::load_facts`] with page and version caps too. Pages are read
    /// most recently stored first, so a cap keeps what the person kept last.
    pub async fn load_bounded<B: muniment::Backend + Clone>(
        owner: &str,
        backend: &B,
        facts: &GraphFacts,
        bounds: LoadBounds,
        records: bool,
    ) -> Result<Self, String> {
        let max_bytes = bounds.max_bytes;
        let text = PageTextStore::new(backend.clone());
        let captures = ResourceCaptureStore::new(backend);
        let mut out = Self {
            owner: owner.to_owned(),
            ..Self::default()
        };
        let mut budget = max_bytes;
        let mut entries = text.references().await.map_err(|e| e.to_string())?;
        entries.sort_by(|a, b| {
            b.stored_at_ms
                .cmp(&a.stored_at_ms)
                .then_with(|| a.url.cmp(&b.url))
        });
        let total = entries.len();
        for entry in entries {
            if out.pages.len() >= bounds.max_pages {
                break;
            }
            let resource = chartulary::resource_id(&entry.url);
            let mut versions = BTreeMap::new();
            // Current first so a tight cap keeps what the page says now.
            let mut wanted = vec![entry.blob.clone()];
            for held in captures
                .for_resource(&resource.to_string())
                .await
                .map_err(|e| e.to_string())?
            {
                if held.content == eidetic::CaptureContent::PageText {
                    let hex = held.content_hash.to_hex();
                    if !wanted.contains(&hex) {
                        wanted.push(hex);
                    }
                }
            }
            let keep = bounds.max_versions.max(1);
            if wanted.len() > keep {
                out.skipped_versions += wanted.len() - keep;
                wanted.truncate(keep);
            }
            for blob in wanted {
                let at = PageTextRef {
                    url: entry.url.clone(),
                    blob: blob.clone(),
                    stored_at_ms: entry.stored_at_ms,
                };
                let Some(body) = text.text_at(&at).await.map_err(|e| e.to_string())? else {
                    continue;
                };
                if body.len() > budget {
                    out.skipped_versions += 1;
                    continue;
                }
                budget -= body.len();
                versions.insert(blob, body);
            }
            if !versions.contains_key(&entry.blob) {
                continue;
            }
            out.pages.insert(
                resource,
                RetainedPage {
                    origin: Origin::PageText,
                    url: entry.url.clone(),
                    title: facts.title(&resource),
                    current: entry.blob,
                    versions,
                },
            );
        }
        out.skipped_pages = total - out.pages.len();
        if records {
            for (resource, (url, title)) in facts.records() {
                if out.pages.contains_key(resource) {
                    continue;
                }
                let body = record_text(url, title.as_deref());
                if body.len() > budget {
                    out.skipped_versions += 1;
                    continue;
                }
                budget -= body.len();
                let version = eidetic::Hash::of(body.as_bytes()).to_hex();
                out.pages.insert(
                    *resource,
                    RetainedPage {
                        origin: Origin::Record,
                        url: url.clone(),
                        title: title.clone(),
                        current: version.clone(),
                        versions: BTreeMap::from([(version, body)]),
                    },
                );
            }
        }
        out.read_links(facts);
        Ok(out)
    }

    fn read_links(&mut self, facts: &GraphFacts) {
        let mut links: BTreeMap<Uuid, BTreeSet<(Uuid, Option<String>)>> = BTreeMap::new();
        for row in facts.links() {
            if !self.pages.contains_key(&row.from) {
                continue;
            }
            if !self.pages.contains_key(&row.to) {
                self.unretained
                    .entry(row.to)
                    .or_insert_with(|| (row.to_url.clone(), row.to_title.clone()));
            }
            links
                .entry(row.from)
                .or_default()
                .insert((row.to, row.label.clone()));
        }
        self.links = links
            .into_iter()
            .map(|(from, set)| (from, set.into_iter().collect()))
            .collect();
    }

    /// Every retained resource in stable (address) order, for choosing starts.
    pub fn sources(&self) -> Vec<(RecallSourceId, &RetainedPage)> {
        let mut all: Vec<_> = self
            .pages
            .iter()
            .map(|(resource, page)| (source_id(&self.owner, *resource), page))
            .collect();
        all.sort_by(|a, b| a.1.url.cmp(&b.1.url));
        all
    }

    /// Rebuild a snapshot from parts a peer sent; see `door::from_wire`.
    pub(super) fn from_parts(
        owner: &str,
        pages: BTreeMap<Uuid, RetainedPage>,
        links: BTreeMap<Uuid, Vec<(Uuid, Option<String>)>>,
        unretained: BTreeMap<Uuid, (String, Option<String>)>,
        skipped_versions: usize,
        skipped_pages: usize,
    ) -> Self {
        Self {
            owner: owner.to_owned(),
            pages,
            links,
            unretained,
            skipped_versions,
            skipped_pages,
        }
    }

    /// Every retained link as (from, to, label), in stable order.
    pub(super) fn link_rows(&self) -> Vec<(Uuid, Uuid, Option<String>)> {
        self.links
            .iter()
            .flat_map(|(from, to)| to.iter().map(|(to, label)| (*from, *to, label.clone())))
            .collect()
    }

    /// The address and title of a link target nothing is retained for.
    pub(super) fn unretained_of(&self, resource: Uuid) -> Option<(String, Option<String>)> {
        self.unretained.get(&resource).cloned()
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The retained page a source names, if it is this owner's.
    pub fn page(&self, source: &RecallSourceId) -> Option<&RetainedPage> {
        self.pages.get(&resource_of(&self.owner, source)?)
    }

    /// Address and title for a source: retained, or only linked from the graph.
    pub fn describe(&self, source: &RecallSourceId) -> Option<(String, Option<String>)> {
        let resource = resource_of(&self.owner, source)?;
        self.pages
            .get(&resource)
            .map(|p| (p.url.clone(), p.title.clone()))
            .or_else(|| self.unretained.get(&resource).cloned())
    }

    /// The text of exactly this source and version, for rendering a citation.
    pub fn text_of(&self, source: &RecallSourceId, version: &str) -> Option<&str> {
        self.page(source)?.versions.get(version).map(String::as_str)
    }

    pub fn provider(&self) -> PagesProvider<'_> {
        PagesProvider { pages: self }
    }
}

/// The text of a captured object: its title and address, which is everything
/// a browser capture keeps of it.
pub fn record_text(url: &str, title: Option<&str>) -> String {
    match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(title) => format!("{title}. {url}"),
        None => url.to_owned(),
    }
}

/// [`RetainedProvider`] over a [`RetainedPages`] snapshot. A read of held
/// state: it holds no store, transport or clock.
pub struct PagesProvider<'a> {
    pages: &'a RetainedPages,
}

impl RetainedProvider for PagesProvider<'_> {
    fn retained(
        &mut self,
        source: &RecallSourceId,
        version: Option<&str>,
    ) -> Option<RetainedObservation> {
        let resource = resource_of(&self.pages.owner, source)?;
        let page = self.pages.pages.get(&resource)?;
        // A named version that is held is read exactly; one that is not falls
        // back to the current text so the traversal reports the mismatch.
        let version = version
            .filter(|v| page.versions.contains_key(*v))
            .unwrap_or(&page.current);
        let text = page.versions.get(version)?.clone();
        let links = self
            .pages
            .links
            .get(&resource)
            .into_iter()
            .flatten()
            .map(|(to, label)| RetainedLink {
                source: source_id(&self.pages.owner, *to),
                version: None,
                label: label.clone(),
            })
            .collect();
        Some(RetainedObservation {
            text: RetainedSourceText {
                source: source.clone(),
                version: version.to_owned(),
                title: page.title.clone(),
                text,
            },
            links,
        })
    }
}
