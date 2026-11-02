// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PendingFetches {
    pub fn note_page(
        &mut self,
        request: fetch::FetchRequestId,
        url: &str,
        node: Uuid,
        owner_url: &str,
        identity_used: bool,
    ) {
        self.pages.insert(
            request,
            PendingPage {
                node,
                fetch_url: url.to_string(),
                owner_url: owner_url.to_string(),
                identity_used,
                kind: PendingPageKind::Page,
            },
        );
    }

    pub fn note_feed(
        &mut self,
        request: fetch::FetchRequestId,
        url: &str,
        node: Uuid,
        identity_used: bool,
    ) {
        self.pages.insert(
            request,
            PendingPage {
                node,
                fetch_url: url.to_string(),
                owner_url: url.to_string(),
                identity_used,
                kind: PendingPageKind::Feed,
            },
        );
    }

    pub fn note_favicon(&mut self, request: fetch::FetchRequestId, owner_url: &str, node: Uuid) {
        self.favicons.insert(
            request,
            PendingFavicon {
                node,
                owner_url: owner_url.to_string(),
            },
        );
    }

    pub fn page_in_flight(&self, url: &str, node: Uuid, owner_url: &str) -> bool {
        self.pages.values().any(|request| {
            request.fetch_url == url && request.node == node && request.owner_url == owner_url
        })
    }

    /// Correlate one session's subresource request. Returns `true` only for
    /// the first waiter so several panes sharing a URL reuse one actor fetch.
    pub fn note_subresource(&mut self, url: &str, node: Uuid) -> bool {
        let requesters = self.subresources.entry(url.to_string()).or_default();
        let first = requesters.is_empty();
        if !requesters.contains(&node) {
            requesters.push(node);
        }
        first
    }

    pub fn take_subresources(&mut self, url: &str) -> Vec<Uuid> {
        self.subresources.remove(url).unwrap_or_default()
    }

    /// Whether any page, favicon, subresource, or submission fetch remains
    /// outstanding. Every actor command has a terminal completion, even when
    /// a favicon failure remains deliberately silent in the UI.
    pub fn any_in_flight(&self) -> bool {
        !self.pages.is_empty()
            || !self.favicons.is_empty()
            || !self.subresources.is_empty()
            || !self.submissions.is_empty()
    }

    pub(super) fn take_page(&mut self, request: fetch::FetchRequestId) -> Option<PendingPage> {
        self.pages.remove(&request)
    }

    pub(super) fn page(&self, request: fetch::FetchRequestId) -> Option<&PendingPage> {
        self.pages.get(&request)
    }

    pub(super) fn cancel_page(&mut self, request: fetch::FetchRequestId) {
        self.pages.remove(&request);
    }

    pub(super) fn take_favicon(
        &mut self,
        request: fetch::FetchRequestId,
    ) -> Option<PendingFavicon> {
        self.favicons.remove(&request)
    }
}
