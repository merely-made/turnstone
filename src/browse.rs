// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The browse lane: address opening, fetch, metadata enrichment, favicon
//! discovery — and the fetch actor's adapter, converting the service's
//! concrete types into the app-owned [`Update`] messages so the vocabulary
//! stays port-agnostic. The folding itself is pure (testable; the app update
//! never blocks). Live content (the engine registry, document lifecycle,
//! verso-tile, content frames) is the separate `content` module born at
//! obviation rung 4: notes, gemini documents, local media, and HTML are all
//! content, while only some of it arrives by web fetching.
//!
//! Page fetches carry an exact request id across the actor boundary. The shell
//! reattaches node ownership through [`PendingFetches`], so same-address loads
//! stay distinct and stopping or replacing one cannot affect another.

use std::collections::HashMap;

use fetch::{FetchCommand, FetchUpdate};
use layout_dom_api::LayoutDom;
use mere::canvas::Canvas;
use uuid::Uuid;

use crate::action::{Effect, FetchedPage, Update};

pub(crate) struct DecodedImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

/// The shell-owned correlation table: which node asked for each in-flight
/// fetch. Port bookkeeping, not app truth (the app's record is the effect it
/// emitted); keyed by the request id the actor echoes on every page answer.
#[derive(Debug, Default)]
pub struct PendingFetches {
    pages: HashMap<fetch::FetchRequestId, PendingPage>,
    favicons: HashMap<fetch::FetchRequestId, PendingFavicon>,
    subresources: HashMap<String, Vec<Uuid>>,
    submissions: HashMap<u64, Option<Uuid>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingPage {
    node: Uuid,
    fetch_url: String,
    owner_url: String,
    identity_used: bool,
    kind: PendingPageKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingFavicon {
    node: Uuid,
    owner_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPageKind {
    Page,
    Feed,
}

#[cfg(test)]
#[path = "browse/tests.rs"]
mod tests;

#[path = "browse/is_fetchable.rs"]
mod is_fetchable;
pub(crate) use is_fetchable::is_fetchable;
#[path = "browse/decode_image_bytes.rs"]
mod decode_image_bytes;
pub(crate) use decode_image_bytes::decode_image_bytes;
#[path = "browse/pending_fetches_note_page.rs"]
mod pending_fetches_note_page;
#[path = "browse/update_from_fetch.rs"]
mod update_from_fetch;
pub use update_from_fetch::update_from_fetch;
#[path = "browse/fetch_commands_for.rs"]
mod fetch_commands_for;
pub use fetch_commands_for::fetch_commands_for;
#[path = "browse/still_current.rs"]
mod still_current;
pub(crate) use still_current::still_current;
#[path = "browse/apply_page.rs"]
mod apply_page;
pub use apply_page::apply_page;
#[path = "browse/apply_favicon.rs"]
mod apply_favicon;
pub use apply_favicon::apply_favicon;
#[path = "browse/favicon_url_for.rs"]
mod favicon_url_for;
use favicon_url_for::favicon_url_for;
#[path = "browse/linked_icon_href.rs"]
mod linked_icon_href;
use linked_icon_href::linked_icon_href;
