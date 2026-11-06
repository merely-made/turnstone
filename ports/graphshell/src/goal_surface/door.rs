// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The goal surface's page snapshot across the resident door.
//!
//! The resident reads its reservoir-owned page-text store with
//! [`RetainedPages::load_bounded`] and answers a `RetainedPages` request with
//! [`to_wire`]; the viewer rebuilds the same snapshot with [`from_wire`] and
//! hands it to the unchanged goal surface. Identity is the resource UUID the
//! wire carries, so a citation names the same source on both sides; the owner
//! label is the viewer's own, because a source minted for one owner never
//! resolves under another (see the provider's identity mapping).

use super::provider::{LoadBounds, Origin, RetainedPage, RetainedPages};
use chirograph::{
    ProjectionSession, RetainedKeeping, RetainedLinkWire, RetainedPageWire, RetainedPagesLimits,
    RetainedPagesResponse, RetainedTruncation, RetainedUnretainedWire, RetainedVersionWire,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// The load bounds a request's effective limits imply.
pub fn bounds(limits: RetainedPagesLimits) -> LoadBounds {
    LoadBounds {
        max_pages: limits.max_pages as usize,
        max_bytes: limits.max_bytes as usize,
        max_versions: limits.max_versions as usize,
    }
}

/// Answer a request from a bounded snapshot. `applied` must be the request's
/// [`RetainedPagesLimits::effective`] limits, the ones `pages` was loaded with.
pub fn to_wire(
    pages: &RetainedPages,
    session: ProjectionSession,
    applied: RetainedPagesLimits,
    keeping: RetainedKeeping,
) -> RetainedPagesResponse {
    let mut wire_pages = Vec::new();
    for (source, page) in pages.sources() {
        let Some(id) = super::provider::resource_of(pages.owner(), &source) else {
            continue;
        };
        // Current first, then the others in version order for a stable reply.
        let mut versions = vec![RetainedVersionWire {
            version: page.current.clone(),
            text: page.versions[&page.current].clone(),
        }];
        versions.extend(
            page.versions
                .iter()
                .filter(|(version, _)| **version != page.current)
                .map(|(version, text)| RetainedVersionWire {
                    version: version.clone(),
                    text: text.clone(),
                }),
        );
        wire_pages.push(RetainedPageWire {
            id: id.to_string(),
            address: page.url.clone(),
            title: page.title.clone(),
            current: page.current.clone(),
            versions,
        });
    }
    let (mut links, mut unretained, mut links_omitted) = (Vec::new(), BTreeMap::new(), 0_u64);
    for (from, to, label) in pages.link_rows() {
        if links.len() >= applied.max_links as usize {
            links_omitted += 1;
            continue;
        }
        if let Some((address, title)) = pages.unretained_of(to) {
            unretained.entry(to).or_insert((address, title));
        }
        links.push(RetainedLinkWire {
            from: from.to_string(),
            to: to.to_string(),
            label,
        });
    }
    RetainedPagesResponse {
        session,
        applied,
        keeping,
        pages: wire_pages,
        links,
        unretained: unretained
            .into_iter()
            .map(|(id, (address, title))| RetainedUnretainedWire {
                id: id.to_string(),
                address,
                title,
            })
            .collect(),
        truncated: RetainedTruncation {
            pages_omitted: pages.skipped_pages as u64,
            versions_omitted: pages.skipped_versions as u64,
            links_omitted,
        },
    }
}

/// Rebuild the snapshot a reply describes, under the viewer's `owner` label.
/// Refuses a reply whose pages do not hold their own current text, or whose
/// version names are not the BLAKE3 of the text they carry.
pub fn from_wire(owner: &str, reply: &RetainedPagesResponse) -> Result<RetainedPages, String> {
    let uuid = |id: &str| Uuid::parse_str(id).map_err(|_| format!("malformed page id {id:?}"));
    let mut pages = BTreeMap::new();
    for page in &reply.pages {
        let mut versions = BTreeMap::new();
        for held in &page.versions {
            if eidetic::Hash::of(held.text.as_bytes()).to_hex() != held.version {
                return Err(format!("{}: a version does not match its text", page.address));
            }
            versions.insert(held.version.clone(), held.text.clone());
        }
        if !versions.contains_key(&page.current) {
            return Err(format!("{}: the current version is not held", page.address));
        }
        pages.insert(
            uuid(&page.id)?,
            RetainedPage {
                origin: Origin::PageText,
                url: page.address.clone(),
                title: page.title.clone(),
                current: page.current.clone(),
                versions,
            },
        );
    }
    let mut links: BTreeMap<Uuid, Vec<(Uuid, Option<String>)>> = BTreeMap::new();
    for link in &reply.links {
        links
            .entry(uuid(&link.from)?)
            .or_default()
            .push((uuid(&link.to)?, link.label.clone()));
    }
    let mut unretained = BTreeMap::new();
    for target in &reply.unretained {
        unretained.insert(uuid(&target.id)?, (target.address.clone(), target.title.clone()));
    }
    Ok(RetainedPages::from_parts(
        owner,
        pages,
        links,
        unretained,
        reply.truncated.versions_omitted as usize,
        reply.truncated.pages_omitted as usize,
    ))
}
