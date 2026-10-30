// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RecallSource {
    /// Ignore fallback labels that add nothing beyond the canonical URL.
    pub fn new(url: impl Into<String>, title: impl Into<String>) -> Option<Self> {
        let url = url.into();
        let title = title.into().trim().to_string();
        (!title.is_empty() && title != url).then_some(Self { url, title })
    }
}
