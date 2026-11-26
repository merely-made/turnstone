//! A starter document mod that imports no capability: `set` rewrites the first
//! text node and `append` adds a paragraph under `<body>`.

wit_bindgen::generate!({
    path: "wit",
    world: "document-core",
});

use crate::mere::script::document::{
    AppendArgs, Block, DocumentQuery, Mutation, SetTextArgs, ViewNode,
};
use crate::mere::script::document_host::inspect;
use crate::mere::script::log::log;

struct Component;

fn first<'a>(nodes: &'a [ViewNode], kind: &str) -> Option<&'a ViewNode> {
    nodes.iter().find(|n| n.kind == kind)
}

impl Guest for Component {
    fn activate() {
        log("notes-mod: activated");
    }

    fn deactivate() {
        log("notes-mod: deactivated");
    }

    fn handle_event(ev: Event) -> Result<ApplyBatch, TurnError> {
        let view = inspect(DocumentQuery::WholeDocument);
        log(&format!("notes-mod: turn '{}' at rev {}", ev.kind, view.revision));
        let changes = match ev.kind.as_str() {
            "set" => {
                let t = first(&view.nodes, "#text")
                    .ok_or_else(|| TurnError::Refused("no text node".to_string()))?;
                vec![Mutation::SetText(SetTextArgs { node: t.id, text: ev.payload })]
            }
            "append" => {
                let body = first(&view.nodes, "body")
                    .ok_or_else(|| TurnError::Refused("no body".to_string()))?;
                let new = Block { kind: "p".to_string(), text: String::new() };
                vec![Mutation::AppendChild(AppendArgs { parent: body.id, new })]
            }
            _ => Vec::new(),
        };
        Ok(ApplyBatch { expected_revision: view.revision, changes })
    }
}

export!(Component);
