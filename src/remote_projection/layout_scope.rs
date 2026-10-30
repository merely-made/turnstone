// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The endpoint's capabilities as parsed scopes. Both are places in the audit
/// graph with unbounded interiors, so they are scopes rather than powers
/// (capability-model round, 2026-07-23).
pub(super) fn layout_scope() -> ScopePath {
    ScopePath::parse(LAYOUT_SCOPE).expect("a valid scope")
}
