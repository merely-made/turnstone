// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn graph_scope() -> ScopePath {
    ScopePath::parse(GRAPH_SCOPE).expect("a valid scope")
}
