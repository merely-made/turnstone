// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The pane's registry of block adapters, its memory of their answers, and the built-in rhyme
//! adapter. The contract (the trait, its types) and `GuestAdapter` live in `block-adapter`;
//! the guest world is `block-guest` in the graphshell protocol's WIT. A call on a host that
//! cannot answer in the frame (the web Worker) may answer `AdapterError::Pending`, and the
//! pane keeps a [`Memo`] and shows the previous answer as stale.

use super::rhyme;
use block_adapter::{
    AdapterError, Answer, BACKDROP_SCENE, BlockAdapter, BlockInput, Change, Claim, Command,
    CommandKind, Field, GuestView, Row, RunResult, ViewAsk,
};
use std::collections::{BTreeMap, HashMap};

/// The adapters the pane can call, by class.
#[derive(Default)]
pub struct Registry {
    adapters: Vec<Box<dyn BlockAdapter>>,
}

impl Registry {
    /// A registry holding the built-in adapters.
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        registry
            .register(Box::new(RhymeAdapter::new()))
            .expect("the built-in class is free");
        registry
    }

    /// Two claims for one class are refused.
    pub fn register(&mut self, adapter: Box<dyn BlockAdapter>) -> Result<(), String> {
        let class = adapter.claim().class.clone();
        if class.is_empty() || class.contains(char::is_whitespace) {
            return Err("An adapter must claim one class name".into());
        }
        if self.get(&class).is_some() {
            return Err(format!("The class {class} already has an adapter"));
        }
        self.adapters.push(adapter);
        Ok(())
    }

    pub fn remove(&mut self, class: &str) -> bool {
        let before = self.adapters.len();
        self.adapters.retain(|a| a.claim().class != class);
        self.adapters.len() != before
    }

    pub fn get(&self, class: &str) -> Option<&dyn BlockAdapter> {
        self.adapters
            .iter()
            .find(|a| a.claim().class == class)
            .map(|a| a.as_ref())
    }

    pub fn classes(&self) -> Vec<&str> {
        self.adapters
            .iter()
            .map(|a| a.claim().class.as_str())
            .collect()
    }

    /// The claims of every adapter that offers an example, for the pane's insert controls.
    pub fn examples(&self) -> Vec<&Claim> {
        self.adapters
            .iter()
            .map(|a| a.claim())
            .filter(|c| c.example.is_some())
            .collect()
    }
}

/// What the pane says about a block whose class has no adapter.
pub fn unclaimed_message(class: &str) -> String {
    format!("No block adapter is installed for the class \"{class}\"")
}

/// The pane's memory of an adapter's answers, keyed by the exact request.
///
/// A guest that answers later is asked once per distinct request. The latest answer to an
/// earlier request stays available as stale until the new one arrives, so the pane does not
/// flicker on every selection. A refusal is an answer and is kept.
#[derive(Debug)]
pub struct Memo<K: std::hash::Hash + Eq + Clone, V> {
    answers: HashMap<K, V>,
    asked: Vec<K>,
    last: Option<K>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Lookup<'a, V> {
    /// The answer to exactly this request.
    Fresh(&'a V),
    /// The answer to an earlier request; the new one is outstanding.
    Stale(&'a V),
    /// Nothing yet, and the request is outstanding.
    Pending,
}

impl<K: std::hash::Hash + Eq + Clone, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self {
            answers: HashMap::new(),
            asked: Vec::new(),
            last: None,
        }
    }
}

impl<K: std::hash::Hash + Eq + Clone, V> Memo<K, V> {
    /// Entries kept; the oldest answer goes first.
    pub const CAPACITY: usize = 32;

    /// The state of `key`. Returns `true` in the second slot when the caller must now send
    /// the request (first sight of the key); the key is then recorded as outstanding.
    pub fn lookup(&mut self, key: &K) -> (Lookup<'_, V>, bool) {
        if self.answers.contains_key(key) {
            self.last = Some(key.clone());
            return (Lookup::Fresh(&self.answers[key]), false);
        }
        let send = !self.asked.contains(key);
        if send {
            self.asked.push(key.clone());
        }
        let state = match self.last.as_ref().and_then(|k| self.answers.get(k)) {
            Some(previous) => Lookup::Stale(previous),
            None => Lookup::Pending,
        };
        (state, send)
    }

    /// Record the answer to `key`.
    pub fn answer(&mut self, key: K, value: V) {
        self.asked.retain(|k| k != &key);
        if self.answers.len() >= Self::CAPACITY && !self.answers.contains_key(&key) {
            if let Some(oldest) = self.answers.keys().next().cloned() {
                self.answers.remove(&oldest);
            }
        }
        self.last = Some(key.clone());
        self.answers.insert(key, value);
    }

    /// Forget everything (a guest was removed or the document was reopened).
    pub fn clear(&mut self) {
        self.answers.clear();
        self.asked.clear();
        self.last = None;
    }
}

/// The built-in adapter for `metre` blocks: authored rhyme groups.
pub struct RhymeAdapter {
    claim: Claim,
}

impl RhymeAdapter {
    pub const CLASS: &'static str = "metre";
    pub const SCHEME: &'static str = "rhyme:scheme";

    pub fn new() -> Self {
        Self {
            claim: Claim {
                class: Self::CLASS.into(),
                source_key: Some("rhyme".into()),
                source_versions: vec![1],
                pattern: Some(Field {
                    label: "Rhyme scheme".into(),
                    default: "ABAB".into(),
                }),
                commands: vec![
                    Command {
                        id: Self::SCHEME.into(),
                        label: "Apply rhyme scheme…".into(),
                        kind: CommandKind::Choice,
                        arg: None,
                    },
                    Command {
                        id: BACKDROP_SCENE.into(),
                        label: "Swatch scene".into(),
                        kind: CommandKind::Text,
                        arg: None,
                    },
                ],
                views: Vec::new(),
                example: None,
            },
        }
    }
}

impl Default for RhymeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn unknown(command: &str) -> AdapterError {
    AdapterError::Refused(format!("Unknown command {command}"))
}

impl BlockAdapter for RhymeAdapter {
    fn claim(&self) -> &Claim {
        &self.claim
    }
    fn rows(&self, block: &BlockInput) -> Answer<Vec<Row>> {
        Ok(block
            .body
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(i, line)| Row {
                index: i as u32,
                label: format!("{} · {}", i + 1, line),
            })
            .collect())
    }
    fn choices(&self, block: &BlockInput, command: &str) -> Answer<Vec<Change>> {
        if command != Self::SCHEME {
            return Err(unknown(command));
        }
        let rows: Vec<usize> = block.selected.iter().map(|&r| r as usize).collect();
        Ok(rhyme::choices(&block.body, &block.attrs, &rows, &block.pattern)?)
    }
    fn apply(
        &self,
        block: &BlockInput,
        command: &str,
        changes: &[Change],
    ) -> Answer<BTreeMap<String, String>> {
        if command != Self::SCHEME {
            return Err(unknown(command));
        }
        Ok(rhyme::attributes(&block.body, &block.attrs, changes)?)
    }
    fn run(&self, block: &BlockInput, command: &str, arg: &str) -> Answer<RunResult> {
        if command != BACKDROP_SCENE {
            return Err(unknown(command));
        }
        let input = rhyme::scene(arg, &block.revision, &block.body, &block.attrs)?;
        serde_json::to_string(&input)
            .map(RunResult::Text)
            .map_err(|e| AdapterError::Refused(e.to_string()))
    }
    fn view(&self, _block: &BlockInput, _ask: &ViewAsk) -> Answer<Option<GuestView>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metre(rows: Vec<u32>, pattern: &str) -> BlockInput {
        BlockInput {
            class: "metre".into(),
            body: "one\ntwo\nthree\nfour\n".into(),
            attrs: BTreeMap::from([("class".into(), "metre".into())]),
            revision: "r1".into(),
            selected: rows,
            pattern: pattern.into(),
        }
    }

    #[test]
    fn rhyme_is_the_built_in_adapter_and_matches_the_module() {
        let registry = Registry::with_builtins();
        assert_eq!(registry.classes(), ["metre"]);
        let adapter = registry.get("metre").unwrap();
        let block = metre(vec![0, 1, 2, 3], "ABAB");
        let via_trait = adapter.choices(&block, RhymeAdapter::SCHEME).unwrap();
        let direct = rhyme::choices(&block.body, &block.attrs, &[0, 1, 2, 3], "ABAB").unwrap();
        assert_eq!(via_trait, direct);
        let attrs = adapter
            .apply(&block, RhymeAdapter::SCHEME, &via_trait)
            .unwrap();
        assert_eq!(
            attrs,
            rhyme::attributes(&block.body, &block.attrs, &direct).unwrap()
        );
    }

    #[test]
    fn rhyme_offers_its_swatch_scene_as_a_hidden_text_command() {
        let registry = Registry::with_builtins();
        let adapter = registry.get("metre").unwrap();
        let block = metre(vec![], "");
        let RunResult::Text(json) = adapter.run(&block, BACKDROP_SCENE, "doc#0").unwrap() else {
            panic!("text")
        };
        let scene = sceno_backdrops::SceneInput::parse(&json).unwrap();
        assert_eq!(scene.revision, "r1");
        assert!(adapter.run(&block, "nope", "").is_err());
    }

    #[test]
    fn rhyme_rows_are_the_nonblank_lines_numbered_from_one() {
        let registry = Registry::with_builtins();
        let mut block = metre(vec![], "");
        block.body = "one\n\ntwo\n".into();
        let rows = registry.get("metre").unwrap().rows(&block).unwrap();
        assert_eq!(
            rows,
            vec![
                Row { index: 0, label: "1 · one".into() },
                Row { index: 2, label: "3 · two".into() },
            ]
        );
    }

    #[test]
    fn unknown_commands_and_classes_are_refused_by_name() {
        let registry = Registry::with_builtins();
        let adapter = registry.get("metre").unwrap();
        let block = metre(vec![0], "A");
        assert_eq!(
            adapter.choices(&block, "nope"),
            Err(AdapterError::Refused("Unknown command nope".into()))
        );
        assert!(adapter.view(&block, &ViewAsk { mode: "x".into(), width: 1.0, selected: None }).unwrap().is_none());
        assert!(registry.get("progression").is_none());
        assert_eq!(
            unclaimed_message("progression"),
            "No block adapter is installed for the class \"progression\""
        );
    }

    #[test]
    fn two_claims_for_one_class_are_refused_and_removal_frees_it() {
        let mut registry = Registry::with_builtins();
        assert!(registry.register(Box::new(RhymeAdapter::new())).is_err());
        assert!(registry.remove("metre"));
        assert!(!registry.remove("metre"));
        assert!(registry.register(Box::new(RhymeAdapter::new())).is_ok());
    }

    #[test]
    fn memo_asks_once_shows_stale_then_fresh() {
        let mut memo: Memo<u32, &str> = Memo::default();
        assert_eq!(memo.lookup(&1), (Lookup::Pending, true));
        assert_eq!(memo.lookup(&1), (Lookup::Pending, false), "asked once");
        memo.answer(1, "one");
        assert_eq!(memo.lookup(&1), (Lookup::Fresh(&"one"), false));
        assert_eq!(memo.lookup(&2), (Lookup::Stale(&"one"), true));
        assert_eq!(memo.lookup(&2), (Lookup::Stale(&"one"), false));
        memo.answer(2, "two");
        assert_eq!(memo.lookup(&2), (Lookup::Fresh(&"two"), false));
        memo.clear();
        assert_eq!(memo.lookup(&2), (Lookup::Pending, true));
    }

    #[test]
    fn memo_is_bounded() {
        let mut memo: Memo<u32, u32> = Memo::default();
        for i in 0..100 {
            memo.answer(i, i);
        }
        assert!(memo.answers.len() <= Memo::<u32, u32>::CAPACITY);
    }
}
