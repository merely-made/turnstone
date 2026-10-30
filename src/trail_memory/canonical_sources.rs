// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Canonicalize the current title projection. A URL may appear in more than
/// one graph member; sorting makes the selected title deterministic rather
/// than dependent on graph iteration order.
pub(super) fn canonical_sources(mut sources: Vec<RecallSource>) -> Vec<RecallSource> {
    sources.sort_unstable();
    sources.dedup_by(|left, right| left.url == right.url);
    sources
}
