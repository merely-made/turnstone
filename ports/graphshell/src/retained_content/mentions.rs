// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Faces beside guest links that name graph nodes (iconography T4, D3).
//!
//! The guest's DOM is never changed. At spawn the engine hands the host the
//! parsed document; each `<a href>` whose resolved address is a node gets one
//! host-sheet rule putting that node's face in its `::before`, and the face
//! is registered as a vector image under the URL the rule names.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, RwLock};

use layout_dom_api::{LayoutDom, LocalName, Namespace, NodeKind};
use mere::canvas::{InlineFaces, inline_face_style, inline_face_url};
use pelt_core::native::{LiveryHostLayer, LiveryHostLayerFn};

/// The graph's node addresses and the faces to draw them with, refreshed by
/// the host as the graph and appearance change.
pub struct Mentions {
    pub addresses: HashSet<String>,
    pub faces: Arc<InlineFaces>,
    /// Device pixels per CSS pixel the reading pane renders at.
    pub scale: f32,
}

/// The engine hook reading `mentions` at each spawn.
pub fn host_layer(mentions: Arc<RwLock<Mentions>>) -> LiveryHostLayerFn {
    Arc::new(move |dom, base| {
        let mentions = mentions.read().unwrap_or_else(|poison| poison.into_inner());
        layer(dom, base, &mentions)
    })
}

/// The host layer for one parsed document at `base`.
pub fn layer<D: LayoutDom>(dom: &D, base: Option<&str>, mentions: &Mentions) -> LiveryHostLayer {
    let base = base.and_then(|base| url::Url::parse(base).ok());
    let href = LocalName::from("href");
    let mut faced: BTreeMap<String, String> = BTreeMap::new();
    let mut stack = vec![dom.document()];
    while let Some(node) = stack.pop() {
        stack.extend(dom.dom_children(node));
        if dom.kind(node) != NodeKind::Element
            || dom.element_name(node).is_none_or(|name| &*name.local != "a")
        {
            continue;
        }
        let Some(raw) = dom.attribute(node, &Namespace::default(), &href) else {
            continue;
        };
        let resolved = match &base {
            Some(base) => base.join(raw).ok().map(String::from),
            None => url::Url::parse(raw).ok().map(String::from),
        };
        if let Some(address) = resolved.filter(|a| mentions.addresses.contains(a)) {
            faced.insert(raw.to_owned(), address);
        }
    }
    let mut out = LiveryHostLayer::default();
    let size = inline_face_style(mentions.scale);
    for (raw, address) in faced {
        let face = inline_face_url(&address);
        out.css.push(format!(
            // No margin: a margin on a generated image breaks its owner's
            // inline fragment in Livery today.
            "a[href=\"{}\"]::before {{ content: url(\"{face}\") !important; {size}; }}",
            css_string(&raw)
        ));
        if let Some(image) = mentions.faces.resolve_image(&face, mentions.scale) {
            out.vectors.push((face, image));
        }
    }
    out
}

/// `text` escaped for a double-quoted CSS string.
fn css_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            },
            '\n' | '\r' | '\u{c}' => out.push_str(&format!("\\{:x} ", ch as u32)),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use mere::canvas::{DerivedFacePalette, TextFacePalette};

    use super::*;

    fn mentions(addresses: &[&str]) -> Mentions {
        Mentions {
            addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
            faces: Arc::new(InlineFaces::new(TextFacePalette::new(
                DerivedFacePalette::default(),
                &[],
            ))),
            scale: 2.0,
        }
    }

    #[test]
    fn only_links_naming_a_node_get_a_face() {
        let dom = genet_scripted_dom::ScriptedDom::from_serialized_document(
            "<p><a href=\"notes\">Notes</a> <a href=\"https://elsewhere.test/\">x</a> \
             <a href=\"notes\">again</a> <a>no href</a></p>",
        );
        let layer = layer(
            &dom,
            Some("https://site.test/dir/page"),
            &mentions(&["https://site.test/dir/notes"]),
        );
        assert_eq!(layer.css.len(), 1, "one rule per distinct href: {:?}", layer.css);
        let face = inline_face_url("https://site.test/dir/notes");
        assert!(layer.css[0].starts_with("a[href=\"notes\"]::before"));
        assert!(layer.css[0].contains(&format!("url(\"{face}\")")));
        assert!(layer.css[0].contains("0.5px"), "sized for scale 2: {}", layer.css[0]);
        assert_eq!(layer.vectors.len(), 1);
        assert_eq!(layer.vectors[0].0, face);
    }

    #[test]
    fn an_href_cannot_escape_its_selector() {
        assert_eq!(css_string(r#"a"] b\"#), r#"a\"] b\\"#);
        assert_eq!(css_string("a\nb"), "a\\a b");
    }
}
