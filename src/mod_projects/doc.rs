// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A project's document as an outline (mod projects MP5): the same form a
//! fixture's `tree` uses, two spaces a level, `#text` then a JSON string.
//! document-host keeps its own reader private, so this is a copy of it.

use document_host::dom_view;
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};

/// The document a project starts with when it names none.
pub const DEFAULT_DOCUMENT: &str = "body\n  p\n    #text \"Hello\"\n";

fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

/// Parse an outline into a document.
pub fn parse(tree: &str) -> Result<ScriptedDom, String> {
    let mut dom = ScriptedDom::new();
    let mut stack: Vec<NodeId> = vec![dom.document()];
    for (n, line) in tree.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let depth = indent / 2;
        if indent % 2 != 0 || depth >= stack.len() {
            return Err(format!("document line {}: bad indentation", n + 1));
        }
        stack.truncate(depth + 1);
        let body = line.trim();
        let node = if let Some(rest) = body.strip_prefix("#text") {
            let text: String = serde_json::from_str(rest.trim())
                .map_err(|e| format!("document line {}: #text wants a JSON string ({e})", n + 1))?;
            dom.create_text(&text)
        } else if body.starts_with('#') || body.contains(char::is_whitespace) {
            return Err(format!("document line {}: unknown node '{body}'", n + 1));
        } else {
            dom.create_element(qual(body))
        };
        dom.append_child(stack[depth], node);
        stack.push(node);
    }
    Ok(dom)
}

/// A document as an outline, one line per node.
pub fn outline(dom: &ScriptedDom) -> Vec<String> {
    let view = dom_view::snapshot(dom, 0);
    let mut depth: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for node in view.nodes.iter().skip(1) {
        let d = node.parent.and_then(|p| depth.get(&p).copied()).map_or(0, |p| p + 1);
        depth.insert(node.id, d);
        let label = if node.kind == "#text" {
            format!("#text {}", serde_json::to_string(&node.text).unwrap_or_default())
        } else {
            node.kind.clone()
        };
        out.push(format!("{}{label}", "  ".repeat(d)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outline_round_trips_and_a_bad_line_is_named() {
        let dom = parse(DEFAULT_DOCUMENT).unwrap();
        assert_eq!(outline(&dom).join("\n") + "\n", DEFAULT_DOCUMENT);
        assert!(parse("body\n   p").err().unwrap().contains("line 2"));
    }
}
