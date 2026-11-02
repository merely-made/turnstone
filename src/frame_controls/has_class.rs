// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn has_class(dom: &ScriptedDom, node: NodeId, class: &str) -> bool {
    attr(dom, node, "class").is_some_and(|c| c.split_whitespace().any(|c| c == class))
}
