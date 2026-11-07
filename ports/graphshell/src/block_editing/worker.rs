// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! A block adapter that answers later: the web host runs the guest component in a Worker.
//!
//! The pane calls an adapter in the frame, a Worker cannot answer in the frame, so this
//! adapter sends each distinct request once through a [`Transport`] (JSON text, a Worker
//! message on the web), answers [`AdapterError::Pending`] until the reply is [`deliver`]ed,
//! and then answers from its [`Memo`]. The request is the key, so selecting the same bar
//! again does not ask the guest again, and a reply to an old request never answers a new one.
//!
//! Wire: a request is `{"call": "...", "block": {...}, ...}`; a reply is `{"ok": value}` or
//! `{"err": text}`. Byte payloads (the view's scene and sheet) are base64 text.
//!
//! [`deliver`]: WorkerAdapter::deliver

use super::Change;
use super::adapter::{
    Answer, AdapterError, BlockAdapter, BlockInput, Claim, Command, CommandKind, Example,
    Field, GuestView, Lookup, Memo, Row, RunResult, ViewAsk, ViewMode,
};
use base64::Engine as _;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// Where a request goes: a Worker message on the web, a script in tests.
pub trait Transport {
    /// Send one request. The reply comes back through [`WorkerAdapter::deliver`] with `id`.
    fn send(&self, id: u64, request: &str);
}

struct Shared {
    transport: Box<dyn Transport>,
    next: Cell<u64>,
    memo: RefCell<Memo<String, Result<Value, String>>>,
    inflight: RefCell<HashMap<u64, String>>,
}

/// An adapter whose guest answers asynchronously.
#[derive(Clone)]
pub struct WorkerAdapter {
    claim: Claim,
    shared: Rc<Shared>,
}

fn attrs_json(attrs: &BTreeMap<String, String>) -> Value {
    Value::Array(
        attrs
            .iter()
            .map(|(k, v)| json!({"key": k, "value": v}))
            .collect(),
    )
}

fn block_json(b: &BlockInput) -> Value {
    json!({
        "class": b.class, "body": b.body, "attrs": attrs_json(&b.attrs),
        "revision": b.revision, "selected": b.selected, "pattern": b.pattern,
    })
}

fn change_json(c: &Change) -> Value {
    json!({
        "index": c.index, "label": c.label, "before": c.before, "after": c.after,
        "conflict": c.conflict, "use_update": c.use_update,
    })
}

fn str_field(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("The adapter's reply has no {key}"))
}

fn field_from(v: &Value) -> Result<Field, String> {
    Ok(Field {
        label: str_field(v, "label")?,
        default: str_field(v, "default")?,
    })
}

fn attrs_from(v: &Value) -> Result<BTreeMap<String, String>, String> {
    v.as_array()
        .ok_or("The adapter's reply has no attributes")?
        .iter()
        .map(|a| Ok((str_field(a, "key")?, str_field(a, "value")?)))
        .collect()
}

impl WorkerAdapter {
    /// Decode the claim a Worker's `describe` returned (`reply` is the `ok` value).
    pub fn claim_from_json(v: &Value) -> Result<Claim, String> {
        let commands = v
            .get("commands")
            .and_then(Value::as_array)
            .ok_or("The claim has no commands")?
            .iter()
            .map(|c| {
                Ok(Command {
                    id: str_field(c, "id")?,
                    label: str_field(c, "label")?,
                    kind: match str_field(c, "kind")?.as_str() {
                        "choice" => CommandKind::Choice,
                        "write" => CommandKind::Write,
                        "text" => CommandKind::Text,
                        other => return Err(format!("Unknown command kind {other}")),
                    },
                    arg: c.get("arg").filter(|a| !a.is_null()).map(field_from).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let views = v
            .get("views")
            .and_then(Value::as_array)
            .ok_or("The claim has no views")?
            .iter()
            .map(|m| {
                Ok(ViewMode {
                    id: str_field(m, "id")?,
                    label: str_field(m, "label")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let example = v
            .get("example")
            .filter(|e| !e.is_null())
            .map(|e| -> Result<Example, String> {
                Ok(Example {
                    title: str_field(e, "title")?,
                    body: str_field(e, "body")?,
                    attrs: attrs_from(e.get("attrs").ok_or("The example has no attributes")?)?,
                })
            })
            .transpose()?;
        let claim = Claim {
            class: str_field(v, "class")?,
            source_key: Some(str_field(v, "source_key")?).filter(|k| !k.is_empty()),
            source_versions: v
                .get("source_versions")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|n| n.as_u64().map(|n| n as u32)).collect())
                .unwrap_or_default(),
            pattern: v.get("pattern").filter(|p| !p.is_null()).map(field_from).transpose()?,
            commands,
            views,
            example,
        };
        if claim.class.is_empty() || claim.class.contains(char::is_whitespace) {
            return Err("The adapter must claim one class name".into());
        }
        Ok(claim)
    }

    pub fn new(claim: Claim, transport: impl Transport + 'static) -> Self {
        Self {
            claim,
            shared: Rc::new(Shared {
                transport: Box::new(transport),
                next: Cell::new(1),
                memo: RefCell::new(Memo::default()),
                inflight: RefCell::new(HashMap::new()),
            }),
        }
    }

    /// Record the reply to request `id` (`{"ok": ...}` or `{"err": ...}`). Returns whether it
    /// answered an outstanding request, so the host knows to wake the pane.
    pub fn deliver(&self, id: u64, reply: &str) -> bool {
        let Some(key) = self.shared.inflight.borrow_mut().remove(&id) else {
            return false;
        };
        let answer = match serde_json::from_str::<Value>(reply) {
            Ok(v) if v.get("ok").is_some() => Ok(v["ok"].clone()),
            Ok(v) => Err(v
                .get("err")
                .and_then(Value::as_str)
                .unwrap_or("The adapter refused")
                .to_owned()),
            Err(e) => Err(format!("The adapter's reply did not parse: {e}")),
        };
        self.shared.memo.borrow_mut().answer(key, answer);
        true
    }

    /// Forget every answer (the document was reopened or the adapter replaced).
    pub fn forget(&self) {
        self.shared.memo.borrow_mut().clear();
        self.shared.inflight.borrow_mut().clear();
    }

    fn call(&self, request: Value) -> Answer<Value> {
        let key = request.to_string();
        let mut memo = self.shared.memo.borrow_mut();
        let (state, send) = memo.lookup(&key);
        match state {
            Lookup::Fresh(Ok(v)) => return Ok(v.clone()),
            Lookup::Fresh(Err(e)) => return Err(AdapterError::Refused(e.clone())),
            Lookup::Stale(_) | Lookup::Pending => {},
        }
        if send {
            let id = self.shared.next.get();
            self.shared.next.set(id + 1);
            self.shared.inflight.borrow_mut().insert(id, key.clone());
            self.shared.transport.send(id, &key);
        }
        Err(AdapterError::Pending)
    }
}

impl BlockAdapter for WorkerAdapter {
    fn claim(&self) -> &Claim {
        &self.claim
    }

    fn rows(&self, block: &BlockInput) -> Answer<Vec<Row>> {
        let v = self.call(json!({"call": "rows", "block": block_json(block)}))?;
        v.as_array()
            .ok_or("The adapter's rows did not decode")?
            .iter()
            .map(|r| {
                Ok(Row {
                    index: r.get("index").and_then(Value::as_u64).ok_or("A row has no index")? as u32,
                    label: str_field(r, "label")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(AdapterError::Refused)
    }

    fn choices(&self, block: &BlockInput, command: &str) -> Answer<Vec<Change>> {
        let v = self.call(json!({"call": "choices", "block": block_json(block), "command": command}))?;
        v.as_array()
            .ok_or("The adapter's choices did not decode")?
            .iter()
            .map(|c| {
                Ok(Change {
                    index: c.get("index").and_then(Value::as_u64).ok_or("A change has no index")? as usize,
                    label: str_field(c, "label")?,
                    before: c.get("before").and_then(Value::as_str).map(str::to_owned),
                    after: str_field(c, "after")?,
                    conflict: c.get("conflict").and_then(Value::as_bool).unwrap_or(false),
                    use_update: c.get("use_update").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(AdapterError::Refused)
    }

    fn apply(
        &self,
        block: &BlockInput,
        command: &str,
        changes: &[Change],
    ) -> Answer<BTreeMap<String, String>> {
        let v = self.call(json!({
            "call": "apply", "block": block_json(block), "command": command,
            "changes": changes.iter().map(change_json).collect::<Vec<_>>(),
        }))?;
        attrs_from(&v).map_err(AdapterError::Refused)
    }

    fn run(&self, block: &BlockInput, command: &str, arg: &str) -> Answer<RunResult> {
        let v = self.call(json!({"call": "run", "block": block_json(block), "command": command, "arg": arg}))?;
        if let Some(attrs) = v.get("attrs") {
            attrs_from(attrs).map(RunResult::Attrs).map_err(AdapterError::Refused)
        } else if let Some(text) = v.get("text").and_then(Value::as_str) {
            Ok(RunResult::Text(text.to_owned()))
        } else {
            Err(AdapterError::Refused("The adapter's run result did not decode".into()))
        }
    }

    fn view(&self, block: &BlockInput, ask: &ViewAsk) -> Answer<Option<GuestView>> {
        let v = self.call(json!({
            "call": "view", "block": block_json(block),
            "ask": {"mode": ask.mode, "width": ask.width, "selected": ask.selected},
        }))?;
        let engine = base64::engine::general_purpose::STANDARD;
        let bytes = |key: &str| -> Result<Vec<u8>, String> {
            engine
                .decode(str_field(&v, key)?)
                .map_err(|e| format!("The view's {key} is not base64: {e}"))
        };
        let view = GuestView::decode(
            &bytes("scene").map_err(AdapterError::Refused)?,
            &bytes("sheet").map_err(AdapterError::Refused)?,
            v.get("width").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            v.get("height").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            str_field(&v, "description").unwrap_or_default(),
        )
        .map_err(AdapterError::Refused)?;
        Ok(Some(view))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct Script(Rc<RefCell<Vec<(u64, String)>>>);
    impl Transport for Script {
        fn send(&self, id: u64, request: &str) {
            self.0.borrow_mut().push((id, request.to_owned()));
        }
    }

    fn claim() -> Claim {
        WorkerAdapter::claim_from_json(&json!({
            "class": "fixture", "source_key": "fixture", "source_versions": [1],
            "pattern": {"label": "P", "default": "xx"},
            "commands": [{"id": "mark", "label": "Mark", "kind": "choice", "arg": null},
                         {"id": "stamp", "label": "Stamp", "kind": "write", "arg": {"label": "T", "default": "s0"}}],
            "views": [{"id": "plain", "label": "Plain"}],
            "example": {"title": "Fixture", "body": "hi\n", "attrs": [{"key": "class", "value": "fixture"}]},
        }))
        .unwrap()
    }

    fn block() -> BlockInput {
        BlockInput {
            class: "fixture".into(),
            body: "one\ntwo\n".into(),
            attrs: BTreeMap::from([("class".into(), "fixture".into())]),
            revision: "r1".into(),
            selected: vec![0],
            pattern: String::new(),
        }
    }

    #[test]
    fn the_claim_decodes_with_fields_commands_views_and_example() {
        let c = claim();
        assert_eq!(c.class, "fixture");
        assert_eq!(c.pattern.unwrap().default, "xx");
        assert_eq!(c.commands[1].arg.as_ref().unwrap().label, "T");
        assert_eq!(c.commands[0].kind, CommandKind::Choice);
        assert_eq!(c.example.unwrap().attrs["class"], "fixture");
        assert!(WorkerAdapter::claim_from_json(&json!({"class": "has space", "source_key": "", "commands": [], "views": []})).is_err());
    }

    #[test]
    fn a_request_is_sent_once_answers_pending_then_from_the_memo() {
        let script = Script::default();
        let adapter = WorkerAdapter::new(claim(), script.clone());
        assert_eq!(adapter.rows(&block()), Err(AdapterError::Pending));
        assert_eq!(adapter.rows(&block()), Err(AdapterError::Pending));
        assert_eq!(script.0.borrow().len(), 1, "the same request is not sent twice");
        let (id, request) = script.0.borrow()[0].clone();
        assert!(request.contains("\"call\":\"rows\""));
        assert!(adapter.deliver(id, r#"{"ok":[{"index":0,"label":"one"}]}"#));
        assert_eq!(
            adapter.rows(&block()).unwrap(),
            vec![Row { index: 0, label: "one".into() }]
        );
        assert_eq!(script.0.borrow().len(), 1, "answered from the memo");
        assert!(!adapter.deliver(id, r#"{"ok":[]}"#), "a late duplicate answers nothing");
    }

    #[test]
    fn a_new_selection_is_a_new_request_and_refusals_are_answers() {
        let script = Script::default();
        let adapter = WorkerAdapter::new(claim(), script.clone());
        let _ = adapter.choices(&block(), "mark");
        let mut other = block();
        other.selected = vec![1];
        let _ = adapter.choices(&other, "mark");
        assert_eq!(script.0.borrow().len(), 2);
        let (id, _) = script.0.borrow()[0].clone();
        adapter.deliver(id, r#"{"err":"Select a leaf block"}"#);
        assert_eq!(
            adapter.choices(&block(), "mark"),
            Err(AdapterError::Refused("Select a leaf block".into()))
        );
        assert_eq!(adapter.choices(&other, "mark"), Err(AdapterError::Pending));
    }

    #[test]
    fn choices_apply_and_run_decode() {
        let script = Script::default();
        let adapter = WorkerAdapter::new(claim(), script.clone());
        let _ = adapter.choices(&block(), "mark");
        let id = script.0.borrow()[0].0;
        adapter.deliver(id, r#"{"ok":[{"index":0,"label":"Row 1","before":null,"after":"x","conflict":false,"use_update":true}]}"#);
        let changes = adapter.choices(&block(), "mark").unwrap();
        assert_eq!(changes[0].after, "x");
        assert!(changes[0].use_update);
        let _ = adapter.apply(&block(), "mark", &changes);
        let id = script.0.borrow()[1].0;
        adapter.deliver(id, r#"{"ok":[{"key":"class","value":"fixture"},{"key":"fixture","value":"marked:1"}]}"#);
        assert_eq!(adapter.apply(&block(), "mark", &changes).unwrap()["fixture"], "marked:1");
        let _ = adapter.run(&block(), "stamp", "s1");
        let id = script.0.borrow()[2].0;
        adapter.deliver(id, r#"{"ok":{"text":"done"}}"#);
        assert_eq!(adapter.run(&block(), "stamp", "s1").unwrap(), RunResult::Text("done".into()));
    }

    #[test]
    fn forgetting_makes_the_next_call_a_fresh_request() {
        let script = Script::default();
        let adapter = WorkerAdapter::new(claim(), script.clone());
        let _ = adapter.rows(&block());
        let id = script.0.borrow()[0].0;
        adapter.deliver(id, r#"{"ok":[]}"#);
        assert!(adapter.rows(&block()).is_ok());
        adapter.forget();
        assert_eq!(adapter.rows(&block()), Err(AdapterError::Pending));
        assert_eq!(script.0.borrow().len(), 2);
    }

    #[test]
    fn a_malformed_reply_is_a_refusal_not_a_panic() {
        let script = Script::default();
        let adapter = WorkerAdapter::new(claim(), script.clone());
        let _ = adapter.rows(&block());
        let id = script.0.borrow()[0].0;
        adapter.deliver(id, "not json");
        assert!(matches!(adapter.rows(&block()), Err(AdapterError::Refused(_))));
    }
}
