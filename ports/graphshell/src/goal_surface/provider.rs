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

use chartulary::Labeled;
use eidetic::{PageTextRef, PageTextStore, ResourceCaptureStore};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
use fleece::retained_traversal::{RetainedLink, RetainedObservation, RetainedProvider};
use mere::kernel::graph::{Graph, RelationKind};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use uuid::Uuid;

/// One retained page: the address it was captured at and every version held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedPage {
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
    /// Page versions left out because the load byte cap was reached.
    pub skipped_versions: usize,
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
        let text = PageTextStore::new(backend.clone());
        let captures = ResourceCaptureStore::new(backend);
        let mut out = Self {
            owner: owner.to_owned(),
            ..Self::default()
        };
        let mut budget = max_bytes;
        let titles = titles_by_resource(graph);
        for entry in text.references().await.map_err(|e| e.to_string())? {
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
                    url: entry.url.clone(),
                    title: titles.get(&resource).cloned().flatten(),
                    current: entry.blob,
                    versions,
                },
            );
        }
        out.read_links(graph);
        Ok(out)
    }

    fn read_links(&mut self, graph: &Graph) {
        let mut links: BTreeMap<Uuid, BTreeSet<(Uuid, Option<String>)>> = BTreeMap::new();
        // Projected relations are surface edges plus resource-level assertions
        // lifted to the surfaces showing their endpoints.
        for (relation, row) in graph.projected_relations() {
            if !matches!(
                row.kind,
                RelationKind::Semantic(_) | RelationKind::OpenPredicate | RelationKind::Traversal
            ) {
                continue;
            }
            let (Some(from), Some(to), Some(target)) = (
                graph.shown_resource_id(row.from),
                graph.shown_resource_id(row.to),
                graph.get_node(row.to),
            ) else {
                continue;
            };
            if to == from || !self.pages.contains_key(&from) {
                continue;
            }
            let edge_label = graph.get_relation(relation).and_then(|payload| {
                payload.held_semantic_statements().find_map(|statement| match row.kind {
                    RelationKind::Semantic(sub) if statement.recognized_sub_kind == Some(sub) => {
                        Some(statement.label.clone().unwrap_or_else(|| format!("{sub:?}").to_lowercase()))
                    },
                    RelationKind::OpenPredicate if statement.recognized_sub_kind.is_none() => {
                        Some(statement.label.clone().unwrap_or_else(|| statement.predicate.clone()))
                    },
                    _ => None,
                })
            });
            let title = Labeled::title(target).map(str::to_owned);
            let label = match (title.clone(), edge_label) {
                (Some(t), Some(e)) => Some(format!("{t} {e}")),
                (t, e) => t.or(e),
            };
            if !self.pages.contains_key(&to) {
                self.unretained
                    .entry(to)
                    .or_insert_with(|| (target.url().to_owned(), title));
            }
            links.entry(from).or_default().insert((to, label));
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

fn titles_by_resource(graph: &Graph) -> HashMap<Uuid, Option<String>> {
    graph
        .nodes()
        .filter_map(|(key, node)| {
            Some((
                graph.shown_resource_id(key)?,
                Labeled::title(node).map(str::to_owned),
            ))
        })
        .collect()
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
