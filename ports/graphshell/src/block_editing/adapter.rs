// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The seam between the block pane and the adapters that edit and show one block class.
//!
//! An adapter claims one Djot div class. The pane looks it up by class and calls it with a
//! copied [`BlockInput`]; the adapter proposes [`Change`]s, returns the complete desired
//! attribute map, and may return a [`GuestView`] (a `sceno` scene naming the regions plus a
//! vector sheet the host paints). Rhyme is the built-in adapter. A guest component loaded
//! under review is the other kind: it is called through a host that cannot answer in the
//! frame (the web Worker), so a call may answer [`AdapterError::Pending`] and the pane keeps a
//! [`Memo`] and shows the previous answer as stale. The guest world is `block-guest` in the
//! graphshell protocol's WIT; nothing here names a guest's owner.

use super::{Change, rhyme};
use sceno::{Footprint, Scene};
use std::collections::{BTreeMap, HashMap};
use vector_sheet::VectorSheet;

/// What a command is for. Mirrors the WIT `command-kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    /// Proposes changes the user reviews, then applies.
    Choice,
    /// Writes new attributes at once.
    Write,
    /// Produces text to export.
    Text,
}

/// A text field the pane shows for an adapter: a label and its default text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub label: String,
    pub default: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub kind: CommandKind,
    /// A field whose text is sent as `run`'s argument.
    pub arg: Option<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewMode {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Example {
    pub title: String,
    pub body: String,
    pub attrs: BTreeMap<String, String>,
}

/// What an adapter claims. Called once when the adapter is registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    /// The Djot div class it edits.
    pub class: String,
    /// The attribute that carries its structured source, if it has one.
    pub source_key: Option<String>,
    pub source_versions: Vec<u32>,
    /// The pane's pattern field for this class, sent as [`BlockInput::pattern`].
    pub pattern: Option<Field>,
    pub commands: Vec<Command>,
    pub views: Vec<ViewMode>,
    pub example: Option<Example>,
}

/// One selectable row of a block: a bar of a progression, a line of a verse. `index` is what
/// [`BlockInput::selected`] and [`Change::index`] refer to; it need not be dense.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub index: u32,
    pub label: String,
}

/// A copied snapshot of one block; never a handle into the document.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockInput {
    pub class: String,
    pub body: String,
    pub attrs: BTreeMap<String, String>,
    /// Content hash of the whole document source.
    pub revision: String,
    /// Selected rows or bars.
    pub selected: Vec<u32>,
    /// The pane's pattern field for this adapter.
    pub pattern: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunResult {
    /// The complete attribute map after an explicit write.
    Attrs(BTreeMap<String, String>),
    /// Text for an export.
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewAsk {
    pub mode: String,
    pub width: f32,
    pub selected: Option<(u32, u32)>,
}

/// One focusable event region read from a view's scene.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRegion {
    pub bar: u32,
    pub event: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
}

/// A view: a scene that names the regions, and the sheet the host paints.
#[derive(Clone, Debug, PartialEq)]
pub struct GuestView {
    pub scene: Scene,
    pub sheet: VectorSheet,
    pub width: f32,
    pub height: f32,
    pub description: String,
}

/// The two payload version bytes a view carries (a version byte, then CBOR).
pub const SCENE_PAYLOAD_VERSION: u8 = 1;

impl GuestView {
    /// Decode the two payloads a guest's view returns, refusing any that is over the 1 MiB cap
    /// or has an unknown version byte. The sheet is validated.
    pub fn decode(
        scene: &[u8],
        sheet: &[u8],
        width: f32,
        height: f32,
        description: String,
    ) -> Result<Self, String> {
        if scene.len() > vector_sheet::MAX_PAYLOAD_BYTES {
            return Err("The view's scene is over the 1 MiB payload cap".into());
        }
        let (version, body) = scene.split_first().ok_or("The view's scene is empty")?;
        if *version != SCENE_PAYLOAD_VERSION {
            return Err(format!("Unsupported scene version {version}"));
        }
        let scene: Scene = ciborium::from_reader(body)
            .map_err(|e| format!("The view's scene did not decode: {e}"))?;
        let sheet = VectorSheet::decode(sheet).map_err(|e| e.to_string())?;
        Ok(Self {
            scene,
            sheet,
            width,
            height,
            description,
        })
    }

    /// The event regions the scene names: one per region that has a label and a member item
    /// whose source id is `bar:<n>/event:<m>`. Items without that id are not events.
    pub fn regions(&self) -> Vec<EventRegion> {
        let mut out = Vec::new();
        for region in &self.scene.regions {
            let (Some(label), Some(&member)) = (region.label.as_ref(), region.members.first())
            else {
                continue;
            };
            let Some(item) = self.scene.items.get(member.0 as usize) else {
                continue;
            };
            let Some(source) = self.scene.sources.get(item.source.0 as usize) else {
                continue;
            };
            let Some((bar, event)) = parse_event_id(&source.id) else {
                continue;
            };
            let Footprint::Rect { size } = item.hit.as_ref().unwrap_or(&item.footprint) else {
                continue;
            };
            let c = item.transform.translate;
            out.push(EventRegion {
                bar,
                event,
                x: c.x - size.w / 2.0,
                y: c.y - size.h / 2.0,
                w: size.w,
                h: size.h,
                label: label.clone(),
            });
        }
        out
    }
}

fn parse_event_id(id: &str) -> Option<(u32, u32)> {
    let rest = id.strip_prefix("bar:")?;
    let (bar, event) = rest.split_once("/event:")?;
    Some((bar.parse().ok()?, event.parse().ok()?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterError {
    /// The adapter cannot answer in this frame; ask again after the host wakes.
    Pending,
    /// The adapter refused, with the text to show.
    Refused(String),
}

impl From<String> for AdapterError {
    fn from(text: String) -> Self {
        Self::Refused(text)
    }
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("Waiting for the block adapter"),
            Self::Refused(text) => f.write_str(text),
        }
    }
}

pub type Answer<T> = Result<T, AdapterError>;

/// An adapter for one block class.
pub trait BlockAdapter {
    fn claim(&self) -> &Claim;
    /// The block's selectable rows, in order.
    fn rows(&self, block: &BlockInput) -> Answer<Vec<Row>>;
    /// Proposed edits for the selection; `command` is a [`CommandKind::Choice`] command.
    fn choices(&self, block: &BlockInput, command: &str) -> Answer<Vec<Change>>;
    /// The complete desired attribute map after the user's decisions.
    fn apply(
        &self,
        block: &BlockInput,
        command: &str,
        changes: &[Change],
    ) -> Answer<BTreeMap<String, String>>;
    /// An explicit write or text export.
    fn run(&self, block: &BlockInput, command: &str, arg: &str) -> Answer<RunResult>;
    /// A view, or `Ok(None)` when the adapter has none.
    fn view(&self, block: &BlockInput, ask: &ViewAsk) -> Answer<Option<GuestView>>;
}

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
                commands: vec![Command {
                    id: Self::SCHEME.into(),
                    label: "Apply rhyme scheme…".into(),
                    kind: CommandKind::Choice,
                    arg: None,
                }],
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
    fn run(&self, _block: &BlockInput, command: &str, _arg: &str) -> Answer<RunResult> {
        Err(unknown(command))
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

    fn event_scene(source_id: &str, labelled: bool) -> GuestView {
        use sceno::{
            ProjectedItem, Region, Representation, Size2, SourceIx, SourceRef, Transform2, Vec2,
        };
        let mut scene = Scene::new();
        scene.sources.push(SourceRef::new("woodshed.event", source_id));
        scene.items.push(ProjectedItem {
            source: SourceIx(0),
            space: Scene::WORLD,
            transform: Transform2 { translate: Vec2::new(60.0, 40.0), ..Transform2::IDENTITY },
            footprint: Footprint::Rect { size: Size2::new(20.0, 30.0) },
            representation: Representation::Open { kind: "event".into() },
            layer: 1,
            visible: true,
            hit: None,
            channels: vec![],
        });
        scene.regions.push(Region {
            members: vec![sceno::InstanceId(0)],
            space: Scene::WORLD,
            contour: None,
            label: labelled.then(|| "Bar 2, event 3".to_string()),
            confidence: 1.0,
        });
        GuestView {
            scene,
            sheet: VectorSheet { width: 100.0, height: 80.0, ops: vec![] },
            width: 100.0,
            height: 80.0,
            description: String::new(),
        }
    }

    #[test]
    fn a_view_decodes_from_its_two_payloads_and_refuses_bad_ones() {
        let view = event_scene("bar:0/event:0", true);
        let mut scene = vec![SCENE_PAYLOAD_VERSION];
        ciborium::into_writer(&view.scene, &mut scene).unwrap();
        let sheet = VectorSheet {
            width: 10.0,
            height: 10.0,
            ops: vec![],
        };
        let sheet_bytes = sheet.encode().unwrap();
        let decoded = GuestView::decode(&scene, &sheet_bytes, 10.0, 10.0, "d".into()).unwrap();
        assert_eq!(decoded.scene, view.scene);
        assert_eq!(decoded.regions().len(), 1);
        assert!(GuestView::decode(&[], &sheet_bytes, 1.0, 1.0, String::new()).is_err());
        assert!(GuestView::decode(&[9, 0], &sheet_bytes, 1.0, 1.0, String::new()).is_err());
        assert!(GuestView::decode(&scene, &[1, 0xff], 1.0, 1.0, String::new()).is_err());
        let huge = vec![1u8; vector_sheet::MAX_PAYLOAD_BYTES + 1];
        assert!(GuestView::decode(&huge, &sheet_bytes, 1.0, 1.0, String::new()).is_err());
    }

    #[test]
    fn regions_come_from_the_scene() {
        let view = event_scene("bar:1/event:2", true);
        assert_eq!(
            view.regions(),
            vec![EventRegion { bar: 1, event: 2, x: 50.0, y: 25.0, w: 20.0, h: 30.0, label: "Bar 2, event 3".into() }]
        );
        assert!(event_scene("bar:1/event:2", false).regions().is_empty());
        assert!(event_scene("something-else", true).regions().is_empty());
    }
}
