// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Every stored page body for the open session. An unreadable store yields an
/// empty snapshot rather than failing the recall: pages then recall by title
/// and URL, which is what they did before W6c.
pub(super) fn stored_page_texts(store: &FjallStore) -> PageTexts {
    let (texts, error) = pollster::block_on(PageTextStore::new(store).load_all_or_empty());
    if let Some(err) = error {
        tracing::warn!(%err, "trail memory: page texts unreadable; recall falls back to titles");
    }
    texts
}
