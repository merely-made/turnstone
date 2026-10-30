// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn custom_leaf_key<D>(dom: &D, node: D::NodeId) -> Option<u64>
where
    D: LayoutDom,
    D::NodeId: Copy,
{
    if dom.kind(node) != NodeKind::Element
        || !matches!(
            dom.element_name(node)?.local.as_ref(),
            "custom-leaf" | "chisel-leaf"
        )
    {
        return None;
    }
    dom.attribute(node, &Namespace::default(), &LocalName::from("key"))?
        .parse()
        .ok()
}
