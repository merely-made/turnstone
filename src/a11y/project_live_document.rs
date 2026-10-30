// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One live document's subtree, from the mirrored structural outline. The
/// root announces as a document and DECLARES the partial capability; outline
/// entries become flat children with mapped roles (depth is structural
/// bookkeeping — nesting reconstruction is a follow-on, said in the
/// description rather than faked).
pub(super) fn project_live_document(app: &App, node: uuid::Uuid, url: &str) -> UxTree {
    let root_path = format!("turnstone/doc/{node}");
    let root_id = node_id_for_path(&root_path);
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let structure = app.content.facts(node).and_then(|f| f.structure.as_ref());
    if let Some(s) = structure {
        for (i, entry) in s.outline.iter().enumerate() {
            let id = node_id_for_path(&format!("{root_path}/outline/{i}"));
            let mut n = Node::new(outline_role(entry));
            if !entry.name.is_empty() {
                n.set_label(entry.name.clone());
            }
            nodes.push((id, n));
            children.push(id);
        }
    }
    let mut root = Node::new(Role::Document);
    match structure.and_then(|s| s.title.as_deref()) {
        Some(title) => root.set_label(title.to_string()),
        None => root.set_label(url.to_string()),
    }
    // The honest capability declaration, on the node itself.
    root.set_description(match structure {
        Some(_) => "structural outline only: no bounds, no per-element focus",
        None => "no structural read for this lane",
    });
    root.set_children(children);
    nodes.push((root_id, root));
    UxTree {
        root: root_id,
        nodes,
    }
}
