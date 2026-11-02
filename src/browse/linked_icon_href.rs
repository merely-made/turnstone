// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn linked_icon_href(doc: &genet_static_dom::StaticDocument) -> Option<String> {
    let namespace = layout_dom_api::Namespace::default();
    let rel = layout_dom_api::LocalName::from("rel");
    let href = layout_dom_api::LocalName::from("href");
    let mut pending = vec![doc.document()];
    while let Some(node) = pending.pop() {
        pending.extend(doc.dom_children(node));
        if !doc
            .element_name(node)
            .is_some_and(|name| str::eq_ignore_ascii_case(&name.local, "link"))
        {
            continue;
        }
        let Some(relation) = doc.attribute(node, &namespace, &rel) else {
            continue;
        };
        if relation
            .split_ascii_whitespace()
            .any(|token| token.eq_ignore_ascii_case("icon"))
        {
            return doc.attribute(node, &namespace, &href).map(str::to_owned);
        }
    }
    None
}
