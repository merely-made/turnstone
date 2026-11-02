// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The focused node's identity and captions, as the UI would present them.
#[derive(Clone, Debug, PartialEq)]
pub struct FocusedNode {
    pub member: Uuid,
    pub url: String,
    /// The at-rest caption (display label, plus host when it adds info).
    pub caption: String,
    /// Durable keep state read from the node's graph tags.
    pub kept: bool,
    /// The page zoom the node REQUESTS, as a percentage (100 = default). This
    /// is what the node asked for, never what an engine settled on.
    pub page_zoom_percent: u32,
    /// The percentage the node's live engine reports it is ACTUALLY presenting
    /// at. Retained sessions answer a zoom request with their effective level;
    /// hosted surfaces have no read-back, so this stays `None` for them rather
    /// than echoing the request as if it were applied.
    pub applied_page_zoom_percent: Option<u32>,
}
