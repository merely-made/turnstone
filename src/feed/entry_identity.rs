// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn entry_identity(entry: &FeedEntry) -> String {
    entry
        .guid
        .as_deref()
        .filter(|guid| !guid.trim().is_empty())
        .map(|guid| format!("guid:{guid}"))
        .unwrap_or_else(|| entry.url.clone())
}
