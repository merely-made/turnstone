// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The block pane, independent of the host that shows it.
//!
//! One ordinary source editor, contextual operations, and an independent preview. The pane
//! edits Djot documents and offers each attributed block to the adapter that claims its class
//! (`block_editing::adapter`). Rhyme is built in; any other adapter is a guest component loaded
//! under review (the web Worker's `WorkerAdapter`, or the native `GuestAdapter`). A block whose
//! class no adapter claims stays editable as text, its command disabled and named.
//!
//! A host (the web page, the native window) implements [`PaneHost`]: it owns a [`Pane`], says how
//! big the window is and what colours to use, and supplies the effects the pane cannot perform
//! itself ([`PanePorts`]: storage, exporting a file, running a future).
use crate::block_editing::{
    Change, DefaultEditors, Document, Preview,
    adapter::{
        AdapterError, BACKDROP_SCENE, BlockAdapter, BlockInput, CommandKind, GuestView, Registry,
        Row, RunResult, ViewAsk, unclaimed_message,
    },
    sheet_leaf::{ColorF, SheetLeaf, SheetTheme},
    store::{self, DocumentStore, StoredDocument},
    worker::WorkerAdapter,
};
use cambium::{
    AnyView, CaretAffinity, CaretPosition, CaretSelection, Command, CommandChoices, CommandSet,
    EditorFactory, GenetCtx, GenetElement, TextCommand,
    TextInput,
    button, custom_leaf, el, lens, status_note, text_field_typed, textarea_typed,
};
use cambium_rootstock::{
    AppCtx, NodeId, ScriptedDom, ProducedTexture, ProducerContext, SourceAlpha, SourceEncoding, TextureProducer,
};
use mere::canvas::backdrop_surface::{BackdropSurface, SurfaceTheme};
use netrender::ColorLoad;
use sceno_backdrops::SceneInput;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

mod woodshed_passage;
pub use woodshed_passage::woodshed_passage;

/// The retained canvas's clear colour, independent of any presentation adapter.
const SHELL_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.027,
    g: 0.047,
    b: 0.059,
    a: 1.0,
};

/// A view of the pane's host page.
pub type Child<P> = Box<dyn AnyView<P, (), GenetCtx, GenetElement>>;
/// The host's view function.
pub type Logic<P> = fn(&P) -> Child<P>;
/// A future the host runs to completion on its own executor.
pub type Task = Pin<Box<dyn Future<Output = ()>>>;

/// Where the document lives: one slot, read and replaced with a compare.
pub trait Storage {
    fn load(&self, slot: &str)
    -> Pin<Box<dyn Future<Output = Result<Option<(StoredDocument, Vec<u8>)>, String>>>>;
    fn save(
        &self,
        slot: &str,
        expected: Option<Vec<u8>>,
        document: StoredDocument,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>>;
    /// A store that refuses every save: the pane shows the saved document and
    /// offers nothing that edits it.
    fn read_only(&self) -> bool {
        false
    }
}

/// What a read-only pane answers when asked to edit or save.
pub const VIEW_ONLY: &str = "Viewing only · this view stores no document";

/// A [`Storage`] that reads through `S` and refuses every save.
pub struct ReadOnly<S>(pub S);

impl<S: Storage> Storage for ReadOnly<S> {
    fn load(&self, slot: &str)
    -> Pin<Box<dyn Future<Output = Result<Option<(StoredDocument, Vec<u8>)>, String>>>> {
        self.0.load(slot)
    }
    fn save(
        &self,
        _slot: &str,
        _expected: Option<Vec<u8>>,
        _document: StoredDocument,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>> {
        Box::pin(async { Err(VIEW_ONLY.into()) })
    }
    fn read_only(&self) -> bool {
        true
    }
}

/// Any muniment backend is a [`Storage`].
pub struct BackendStorage<B>(pub B);

impl<B: DocumentStore + Clone + 'static> Storage for BackendStorage<B> {
    fn load(&self, slot: &str)
    -> Pin<Box<dyn Future<Output = Result<Option<(StoredDocument, Vec<u8>)>, String>>>> {
        let (backend, slot) = (self.0.clone(), slot.to_owned());
        Box::pin(async move { store::load(&backend, &slot).await })
    }
    fn save(
        &self,
        slot: &str,
        expected: Option<Vec<u8>>,
        document: StoredDocument,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>> {
        let (backend, slot) = (self.0.clone(), slot.to_owned());
        Box::pin(async move { store::save(&backend, &slot, expected, document).await })
    }
}

/// The effects a host performs for the pane.
pub trait PanePorts {
    /// Run `task` on the host's executor.
    fn spawn(&self, task: Task);
    /// Offer `bytes` for download; returns a handle the pane gives back to [`Self::release_export`].
    fn export(&self, bytes: &[u8], mime: &str, name: &str) -> Result<String, String>;
    fn release_export(&self, handle: &str);
}

/// What the swatch's producer asks the host for each frame.
pub struct SwatchStyle {
    pub revision: u64,
    pub font_roles: String,
    pub theme: SurfaceTheme,
}

/// The page that owns a [`Pane`].
pub trait PaneHost: 'static + Sized {
    fn pane(&self) -> &Pane;
    fn pane_mut(&mut self) -> &mut Pane;
    /// The window's logical size.
    fn size(&self) -> (u32, u32);
    fn sheet_theme(&self) -> SheetTheme;
    /// The pane's background colour, which a sheet's text must read against. A host that does
    /// not say keeps each text role's colour as the theme gives it.
    fn sheet_ground(&self) -> Option<ColorF> {
        None
    }
    /// A source of the swatch's style that outlives the borrow of the page.
    fn swatch_style(&self) -> Rc<dyn Fn() -> SwatchStyle>;
    /// Whether the pane may show at all (the web page needs its product loaded).
    fn available(&self) -> bool {
        true
    }
    /// How many times the host has read its store again since the pane
    /// loaded. A read-only pane reads its document again when this changes.
    fn refreshes(&self) -> u64 {
        0
    }
}

const VIEW_KEY: u64 = 0x6773_6e74;

pub const SHEET: &str = r#"
.n2-woodshed-passage { display:flex; flex-direction:column; flex:1; min-height:0; overflow:auto; padding:14px; }
.blocks-workspace { display:flex; flex-direction:row; gap:10px; padding:14px; min-width:0; min-height:0; flex:1; overflow:hidden; }
.blocks-editor, .blocks-preview { display:flex; flex-direction:column; gap:10px; flex:1; min-width:0; min-height:0; overflow:auto; }
@media(max-width:900px) { .blocks-workspace { flex-direction:column; overflow:auto; } .blocks-editor, .blocks-preview { flex:none; overflow:visible; } }
.blocks-workspace h2, .blocks-workspace p { margin:0; }
.blocks-workspace label { display:flex; flex-direction:column; gap:5px; min-width:0; }
.blocks-workspace [role=textbox] { box-sizing:border-box; padding:8px; color:var(--tabard-color-text); background:var(--tabard-color-surface); border:1px solid var(--tabard-color-border); width:100%; }
.blocks-workspace .blocks-source { font-family:IBM Plex Mono,monospace; font-size:13px; line-height:1.5; white-space:pre-wrap; overflow-wrap:anywhere; max-height:220px; overflow:auto; min-height:120px; }
.blocks-actions { display:flex; flex-wrap:wrap; gap:6px; }
.blocks-toolbar { padding:8px 14px; border-bottom:1px solid var(--tabard-color-border); }
.blocks-toolbar button, .blocks-workspace button { text-align:left; padding:7px 10px; border:1px solid var(--tabard-color-border); color:var(--tabard-color-text); background:var(--tabard-color-surface); white-space:normal; overflow-wrap:anywhere; }
.blocks-workspace button[aria-pressed=true] { border-color:var(--tabard-color-accent); }
.blocks-toolbar button[aria-disabled=true], .blocks-workspace button[aria-disabled=true] { opacity:0.5; }
.blocks-guest-reading { display:flex; flex-direction:column; gap:10px; flex:none; }
@media(max-width:900px) { .blocks-guest-view .blocks-preview { order:-1; } }
.blocks-guest-event:focus-visible { outline:2px solid var(--tabard-color-primary); outline-offset:-2px; }
.blocks-row { display:block; width:100%; margin-block:3px; font-family:var(--workspace-font-reading,serif); }
.blocks-review { display:flex; flex-direction:column; gap:6px; }
.blocks-swatch { flex:none; max-width:100%; }
.blocks-workspace .backdrop-scene-reading { display:flex; flex-direction:column; gap:5px; }
"#;

pub const SWATCH_KEY: u64 = 0x626c_6f63;
const SOURCE_LABEL: &str = "Document source";
const QUERY_LABEL: &str = "Search block commands";
/// The disabled command offered for a block whose class no adapter claims.
const UNCLAIMED_COMMAND: &str = "block:unclaimed";
const WAITING: &str = "Waiting for the block adapter…";
const INITIAL: &str = "# Composition\n\n{#verse}\n::: metre\nThe path runs quiet through the night\nThe rain is gathering on the stone\nA window holds its little light\nAnd leaves the farthest field alone\n:::\n";

#[derive(Default)]
struct Display {
    input: Option<SceneInput>,
    surface: Option<BackdropSurface>,
    generation: u64,
}
type Saved = Result<(Vec<u8>, String), String>;
type Loaded = Result<Option<(StoredDocument, Vec<u8>)>, String>;

/// Work the pane retries when the adapter's answer arrives.
enum Pending {
    Preview(String),
    Apply,
    Run(String, String),
}

/// The latest view an adapter returned for the selected block.
struct ViewState {
    view: Result<GuestView, String>,
    generation: u64,
}

mod source_input;

pub struct Pane {
    document: Document,
    /// The source text widget. The pane owns it; the document's editor is told of
    /// its edits as messages ([`Self::push_input`]) and read back ([`Self::pull_input`]).
    source_input: TextInput,
    /// Makes the editor behind every document this pane opens.
    editors: Rc<dyn EditorFactory>,
    storage: Option<Rc<dyn Storage>>,
    ports: Rc<dyn PanePorts>,
    slot: String,
    expected: Option<Vec<u8>>,
    blocked: Option<String>,
    completed: Rc<RefCell<Option<Saved>>>,
    busy: bool,
    open: bool,
    woodshed_passage_proof: bool,
    selected: usize,
    selected_rows: Option<Vec<usize>>,
    /// Pattern and argument text fields, by key (`pattern:<class>`, `arg:<class>:<command>`).
    fields: BTreeMap<String, TextInput>,
    query: TextInput,
    registry: Registry,
    workers: Vec<WorkerAdapter>,
    preview: Option<Preview>,
    preview_command: String,
    preview_pattern: String,
    preview_rows: Vec<u32>,
    pending: Option<Pending>,
    display: Rc<RefCell<Display>>,
    canvas: bool,
    /// The selected view mode per class; none shows the swatch.
    view_modes: BTreeMap<String, String>,
    view: Option<ViewState>,
    view_leaf_signature: Option<String>,
    event_selected: (usize, usize),
    event_selected_revision: Option<String>,
    event_selected_block: usize,
    status: String,
    export_url: Option<String>,
    wake: Option<cambium_rootstock::HostWake>,
    refresh_key: Option<String>,
    /// Set by "Review block adapters"; the host that discloses adapters opens its review.
    review_requested: bool,
    /// The host refresh a read-only pane last read its document at, and that read's answer.
    refreshed: Option<u64>,
    reloaded: Rc<RefCell<Option<Loaded>>>,
}
impl Pane {
    /// A pane over `storage` (the backend and the document's slot id), if the host has one.
    pub async fn load(storage: Option<(Rc<dyn Storage>, String)>, ports: Rc<dyn PanePorts>) -> Self {
        Self::load_with(storage, ports, Rc::new(DefaultEditors)).await
    }
    /// [`Self::load`], editing through the editors `editors` makes.
    pub async fn load_with(
        storage: Option<(Rc<dyn Storage>, String)>,
        ports: Rc<dyn PanePorts>,
        editors: Rc<dyn EditorFactory>,
    ) -> Self {
        let source = storage;
        let document = Document::open(
            &format!("graphshell:document/{}", uuid::Uuid::new_v4()),
            INITIAL,
            editors.as_ref(),
        );
        let mut pane = Self {
            source_input: Pane::fresh_input(document.editor.as_ref()),
            document,
            editors,
            storage: source.as_ref().map(|(b, _)| b.clone()),
            ports,
            slot: source.map_or_else(
                || "document/unavailable".into(),
                |(_, id)| format!("graphshell/block-document/v1/{id}"),
            ),
            expected: None,
            blocked: None,
            completed: Default::default(),
            busy: false,
            open: false,
            woodshed_passage_proof: false,
            selected: 0,
            selected_rows: None,
            fields: BTreeMap::new(),
            query: TextInput::default(),
            registry: Registry::with_builtins(),
            workers: Vec::new(),
            preview: None,
            preview_command: String::new(),
            preview_pattern: String::new(),
            preview_rows: Vec::new(),
            pending: None,
            display: Default::default(),
            canvas: false,
            view_modes: BTreeMap::new(),
            view: None,
            view_leaf_signature: None,
            event_selected: (usize::MAX, usize::MAX),
            event_selected_revision: None,
            event_selected_block: usize::MAX,
            status: String::new(),
            export_url: None,
            wake: None,
            refresh_key: None,
            review_requested: false,
            refreshed: None,
            reloaded: Default::default(),
        };
        pane.install_defaults();
        if let Some(storage) = pane.storage.clone() {
            match storage.load(&pane.slot).await {
                Ok(Some((stored, bytes))) => {
                    pane.document = Document::open(&stored.id, &stored.source, pane.editors.as_ref());
                    pane.reset_input();
                    pane.expected = Some(bytes);
                    pane.status = "Saved document reopened".into();
                },
                Ok(None) if storage.read_only() => {
                    pane.status = "No saved document yet · viewing only".into()
                },
                Ok(None) => pane.status = "Example composition · Save to keep your edits".into(),
                Err(error) => {
                    pane.blocked = Some(error.clone());
                    pane.status =
                        format!("Document could not be restored: {error}. Stored bytes retained.");
                },
            }
        }
        pane
    }
    /// Whether this pane stores nothing: it shows the saved source only.
    pub fn read_only(&self) -> bool {
        self.storage.as_ref().is_some_and(|storage| storage.read_only())
    }
    /// Read the stored document again; [`register`] adopts the answer.
    fn reload(&mut self, refreshes: u64, wake: cambium_rootstock::HostWake) {
        self.refreshed = Some(refreshes);
        let Some(storage) = self.storage.clone() else {
            return;
        };
        let (slot, done) = (self.slot.clone(), self.reloaded.clone());
        self.ports.spawn(Box::pin(async move {
            *done.borrow_mut() = Some(storage.load(&slot).await);
            wake.wake();
        }));
    }
    fn adopt_reload(&mut self, loaded: Loaded) {
        match loaded {
            Ok(Some((stored, bytes))) => {
                if stored.source != self.source() || stored.id != self.document.id
                {
                    self.document = Document::open(&stored.id, &stored.source, self.editors.as_ref());
                    self.reset_input();
                    self.selected = self
                        .selected
                        .min(self.document.blocks().len().saturating_sub(1));
                    self.selected_rows = None;
                    self.preview = None;
                    self.refresh_key = None;
                }
                self.expected = Some(bytes);
                self.blocked = None;
                self.status = "Saved document reopened".into();
            },
            Ok(None) => {},
            Err(error) => self.status = format!("Document could not be read again: {error}"),
        }
    }
    /// Give every registered adapter's pattern and argument fields their default text.
    fn install_defaults(&mut self) {
        let claims: Vec<_> = self
            .registry
            .classes()
            .into_iter()
            .filter_map(|c| self.registry.get(c).map(|a| a.claim().clone()))
            .collect();
        for claim in claims {
            if let Some(field) = &claim.pattern {
                self.fields
                    .entry(format!("pattern:{}", claim.class))
                    .or_insert_with(|| TextInput::new(&field.default));
            }
            for command in &claim.commands {
                if let Some(field) = &command.arg {
                    self.fields
                        .entry(format!("arg:{}:{}", claim.class, command.id))
                        .or_insert_with(|| TextInput::new(&field.default));
                }
            }
        }
    }
    /// Register an adapter (a guest answering in process, or through a Worker). Two claims for
    /// one class are refused.
    pub fn add_adapter(&mut self, adapter: Box<dyn BlockAdapter>) -> Result<(), String> {
        self.registry.register(adapter)?;
        self.install_defaults();
        Ok(())
    }
    /// Register a guest's adapter, answered by `worker`.
    pub fn add_worker(&mut self, worker: WorkerAdapter) -> Result<(), String> {
        self.registry.register(Box::new(worker.clone()))?;
        self.workers.push(worker);
        self.install_defaults();
        Ok(())
    }
    /// A guest's adapter was revoked or stopped: blocks of its class return to plain text.
    pub fn remove_adapter(&mut self, class: &str) {
        self.registry.remove(class);
        self.workers.retain(|w| w.claim().class != class);
        self.view_modes.remove(class);
        self.view = None;
        self.preview = None;
        self.refresh_key = None;
    }
    /// Deliver a Worker's reply for `class`; true means the pane should wake.
    pub fn deliver(&self, class: &str, id: u64, reply: &str) -> bool {
        self.workers
            .iter()
            .find(|w| w.claim().class == class)
            .is_some_and(|w| w.deliver(id, reply))
    }
    pub fn open(&self) -> bool {
        self.open
    }
    /// Whether an adapter has been asked something it has not answered yet: work waiting to be
    /// retried, or a chosen reading with no view yet. A scenario's waits settle on this.
    pub fn awaiting_adapter(&self) -> bool {
        self.pending.is_some()
            || (self.open
                && self.current_class().is_some_and(|c| self.view_modes.contains_key(&c))
                && self.view.is_none())
    }
    pub fn set_open(&mut self, open: bool) {
        if !self.woodshed_passage_proof {
            self.open = open;
        }
    }
    /// Whether the user asked to review block adapters; reading it clears it.
    pub fn take_review_request(&mut self) -> bool {
        std::mem::take(&mut self.review_requested)
    }
    pub fn review_requested(&self) -> bool {
        self.review_requested
    }
    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn classes(&self) -> Vec<&str> {
        self.registry.classes()
    }
    fn field(&self, key: &str) -> &str {
        self.fields.get(key).map_or("", TextInput::text)
    }
    fn field_mut(&mut self, key: &str) -> &mut TextInput {
        self.fields.entry(key.to_owned()).or_default()
    }
    fn target(&self) -> Result<crate::block_editing::Div, String> {
        self.document
            .blocks()
            .get(self.selected)
            .cloned()
            .ok_or("Select a current document block".into())
    }
    /// The first class of `div` that an adapter claims.
    fn class_of(&self, div: &crate::block_editing::Div) -> Option<String> {
        classes(div)
            .into_iter()
            .find(|c| self.registry.get(c).is_some())
    }
    /// A class on `div` that no adapter claims, if the block names one.
    fn unclaimed_class(&self, div: &crate::block_editing::Div) -> Option<String> {
        if self.class_of(div).is_some() {
            None
        } else {
            classes(div).into_iter().next()
        }
    }
    fn current_class(&self) -> Option<String> {
        self.target().ok().and_then(|d| self.class_of(&d))
    }
    fn adapter(&self, class: &str) -> Option<&dyn BlockAdapter> {
        self.registry.get(class)
    }
    fn input(
        &self,
        class: &str,
        div: &crate::block_editing::Div,
        source: &str,
        selected: Vec<u32>,
    ) -> BlockInput {
        BlockInput {
            class: class.to_owned(),
            body: source[div.body.clone()].to_owned(),
            attrs: div.attributes.clone(),
            revision: source_revision(source),
            selected,
            pattern: self.field(&format!("pattern:{class}")).to_owned(),
        }
    }
    fn answered<T>(&mut self, answer: Result<T, AdapterError>, retry: Pending) -> Result<T, String> {
        match answer {
            Ok(value) => Ok(value),
            Err(AdapterError::Pending) => {
                self.pending = Some(retry);
                Err(WAITING.into())
            },
            Err(AdapterError::Refused(text)) => Err(text),
        }
    }
    /// The block's rows, or why there are none.
    fn rows(&self, class: &str, div: &crate::block_editing::Div) -> Result<Vec<Row>, String> {
        let adapter = self.adapter(class).ok_or_else(|| unclaimed_message(class))?;
        let source = self.source();
        match adapter.rows(&self.input(class, div, source, Vec::new())) {
            Ok(rows) => Ok(rows),
            Err(AdapterError::Pending) => Err(WAITING.into()),
            Err(AdapterError::Refused(text)) => Err(text),
        }
    }
    /// The rows the user chose; every row until the user chooses.
    fn selection(&self, class: &str, div: &crate::block_editing::Div) -> Result<Vec<u32>, String> {
        match &self.selected_rows {
            Some(rows) => Ok(rows.iter().map(|&r| r as u32).collect()),
            None => Ok(self.rows(class, div)?.into_iter().map(|r| r.index).collect()),
        }
    }
    fn commands(&self) -> CommandSet {
        let mut set = CommandSet::new();
        let Ok(div) = self.target() else {
            return set;
        };
        if let Some(class) = self.unclaimed_class(&div) {
            let id = UNCLAIMED_COMMAND;
            set.register(Command::new(
                id,
                format!("Edit {class} block…"),
                "block",
                cambium::EffectClass::DomainTruth,
            ));
            set.set_disabled(id, Some(unclaimed_message(&class)));
            return set;
        }
        let Some(class) = self.class_of(&div) else {
            return set;
        };
        let Some(adapter) = self.adapter(&class) else {
            return set;
        };
        for command in adapter
            .claim()
            .commands
            .iter()
            .filter(|c| c.kind == CommandKind::Choice)
        {
            set.register(Command::new(
                command.id.clone(),
                command.label.clone(),
                "block",
                cambium::EffectClass::DomainTruth,
            ));
            let reason = self.proposals(&command.id).map(|_| ()).err();
            set.set_disabled(&command.id, reason);
        }
        set
    }
    /// Why each disabled block command is disabled, as the pane says it. The unclaimed command's
    /// reason is left out: the unclaimed note beside it already says so.
    fn command_reasons(&self) -> Vec<String> {
        self.commands()
            .menu(&CommandChoices::default(), Some("block"), self.query.text())
            .into_iter()
            .filter(|command| command.id != UNCLAIMED_COMMAND)
            .filter_map(|command| command.disabled_reason.clone())
            .collect()
    }
    /// How many times the pane tells the person the selected block's class is unclaimed.
    fn unclaimed_notices(&self) -> usize {
        let Some(class) = self.target().ok().and_then(|d| self.unclaimed_class(&d)) else {
            return 0;
        };
        let message = unclaimed_message(&class);
        1 + self.command_reasons().iter().filter(|r| **r == message).count()
    }
    fn proposals(&self, command: &str) -> Result<Vec<Change>, String> {
        let div = self.target()?;
        let class = self
            .class_of(&div)
            .ok_or_else(|| "This block has no registered editing adapter".to_string())?;
        if !div.children.is_empty() {
            return Err("Select a leaf block; nested block contents need their own adapter".into());
        }
        let selected = self.selection(&class, &div)?;
        let source = self.source();
        let input = self.input(&class, &div, source, selected);
        match self.adapter(&class).ok_or("No adapter")?.choices(&input, command) {
            Ok(changes) => Ok(changes),
            Err(AdapterError::Pending) => Err(WAITING.into()),
            Err(AdapterError::Refused(text)) => Err(text),
        }
    }
    fn preview(&mut self, command: &str) -> Result<(), String> {
        let div = self.target()?;
        let class = self
            .class_of(&div)
            .ok_or_else(|| "This block has no registered editing adapter".to_string())?;
        let rows = self.selection(&class, &div)?;
        let answer = self.proposals(command);
        let changes = match answer {
            Err(text) if text == WAITING => {
                self.pending = Some(Pending::Preview(command.to_owned()));
                return Err(text);
            },
            other => other?,
        };
        self.preview = Some(Preview::new(self.source(), div, changes)?);
        self.preview_command = command.to_owned();
        self.preview_pattern = self.field(&format!("pattern:{class}")).to_owned();
        self.preview_rows = rows;
        self.status = "Preview only · review changes, then Apply".into();
        Ok(())
    }
    fn preview_class(&self) -> Option<String> {
        self.preview.as_ref().and_then(|p| self.class_of(&p.block))
    }
    fn apply(&mut self) -> Result<(), String> {
        let class = self.preview_class().ok_or("Preview a change first")?;
        if self.field(&format!("pattern:{class}")) != self.preview_pattern {
            return Err("Pattern changed; preview it again before applying".into());
        }
        let preview = self.preview.clone().ok_or("Preview a change first")?;
        let input = self.input(&class, &preview.block, &preview.source, self.preview_rows.clone());
        let answer = self
            .adapter(&class)
            .ok_or("No adapter")?
            .apply(&input, &self.preview_command, &preview.changes);
        let attrs = self.answered(answer, Pending::Apply)?;
        let changed = attrs != preview.block.attributes;
        self.edit(|d| d.apply(&preview, attrs))?;
        self.preview = None;
        self.status = if changed {
            "Change applied · Undo and Save use this document’s history"
        } else {
            "No changes accepted · document unchanged"
        }
        .into();
        Ok(())
    }
    /// An explicit write or text export of the selected block's adapter.
    fn run_command(&mut self, command: &str, arg: &str) -> Result<(), String> {
        if self.preview.is_some() {
            return Err("Apply or cancel the current preview first".into());
        }
        let div = self.target()?;
        let class = self
            .class_of(&div)
            .ok_or_else(|| "This block has no registered editing adapter".to_string())?;
        if !div.children.is_empty() {
            return Err("Select a leaf block".into());
        }
        let selected = self.selection(&class, &div).unwrap_or_default();
        let source = self.source().to_owned();
        let input = self.input(&class, &div, &source, selected);
        let answer = self
            .adapter(&class)
            .ok_or("No adapter")?
            .run(&input, command, arg);
        match self.answered(answer, Pending::Run(command.to_owned(), arg.to_owned()))? {
            RunResult::Attrs(attrs) => {
                let preview = Preview::new(&source, div, Vec::new())?;
                self.edit(|d| d.apply(&preview, attrs))?;
                // A write that gives the block something to show shows the adapter's first view.
                if let Some(first) = self.adapter(&class).and_then(|a| a.claim().views.first().cloned()) {
                    self.view_modes.insert(class.clone(), first.id);
                }
                self.view = None;
                self.event_selected = (usize::MAX, usize::MAX);
                self.status = if self.woodshed_passage_proof {
                    format!("{command} written · Kept for this session")
                } else {
                    format!("{command} written · Save document to keep it")
                };
            },
            RunResult::Text(text) => {
                let url = self.ports.export(
                    text.as_bytes(),
                    "text/plain",
                    &format!("{class}-{command}.txt"),
                )?;
                if let Some(old) = self.export_url.replace(url) {
                    self.ports.release_export(&old);
                }
                self.status = format!("{command} exported");
            },
        }
        Ok(())
    }
    /// Try the work an adapter's late answer was needed for.
    fn retry_pending(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let result = match &pending {
            Pending::Preview(command) => self.preview(command),
            Pending::Apply => self.apply(),
            Pending::Run(command, arg) => self.run_command(command, arg),
        };
        match result {
            Ok(()) => {},
            Err(text) if text == WAITING => {},
            Err(text) => self.status = text,
        }
    }
    /// Insert the example the class's adapter offers, at the end of the document.
    fn insert_example(&mut self, class: &str) -> Result<(), String> {
        let example = self
            .adapter(class)
            .and_then(|a| a.claim().example.clone())
            .ok_or("This adapter offers no example")?;
        let mut attrs = example.attrs.clone();
        attrs.insert("class".into(), class.to_owned());
        attrs.insert("id".into(), format!("{class}-{}", uuid::Uuid::new_v4()));
        let block = crate::block_editing::wrap(&attrs, &example.body);
        let end = self.source().len();
        if end + block.len() + 2 > crate::block_editing::MAX_DOCUMENT_BYTES {
            return Err("Document exceeds one MiB".into());
        }
        let position = CaretPosition {
            byte: end,
            affinity: CaretAffinity::Downstream,
        };
        self.document
            .editor
            .apply(TextCommand::SetSelection(CaretSelection {
                anchor: position,
                focus: position,
            }));
        self.document
            .editor
            .apply(TextCommand::Insert(format!("\n\n{block}")));
        self.selected = self.document.blocks().len() - 1;
        self.selected_rows = None;
        self.preview = None;
        self.status = format!("Inserted the {class} example");
        Ok(())
    }
    fn event_selection(&self) -> (usize, usize) {
        if self.event_selected_block == self.selected
            && self.event_selected_revision.as_ref()
                == Some(&source_revision(self.source()))
        {
            self.event_selected
        } else {
            (usize::MAX, usize::MAX)
        }
    }
    /// Ask the selected block's adapter for the view in the chosen mode. Returns true when a
    /// new result replaced the old one. A pending answer keeps the previous view as stale.
    fn refresh_view(&mut self, width: f32) -> bool {
        let Ok(div) = self.target() else {
            return self.view.take().is_some();
        };
        let Some(class) = self.class_of(&div) else {
            return self.view.take().is_some();
        };
        let Some(mode) = self.view_modes.get(&class).cloned() else {
            return self.view.take().is_some();
        };
        let selected = self.selection(&class, &div).unwrap_or_default();
        let source = self.source();
        let input = self.input(&class, &div, source, selected);
        let (bar, event) = self.event_selection();
        let ask = ViewAsk {
            mode,
            width,
            selected: (bar != usize::MAX).then_some((bar as u32, event as u32)),
        };
        let answer = self.adapter(&class).map(|a| a.view(&input, &ask));
        let fresh = match answer {
            None | Some(Ok(None)) => return self.view.take().is_some(),
            Some(Err(AdapterError::Pending)) => return false,
            Some(Ok(Some(view))) => Ok(view),
            Some(Err(AdapterError::Refused(text))) => Err(text),
        };
        let same = self
            .view
            .as_ref()
            .is_some_and(|v| match (&v.view, &fresh) {
                (Ok(a), Ok(b)) => a == b,
                (Err(a), Err(b)) => a == b,
                _ => false,
            });
        if same {
            return false;
        }
        let generation = self.view.as_ref().map_or(1, |v| v.generation + 1);
        self.view = Some(ViewState {
            view: fresh,
            generation,
        });
        true
    }
    fn save(&mut self) -> Result<(), String> {
        if self.read_only() {
            return Err(VIEW_ONLY.into());
        }
        if self.busy {
            return Err("Document save is pending".into());
        }
        if let Some(error) = &self.blocked {
            return Err(error.clone());
        }
        let storage = self
            .storage
            .clone()
            .ok_or("Document storage is unavailable")?;
        let document = StoredDocument {
            version: 1,
            id: self.document.id.clone(),
            source: self.source().into(),
        };
        document.validate()?;
        let expected = self.expected.clone();
        let slot = self.slot.clone();
        let completion = self.completed.clone();
        let wake = self.wake.clone();
        self.busy = true;
        self.status = "Saving document…".into();
        self.ports.spawn(Box::pin(async move {
            let source = document.source.clone();
            *completion.borrow_mut() = Some(
                storage
                    .save(&slot, expected, document)
                    .await
                    .map(|bytes| (bytes, source)),
            );
            if let Some(wake) = wake {
                wake.wake();
            }
        }));
        Ok(())
    }
    /// Rebuild the swatch for the selected block. Returns true when an adapter has not answered
    /// yet, so the caller asks again next frame.
    fn refresh(&mut self) -> bool {
        let result: Result<Option<SceneInput>, String> = (|| {
            let div = self.target()?;
            let source = self.source();
            let mut attrs = div.attributes.clone();
            let class = self
                .class_of(&div)
                .ok_or_else(|| "No scene adapter for this block. Its source stays editable.".to_string())?;
            if let Some(preview) = &self.preview {
                if preview.source != source || preview.block != div {
                    return Err("Document changed since this preview. Cancel or preview again before applying.".into());
                }
                let input = self.input(&class, &preview.block, &preview.source, self.preview_rows.clone());
                match self
                    .adapter(&class)
                    .ok_or("No adapter")?
                    .apply(&input, &self.preview_command, &preview.changes)
                {
                    Ok(next) => attrs = next,
                    Err(AdapterError::Pending) => return Ok(None),
                    Err(AdapterError::Refused(text)) => return Err(text),
                }
            }
            if !div.children.is_empty() {
                return Err(
                    "Select a leaf block; nested block contents need their own adapter".into(),
                );
            }
            let adapter = self.adapter(&class).ok_or("No adapter")?;
            if !adapter.claim().commands.iter().any(|c| c.id == BACKDROP_SCENE) {
                return Err("No scene adapter for this block. Its source stays editable.".into());
            }
            let mut input = self.input(&class, &div, source, Vec::new());
            input.attrs = attrs;
            let address = format!("{}#{}", self.document.id, self.selected);
            match adapter.run(&input, BACKDROP_SCENE, &address) {
                Ok(RunResult::Text(json)) => SceneInput::parse(&json)
                    .map(Some)
                    .map_err(|e| e.to_string()),
                Ok(RunResult::Attrs(_)) => Err("The scene command returned attributes".into()),
                Err(AdapterError::Pending) => Ok(None),
                Err(AdapterError::Refused(text)) => Err(text),
            }
        })();
        let pending = matches!(result, Ok(None));
        let mut display = self.display.borrow_mut();
        match result {
            Ok(Some(input)) if display.input.as_ref() != Some(&input) => {
                let accepted = if let Some(surface) = &mut display.surface {
                    surface.replace(&input).map_err(|e| e.to_string())
                } else {
                    BackdropSurface::new(&input)
                        .map(|mut surface| {
                            surface.register_font(
                                include_bytes!("../web/IBMPlexSans-Regular.ttf").to_vec(),
                            );
                            display.surface = Some(surface);
                        })
                        .map_err(|e| e.to_string())
                };
                match accepted {
                    Ok(()) => {
                        display.input = Some(input);
                        display.generation = display.generation.wrapping_add(1);
                    },
                    Err(error) => self.status = error,
                }
            },
            Ok(_) => {},
            Err(error) => {
                display.surface = None;
                display.input = None;
                self.status = error;
            },
        }
        pending
    }
}
/// The class tokens of a block's `class` attribute, in order.
fn classes(div: &crate::block_editing::Div) -> Vec<String> {
    div.attributes
        .get("class")
        .map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}
fn source_revision(source: &str) -> String {
    format!(
        "{:016x}",
        source
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b))
                .wrapping_mul(0x100_0000_01b3))
    )
}
fn action<P: PaneHost>(page: &mut P, change: impl FnOnce(&mut Pane) -> Result<(), String>) {
    if let Err(error) = change(page.pane_mut()) {
        if error != WAITING {
            page.pane_mut().status = error;
        } else {
            page.pane_mut().status = WAITING.into();
        }
    }
}
pub fn control<P: PaneHost>(page: &P) -> Child<P> {
    Box::new(
        button(
            if page.pane().open {
                "Close document"
            } else {
                "Editable document"
            },
            |p: &mut P, _| {
                p.pane_mut().open = !p.pane_mut().open;
            },
        )
        .attr("class", "blocks-open"),
    )
}
pub fn active<P: PaneHost>(page: &P) -> bool {
    page.available() && page.pane().open
}
pub fn toolbar<P: PaneHost>(page: &P) -> Option<Child<P>> {
    if !active(page) {
        return None;
    }
    let pane = page.pane();
    let read_only = pane.read_only();
    let set = pane.commands();
    let commands = set.menu(&CommandChoices::default(), Some("block"), pane.query.text());
    let export = button(
        "Export document",
        |p: &mut P, _| {
            let pane = p.pane_mut();
            match pane.ports.export(
                pane.source().as_bytes(),
                "text/djot",
                "composition.djot",
            ) {
                Ok(url) => {
                    if let Some(old) = pane.export_url.replace(url) {
                        pane.ports.release_export(&old);
                    }
                },
                Err(error) => pane.status = error,
            }
        },
    );
    let mut children: Vec<Child<P>> = vec![Box::new(
        button(
            if pane.woodshed_passage_proof {
                "Return to passage"
            } else {
                "Return to graph"
            },
            |p: &mut P, _| p.pane_mut().open = false,
        )
        .attr("class", "blocks-close"),
    )];
    if read_only {
        // A read-only pane stores nothing, so it offers no save, edit or command.
        children.push(Box::new(el("div", export).attr("class", "blocks-actions")));
        children.push(Box::new(el("p", VIEW_ONLY)));
    } else {
        children.push(Box::new(
            el(
                "div",
                (
                    button("Save document", |p: &mut P, _| action(p, Pane::save))
                        .attr(
                            "aria-disabled",
                            (pane.busy || pane.blocked.is_some() || pane.woodshed_passage_proof)
                                .to_string(),
                        )
                        .attr("class", "blocks-save"),
                    button("Undo document edit", |p: &mut P, _| {
                        p.pane_mut().edit(|d| d.editor.apply(TextCommand::Undo));
                        p.pane_mut().preview = None;
                    })
                    .attr("class", "blocks-undo"),
                    button("Redo document edit", |p: &mut P, _| {
                        p.pane_mut().edit(|d| d.editor.apply(TextCommand::Redo));
                        p.pane_mut().preview = None;
                    })
                    .attr("class", "blocks-redo"),
                    export,
                ),
            )
            .attr("class", "blocks-actions"),
        ));
    }
    for claim in pane.registry.examples().into_iter().filter(|_| !read_only) {
        let class = claim.class.clone();
        let aria_class = class.clone();
        let title = claim.example.as_ref().map(|e| e.title.trim().to_owned()).unwrap_or_default();
        children.push(Box::new(
            button(format!("Insert {class} example"), move |p: &mut P, _| {
                let class = class.clone();
                action(p, move |pane| pane.insert_example(&class))
            })
            .attr("aria-label", format!("Insert {aria_class} example · {title}"))
            .attr("class", "blocks-insert-example"),
        ));
    }
    for command in commands.into_iter().filter(|_| !read_only) {
        let id = command.id.to_string();
        let disabled = command.disabled_reason.is_some();
        children.push(Box::new(
            button(command.label.clone(), move |p: &mut P, _| {
                let id = id.clone();
                action(p, move |pane| pane.preview(&id))
            })
            .attr("aria-disabled", disabled.to_string())
            .attr("class", "blocks-preview-command"),
        ));
    }
    Some(Box::new(
        el("div", children)
            .attr("class", "blocks-toolbar blocks-actions")
            .attr("aria-label", "Document commands"),
    ))
}
pub fn busy<P: PaneHost>(page: &P) -> bool {
    page.pane().busy || page.pane().awaiting_adapter()
}
pub fn workspace<P: PaneHost>(page: &P) -> Option<Child<P>> {
    if !page.available() || !page.pane().open {
        return None;
    }
    let pane = page.pane();
    let read_only = pane.read_only();
    let mut children: Vec<Child<P>> = vec![Box::new(el("h2", "Composition"))];
    if !read_only {
        children.push(Box::new(button(
            "Create verse block from selected lines",
            |p: &mut P, _| {
                action(p, |pane| {
                    let id = format!("verse-{}", uuid::Uuid::new_v4());
                    pane.edit(|d| {
                        d.wrap_selection(std::collections::BTreeMap::from([
                            ("class".into(), "metre".into()),
                            ("id".into(), id.clone()),
                        ]))
                    })?;
                    pane.selected = pane
                        .document
                        .blocks()
                        .iter()
                        .position(|d| d.attributes.get("id") == Some(&id))
                        .ok_or("Created block could not be resolved")?;
                    pane.selected_rows = None;
                    pane.preview = None;
                    pane.status =
                        "Selected lines are now a verse block · no poetic form imposed".into();
                    Ok(())
                })
            },
        )));
    }
    children.extend::<Vec<Child<P>>>(vec![
        Box::new(el(
            "p",
            format!(
                "{} · {}",
                "Composition",
                if pane.dirty() {
                    "Unsaved changes"
                } else if pane.expected.is_none() && read_only {
                    "No saved document yet"
                } else if pane.expected.is_none() {
                    "Example composition · not saved"
                } else {
                    "Saved source"
                }
            ),
        )),
        Box::new(status_note("blocks-status", pane.status.clone())),
        // The label stays visible text: an aria-label would replace the source for readers.
        if read_only {
            Box::new(el(
                "div",
                (
                    el("p", SOURCE_LABEL),
                    el("pre", pane.source().to_string())
                        .attr("class", "blocks-source"),
                ),
            ))
        } else {
            Box::new(el(
                "label",
                (
                    SOURCE_LABEL,
                    lens(
                        |input: &mut TextInput| {
                            textarea_typed(input)
                                .attr("aria-label", SOURCE_LABEL)
                                .attr("class", "blocks-source")
                        },
                        |page: &mut P| &mut page.pane_mut().source_input,
                    ),
                ),
            ))
        },
    ]);
    let blocks = pane.document.blocks();
    let mut block_buttons: Vec<Child<P>> = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let label = match classes(block).first() {
            Some(class) if class != "metre" => format!("{class} block {}", index + 1),
            _ => block
                .attributes
                .get("id")
                .cloned()
                .unwrap_or_else(|| format!("Block {}", index + 1)),
        };
        block_buttons.push(Box::new(
            button(label, move |p: &mut P, _| {
                p.pane_mut().selected = index;
                p.pane_mut().selected_rows = None;
                p.pane_mut().preview = None;
            })
            .attr("aria-pressed", (pane.selected == index).to_string())
            .attr("class", format!("blocks-select-{index}")),
        ));
    }
    children.push(Box::new(
        el("div", block_buttons).attr("class", "blocks-actions"),
    ));
    let mut guest_class: Option<String> = None;
    if let Ok(div) = pane.target() {
        if let Some(class) = pane.class_of(&div) {
            guest_class = Some(class.clone());
            let selected = pane.selection(&class, &div).unwrap_or_default();
            let mut rows: Vec<Child<P>> = Vec::new();
            for row in pane.rows(&class, &div).unwrap_or_default() {
                let index = row.index;
                let class = class.clone();
                rows.push(Box::new(
                    button(row.label, move |p: &mut P, _| {
                        if let Ok(div) = p.pane_mut().target() {
                            let mut selected = p.pane_mut().selection(&class, &div).unwrap_or_default();
                            if selected.contains(&index) {
                                selected.retain(|i| *i != index);
                            } else {
                                selected.push(index);
                                selected.sort();
                            }
                            p.pane_mut().selected_rows =
                                Some(selected.into_iter().map(|i| i as usize).collect());
                            p.pane_mut().preview = None;
                        }
                    })
                    .attr("aria-pressed", selected.contains(&index).to_string())
                    .attr("class", format!("blocks-row blocks-row-{index}")),
                ));
            }
            children.push(Box::new(
                el("div", rows).attr("aria-label", "Select rows"),
            ));
            let claim = pane.adapter(&class).map(|a| a.claim().clone());
            if let Some(claim) = &claim {
                if let Some(field) = claim.pattern.as_ref().filter(|_| !read_only) {
                    let key = format!("pattern:{class}");
                    let label = field.label.clone();
                    let aria = label.clone();
                    children.push(Box::new(el(
                        "label",
                        (
                            label,
                            lens(
                                move |input: &mut TextInput| {
                                    text_field_typed(input).attr("aria-label", aria.clone())
                                },
                                move |p: &mut P| p.pane_mut().field_mut(&key),
                            ),
                        ),
                    )));
                }
            }
        } else if let Some(class) = pane.unclaimed_class(&div) {
            children.push(Box::new(
                status_note("blocks-unclaimed", unclaimed_message(&class)),
            ));
            children.push(Box::new(
                button("Review block adapters", |p: &mut P, _| {
                    p.pane_mut().review_requested = true;
                    p.pane_mut().status = "Review requested · the host that discloses block adapters opens it".into();
                })
                .attr("class", "blocks-review-adapters"),
            ));
        }
    } else {
        children.push(Box::new(el("p", "Use an attributed Djot block to offer domain commands. Unrecognized content remains editable.")));
    }
    // A following view runs no block commands, so it offers no search for them.
    if !read_only {
        children.push(Box::new(el(
            "label",
            (
                QUERY_LABEL,
                lens(
                    |input: &mut TextInput| text_field_typed(input).attr("aria-label", QUERY_LABEL),
                    |p: &mut P| &mut p.pane_mut().query,
                ),
            ),
        )));
        for reason in pane.command_reasons() {
            children.push(Box::new(el("p", reason)));
        }
    }
    if let Some(class) = &guest_class {
        if let Some(claim) = pane.adapter(class).map(|a| a.claim().clone()) {
            for command in claim
                .commands
                .iter()
                .filter(|c| c.kind != CommandKind::Choice && c.id != BACKDROP_SCENE)
                .filter(|_| !read_only)
            {
                let arg_key = format!("arg:{class}:{}", command.id);
                if let Some(field) = &command.arg {
                    let label = field.label.clone();
                    let aria = label.clone();
                    let key = arg_key.clone();
                    children.push(Box::new(el(
                        "label",
                        (
                            label,
                            lens(
                                move |input: &mut TextInput| {
                                    text_field_typed(input).attr("aria-label", aria.clone())
                                },
                                move |p: &mut P| p.pane_mut().field_mut(&key),
                            ),
                        ),
                    )));
                }
                let id = command.id.clone();
                children.push(Box::new(
                    button(command.label.clone(), move |p: &mut P, _| {
                        let id = id.clone();
                        let key = arg_key.clone();
                        action(p, move |pane| {
                            let arg = pane.field(&key).to_owned();
                            pane.run_command(&id, &arg)
                        })
                    })
                    .attr("class", format!("blocks-command-{}", command.id)),
                ));
            }
        }
    }
    let mut preview_rows: Vec<Child<P>> = Vec::new();
    if let Some(preview) = pane.preview.as_ref().filter(|_| !read_only) {
        let class = pane.preview_class().unwrap_or_default();
        let stale = preview.source != pane.source()
            || pane.preview_pattern != pane.field(&format!("pattern:{class}"));
        for (index, change) in preview.changes.iter().enumerate() {
            let label = format!(
                "{} · {} → {} · {}",
                change.label,
                change.before.as_deref().unwrap_or("Not set"),
                change.after,
                if change.use_update {
                    "Use update"
                } else {
                    "Keep local"
                }
            );
            preview_rows.push(Box::new(
                button(label, move |p: &mut P, _| {
                    if let Some(change) = p
                        .pane_mut()
                        .preview
                        .as_mut()
                        .and_then(|v| v.changes.get_mut(index))
                    {
                        change.use_update = !change.use_update;
                    }
                })
                .attr("aria-pressed", change.use_update.to_string())
                .attr("class", format!("blocks-change-{index}")),
            ));
        }
        preview_rows.push(Box::new(
            el(
                "div",
                (
                    button("Apply to document", |p: &mut P, _| {
                        action(p, Pane::apply)
                    })
                    .attr("aria-disabled", stale.to_string())
                    .attr("class", "blocks-apply"),
                    button("Cancel preview", |p: &mut P, _| {
                        p.pane_mut().preview = None;
                        p.pane_mut().status = "Preview cancelled · source unchanged".into();
                    })
                    .attr("class", "blocks-cancel"),
                ),
            )
            .attr("class", "blocks-actions"),
        ));
    }
    children.push(Box::new(
        el("div", preview_rows).attr("class", "blocks-review"),
    ));
    let mut surface_children: Vec<Child<P>> = Vec::new();
    if let Some(class) = &guest_class {
        surface_children.push(view_panel(page, class));
    }
    let showing_view = guest_class
        .as_ref()
        .is_some_and(|c| pane.view_modes.contains_key(c));
    let display = pane.display.borrow();
    if let Some(surface) = &display.surface
        && !showing_view
    {
        let available = if page.size().0 > 900 {
            page.size().0.saturating_sub(64) / 2
        } else {
            page.size().0.saturating_sub(44)
        }
        .max(1);
        let width = available.min(if pane.canvas { 1000 } else { 460 });
        let height = if pane.canvas { 520 } else { 300 };
        surface_children.push(Box::new(
            button(
                if pane.canvas {
                    "Show Swatch"
                } else {
                    "Expand to Canvas"
                },
                |p: &mut P, _| p.pane_mut().canvas = !p.pane_mut().canvas,
            )
            .attr("class", "blocks-expand"),
        ));
        surface_children.push(Box::new(
            custom_leaf::<P, ()>(SWATCH_KEY, width, height)
                .attr(
                    "aria-label",
                    if pane.preview.is_some() {
                        "Swatch preview of proposed document change"
                    } else {
                        "Document Swatch"
                    },
                )
                .attr("class", "blocks-swatch"),
        ));
        surface_children.push(Box::new(cambium::backdrop_scene_reading(
            surface.scene(),
            surface.selected(),
            surface.retained(),
            |p: &mut P, event| {
                match &event {
                    cambium::BackdropSceneEvent::Select(key) => {
                        if let Some(index) = key.strip_prefix("line-").and_then(|s| s.parse().ok())
                        {
                            p.pane_mut().selected_rows = Some(vec![index]);
                            p.pane_mut().preview = None;
                        }
                    },
                    cambium::BackdropSceneEvent::Clear => {
                        p.pane_mut().selected_rows = Some(Vec::new());
                        p.pane_mut().preview = None;
                    },
                    _ => {},
                }
                let mut display = p.pane_mut().display.borrow_mut();
                if let Some(surface) = &mut display.surface {
                    let _ = match event {
                        cambium::BackdropSceneEvent::Select(key) => surface.select(Some(&key)),
                        cambium::BackdropSceneEvent::Clear => surface.select(None),
                        cambium::BackdropSceneEvent::DismissRetained => {
                            surface.dismiss_retained();
                            Ok(())
                        },
                    };
                }
                display.generation = display.generation.wrapping_add(1);
            },
        )));
    }
    Some(Box::new(
        el(
            "section",
            (
                el("div", children).attr("class", "blocks-editor"),
                el("div", surface_children).attr("class", "blocks-preview"),
            ),
        )
        .attr(
            "class",
            if showing_view {
                "blocks-workspace blocks-guest-view"
            } else {
                "blocks-workspace"
            },
        )
        .attr("aria-label", "Editable document workspace"),
    ))
}

fn view_width<P: PaneHost>(page: &P) -> f32 {
    if page.size().0 > 900 {
        page.size().0.saturating_sub(64) as f32 / 2.0
    } else {
        page.size().0.saturating_sub(44) as f32
    }
}

/// The view modes an adapter offers, and the selected one's drawing with its event regions.
fn view_panel<P: PaneHost>(page: &P, class: &str) -> Child<P> {
    let pane = page.pane();
    let Some(claim) = pane.adapter(class).map(|a| a.claim().clone()) else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child<P>> = Vec::new();
    if claim.views.is_empty() {
        return Box::new(el("div", ()));
    }
    let selected_mode = pane.view_modes.get(class).cloned();
    let mut buttons: Vec<Child<P>> = Vec::new();
    for mode in &claim.views {
        let id = mode.id.clone();
        let class_owned = class.to_owned();
        let is_selected = selected_mode.as_deref() == Some(mode.id.as_str());
        buttons.push(Box::new(
            button(mode.label.clone(), move |p: &mut P, _| {
                if is_selected {
                    p.pane_mut().view_modes.remove(&class_owned);
                } else {
                    p.pane_mut().view_modes.insert(class_owned.clone(), id.clone());
                }
                // The selected event carries across the readings.
                p.pane_mut().view = None;
            })
            .attr("aria-pressed", is_selected.to_string())
            .attr("class", format!("blocks-view-{}", mode.id)),
        ));
    }
    children.push(Box::new(el("div", buttons).attr("class", "blocks-actions")));
    match (&selected_mode, &pane.view) {
        (None, _) => children.push(Box::new(el(
            "p",
            format!("Choose a reading to see the {class} block drawn by its adapter."),
        ))),
        (Some(_), None) => children.push(Box::new(status_note("blocks-view-status", WAITING))),
        (Some(_), Some(ViewState { view: Err(error), .. })) => children.push(Box::new(
            status_note("blocks-view-status", format!("View unavailable: {error}. Source retained.")),
        )),
        (Some(mode), Some(ViewState { view: Ok(view), .. })) => {
            let label = claim
                .views
                .iter()
                .find(|m| &m.id == mode)
                .map_or(mode.clone(), |m| m.label.clone());
            let mut overlay: Vec<Child<P>> = vec![Box::new(
                custom_leaf::<P, ()>(
                    VIEW_KEY,
                    view.width.ceil() as u32,
                    view.height.ceil() as u32,
                )
                .attr("aria-label", format!("{label} · {}", view.description))
                .attr("class", "blocks-guest-leaf"),
            )];
            let theme = page.sheet_theme();
            if let Ok(leaf) = SheetLeaf::new(view.sheet.clone(), theme) {
                let leaf = match page.sheet_ground() {
                    Some(ground) => leaf.on_ground(ground),
                    None => leaf,
                };
                for text in leaf.texts() {
                    overlay.push(Box::new(el("span", text.text.clone()).attr("aria-hidden", "true").attr(
                        "style",
                        format!(
                            "position:absolute;left:{}px;top:{}px;font-size:{}px;color:{};pointer-events:none;",
                            text.x, text.y, text.size, text.css_colour()
                        ),
                    )));
                }
            }
            let revision = source_revision(pane.source());
            let block_index = pane.selected;
            for region in view.regions() {
                let revision = revision.clone();
                let (bar, event) = (region.bar as usize, region.event as usize);
                overlay.push(Box::new(
                    button(region.label.clone(), move |p: &mut P, _| {
                        if p.pane_mut().selected == block_index
                            && source_revision(p.pane_mut().source()) == revision
                        {
                            p.pane_mut().event_selected = (bar, event);
                            p.pane_mut().event_selected_revision = Some(revision.clone());
                            p.pane_mut().event_selected_block = block_index;
                        }
                    })
                    .attr("aria-label", region.label.clone())
                    .attr(
                        "aria-pressed",
                        (pane.event_selection() == (bar, event)).to_string(),
                    )
                    .attr("class", format!("blocks-guest-event blocks-guest-event-{bar}-{event}"))
                    .attr(
                        "style",
                        format!(
                            "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;color:transparent;background:transparent;border-color:transparent;padding:0;",
                            region.x, region.y, region.w, region.h
                        ),
                    ),
                ));
            }
            children.push(Box::new(
                el("div", overlay).attr("class", "blocks-guest").attr(
                    "style",
                    format!(
                        "position:relative;width:{}px;height:{}px;flex:none;",
                        view.width, view.height
                    ),
                ),
            ));
            children.push(Box::new(status_note(
                "blocks-guest-selection",
                if pane.event_selection().0 == usize::MAX {
                    "Select an event to carry attention across the readings".to_string()
                } else {
                    format!(
                        "Selected bar {}, event {}",
                        pane.event_selection().0 + 1,
                        pane.event_selection().1 + 1
                    )
                },
            )));
        },
    }
    Box::new(el("div", children).attr("class", "blocks-guest-reading"))
}

/// The block pane's focused text field, if one is focused. `has_label(dom, node, label)` is the
/// host's test that the focused DOM node is the field authored with that `aria-label`.
pub fn focused_text<P: PaneHost>(
    runner: &cambium_rootstock::Runner<P, Logic<P>, Child<P>>,
    has_label: impl Fn(&ScriptedDom, NodeId, &str) -> bool,
) -> Option<cambium_rootstock::FocusedTextSlot<P>> {
    if !runner.state().pane().open {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let pane = runner.state().pane();
    let mut labels: Vec<(String, Option<String>)> = vec![
        (SOURCE_LABEL.to_owned(), None),
        (QUERY_LABEL.to_owned(), None),
    ];
    if let Some(class) = pane.current_class() {
        if let Some(claim) = pane.adapter(&class).map(|a| a.claim().clone()) {
            if let Some(field) = &claim.pattern {
                labels.push((field.label.clone(), Some(format!("pattern:{class}"))));
            }
            for command in &claim.commands {
                if let Some(field) = &command.arg {
                    labels.push((field.label.clone(), Some(format!("arg:{class}:{}", command.id))));
                }
            }
        }
    }
    let (index, key) = labels
        .iter()
        .enumerate()
        .find(|(_, (label, _))| has_label(&*dom, node, label))
        .map(|(i, (_, key))| (i, key.clone()))?;
    let key_get = key.clone();
    let key_set = key;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |p| match (index, &key_get) {
            (0, _) => &p.pane().source_input,
            (1, _) => &p.pane().query,
            (_, Some(key)) => p.pane().fields.get(key).unwrap_or(&p.pane().query),
            _ => &p.pane().query,
        }),
        get_mut: Box::new(move |p| match (index, &key_set) {
            (0, _) => &mut p.pane_mut().source_input,
            (1, _) => &mut p.pane_mut().query,
            (_, Some(key)) => p.pane_mut().fields.entry(key.clone()).or_default(),
            _ => &mut p.pane_mut().query,
        }),
    })
}

struct Producer {
    display: Rc<RefCell<Display>>,
    style: Rc<dyn Fn() -> SwatchStyle>,
    texture: Option<wgpu::Texture>,
    generation: u64,
    last: Option<(u64, u64, (u32, u32), cambium_rootstock::ProducedScene)>,
}
impl Producer {
    fn produce(
        &mut self,
        frame: &cambium_rootstock::ProducerFrameInfo,
    ) -> Result<cambium_rootstock::ProducedScene, cambium_rootstock::PortableProducerError> {
        let mut display = self.display.borrow_mut();
        let size = (
            frame.logical_size.0.max(1.0).round() as u32,
            frame.logical_size.1.max(1.0).round() as u32,
        );
        let style = (self.style)();
        let appearance_revision = style.revision;
        if frame.purpose == cambium_rootstock::ProducerFramePurpose::Export {
            return self
                .last
                .as_ref()
                .filter(|(view, appearance, previous, _)| {
                    *view == display.generation
                        && *appearance == appearance_revision
                        && *previous == size
                })
                .map(|(_, _, _, output)| output.clone())
                .ok_or(cambium_rootstock::PortableProducerError::InvalidSource);
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let surface = display
            .surface
            .as_mut()
            .ok_or(cambium_rootstock::PortableProducerError::InvalidSource)?;
        surface.set_font_roles(&style.font_roles);
        let scene = surface
            .paint(
                frame.logical_size.0.max(1.0) as u32,
                frame.logical_size.1.max(1.0) as u32,
                &style.theme,
            )
            .map_err(|_| cambium_rootstock::PortableProducerError::InvalidSource)?;
        let output = cambium_rootstock::ProducedScene {
            scene,
            source_identity: SWATCH_KEY,
            source_revision: generation,
            alpha: SourceAlpha::Premultiplied,
            encoding: SourceEncoding::Srgb,
        };
        self.last = Some((
            display.generation,
            appearance_revision,
            size,
            output.clone(),
        ));
        Ok(output)
    }
}
impl TextureProducer for Producer {
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        let output = self.produce(cx.frame).ok()?;
        let [w, h] = cx.frame.physical_size;
        let (texture, view) = cx.core.rasterize_scaled_for(
            SWATCH_KEY,
            &output.scene,
            w,
            h,
            ColorLoad::Clear(SHELL_CLEAR),
            cx.frame.layout_scale,
        );
        self.texture = Some(texture);
        Some(ProducedTexture {
            view,
            generation: output.source_revision,
            alpha: output.alpha,
            encoding: output.encoding,
        })
    }
    fn portable_scene(
        &mut self,
        frame: &cambium_rootstock::ProducerFrameInfo,
    ) -> Result<cambium_rootstock::ProducedScene, cambium_rootstock::PortableProducerError> {
        self.produce(frame)
    }
    fn suspend(&mut self) {
        self.texture = None;
        self.last = None;
    }
}
pub fn register<P: PaneHost>(ctx: &mut AppCtx<'_, P, Logic<P>, Child<P>>) {
    // The person's keys reach the editor, and an undo or redo comes back.
    if ctx.runner.state().pane().has_pending_input() {
        ctx.runner.update(|p| p.pane_mut().sync_input());
    }
    // A read-only pane reads its document again after each of the host's
    // refreshes, so it shows what the writing view last saved.
    let refreshes = ctx.runner.state().refreshes();
    match ctx.runner.state().pane().refreshed {
        None => ctx.runner.update(|p| p.pane_mut().refreshed = Some(refreshes)),
        Some(seen) if seen != refreshes && ctx.runner.state().pane().read_only() => {
            let wake = ctx.wake.clone();
            ctx.runner.update(|p| p.pane_mut().reload(refreshes, wake));
        },
        _ => {},
    }
    let reloaded = ctx.runner.state().pane().reloaded.borrow_mut().take();
    if let Some(loaded) = reloaded {
        ctx.runner.update(|p| p.pane_mut().adopt_reload(loaded));
    }
    let completion = ctx.runner.state().pane().completed.borrow_mut().take();
    if let Some(completion) = completion {
        ctx.runner.update(|p| {
            p.pane_mut().busy = false;
            match completion {
                Ok((bytes, source)) => {
                    p.pane_mut().expected = Some(bytes);
                    p.pane_mut().edit(|d| d.editor.accept_saved_source(&source));
                    p.pane_mut().status = "Document saved".into();
                },
                Err(error) => p.pane_mut().status = error,
            }
        });
    }
    if ctx.runner.state().pane().pending.is_some() {
        ctx.runner.update(|p| p.pane_mut().retry_pending());
    }
    let page = ctx.runner.state();
    let visible = page.pane().open
        && page
            .pane()
            .current_class()
            .is_some_and(|c| page.pane().view_modes.contains_key(&c));
    if visible {
        let width = view_width(page);
        let changed = {
            let mut changed = false;
            ctx.runner.update(|p| changed = p.pane_mut().refresh_view(width));
            changed
        };
        let page = ctx.runner.state();
        let signature = format!(
            "{}:{}",
            page.pane().view.as_ref().map_or(0, |v| v.generation),
            (page.swatch_style())().revision
        );
        if (changed || page.pane().view_leaf_signature.as_ref() != Some(&signature))
            && let Some(ViewState { view: Ok(view), .. }) = &page.pane().view
        {
            match SheetLeaf::new(view.sheet.clone(), page.sheet_theme()) {
                Ok(leaf) => {
                    ctx.leaves.insert(VIEW_KEY, Box::new(leaf));
                },
                Err(_) => {
                    ctx.leaves.remove(&VIEW_KEY);
                },
            }
            ctx.runner
                .update(|p| p.pane_mut().view_leaf_signature = Some(signature));
        }
    } else {
        ctx.leaves.remove(&VIEW_KEY);
        if ctx.runner.state().pane().view_leaf_signature.is_some() {
            ctx.runner.update(|p| {
                p.pane_mut().view_leaf_signature = None;
                p.pane_mut().view = None;
            });
        }
    }
    if !ctx.runner.state().pane().open {
        ctx.producers.remove(SWATCH_KEY);
        return;
    }
    let pane = ctx.runner.state().pane();
    let refresh_key = format!(
        "{}:{}:{}:{}",
        source_revision(pane.source()),
        pane.selected,
        pane.preview
            .as_ref()
            .map(|p| source_revision(&p.source))
            .unwrap_or_default(),
        pane.preview
            .as_ref()
            .map(|p| p
                .changes
                .iter()
                .map(|c| format!("{}:{}:{}", c.index, c.after, c.use_update))
                .collect::<Vec<_>>()
                .join(";"))
            .unwrap_or_default()
    );
    if pane.refresh_key.as_ref() != Some(&refresh_key) {
        let wake = ctx.wake.clone();
        ctx.runner.update(|p| {
            let pane = p.pane_mut();
            pane.wake = Some(wake);
            pane.refresh_key = Some(refresh_key);
            // An adapter that has not answered is asked again next frame.
            if pane.refresh() {
                pane.refresh_key = None;
            }
        });
    }
    if ctx.runner.state().pane().display.borrow().surface.is_none() {
        ctx.producers.remove(SWATCH_KEY);
        return;
    }
    if !ctx.producers.contains(SWATCH_KEY) {
        let producer = Producer {
            display: ctx.runner.state().pane().display.clone(),
            style: ctx.runner.state().swatch_style(),
            texture: None,
            generation: 0,
            last: None,
        };
        if let Err(error) = ctx.producers.register(SWATCH_KEY, producer, &[]) {
            eprintln!("document Swatch: {error:?}");
        }
    }
}
impl Drop for Pane {
    fn drop(&mut self) {
        if let Some(url) = self.export_url.take() {
            self.ports.release_export(&url);
        }
    }
}

impl Pane {
    /// The probe fields a host publishes for scenarios (`blocks-view-mode`, `blocks-source`, ...).
    pub fn probe_fields(&self) -> Vec<(&'static str, String)> {
        let pane = self;
        let class = pane.current_class();
        let mut fields = vec![
            (
                "blocks-view-mode",
                class
                    .as_ref()
                    .and_then(|c| {
                        let id = pane.view_modes.get(c)?;
                        let claim = pane.registry.get(c)?.claim();
                        claim
                            .views
                            .iter()
                            .find(|v| &v.id == id)
                            .map(|v| v.label.clone())
                    })
                    .unwrap_or_else(|| "Arrangement".into()),
            ),
            (
                "blocks-event-selected",
                format!("{}:{}", pane.event_selection().0, pane.event_selection().1),
            ),
            ("blocks-adapters", pane.registry.classes().join(",")),
            ("blocks-unclaimed-notices", pane.unclaimed_notices().to_string()),
            ("blocks-review-requested", pane.review_requested.to_string()),
            ("blocks-open", pane.open.to_string()),
            ("blocks-source", pane.source().to_string()),
            ("blocks-dirty", pane.dirty().to_string()),
            ("blocks-saving", pane.busy.to_string()),
            ("blocks-preview", pane.preview.is_some().to_string()),
            ("blocks-status", pane.status.clone()),
        ];
        if self.woodshed_passage_proof {
            fields.extend(woodshed_passage::probe_fields(self));
        }
        fields
    }
}

#[cfg(test)]
pub(super) mod editor_tests {
    use super::*;
    use crate::block_editing::BufferEditor;
    use cambium::{BlockEditor, CaretSelection, EditOutcome, TextCommand};
    use std::future::Future;

    struct NoPorts;
    impl PanePorts for NoPorts {
        fn spawn(&self, _task: Task) {}
        fn export(&self, _bytes: &[u8], _mime: &str, _name: &str) -> Result<String, String> {
            Err("no exports".into())
        }
        fn release_export(&self, _handle: &str) {}
    }

    /// A buffer that always reports itself dirty: distinguishable from the default.
    struct AlwaysDirty(BufferEditor);
    impl BlockEditor for AlwaysDirty {
        fn source(&self) -> &str {
            self.0.source()
        }
        fn selection(&self) -> CaretSelection {
            self.0.selection()
        }
        fn apply(&mut self, command: TextCommand) -> EditOutcome {
            self.0.apply(command)
        }
        fn is_dirty(&self) -> bool {
            true
        }
        fn accept_saved_source(&mut self, source: &str) {
            self.0.accept_saved_source(source)
        }
    }

    fn always_dirty() -> Rc<dyn EditorFactory> {
        Rc::new(|_: &str, source: &str| -> Box<dyn BlockEditor> {
            Box::new(AlwaysDirty(BufferEditor::new(source)))
        })
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
        }
    }

    pub(super) fn pane(editors: Option<Rc<dyn EditorFactory>>) -> Pane {
        let ports: Rc<dyn PanePorts> = Rc::new(NoPorts);
        match editors {
            Some(editors) => block_on(Pane::load_with(None, ports, editors)),
            None => block_on(Pane::load(None, ports)),
        }
    }

    #[test]
    fn two_panes_in_one_process_edit_through_different_editors() {
        let custom = pane(Some(always_dirty()));
        let default = pane(None);
        assert!(custom.document.editor.is_dirty(), "the passed editor is the custom one");
        assert!(!default.document.editor.is_dirty(), "the other pane keeps the default buffer");
        assert_eq!(custom.document.editor.source(), default.document.editor.source());
    }

    #[test]
    fn no_editor_choice_leaks_between_panes_or_documents() {
        // A custom pane opened first does not change what later panes or bare
        // documents are made of: there is no process-wide editor.
        let _custom = pane(Some(always_dirty()));
        assert!(!pane(None).document.editor.is_dirty());
        assert!(!Document::new("memory:fresh", "text").editor.is_dirty());
        assert!(Document::open("memory:custom", "text", always_dirty().as_ref()).editor.is_dirty());
        assert!(!Document::new("memory:after", "text").editor.is_dirty());
    }
}
