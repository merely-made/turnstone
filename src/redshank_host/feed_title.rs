// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The show a feed episode belongs to, as far as a projection payload knows:
/// the feed's host. The full show title lives in Turnstone's own feed store,
/// not in the pane source.
pub(super) fn feed_title(item: &LibraryItem) -> Option<String> {
    let LibraryItem::FeedEpisode { feed_url, .. } = item else {
        return None;
    };
    url::Url::parse(feed_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
}
