// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! One ordinary source editor, contextual operations, and an independent preview.
use super::*;
use cambium::{
    Command, CommandChoices, CommandSet, TextCommand, TextInput, button, lens, text_field_typed,
    textarea_typed,
};
use graphshell::block_editing::{
    Document, Preview, bars, rhyme,
    store::{self, StoredDocument},
};
use mere::canvas::backdrop_surface::BackdropSurface;
use muniment::IndexedDbBackend;
use scenomise::backdrops::SceneInput;
use woodshed_notation::{ColorF, NotationLeaf, NotationView};
const NOTATION_KEY: u64 = 0x6773_6e74;

pub(super) const SHEET: &str = r#"
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
.blocks-workspace button[aria-disabled=true] { opacity:0.5; }
.blocks-notation-reading { display:flex; flex-direction:column; gap:10px; flex:none; }
@media(max-width:900px) { .blocks-musical .blocks-preview { order:-1; } }
.blocks-notation-event:focus-visible { outline:2px solid var(--tabard-color-primary); outline-offset:-2px; }
.blocks-row { display:block; width:100%; margin-block:3px; font-family:var(--workspace-font-reading,serif); }
.blocks-review { display:flex; flex-direction:column; gap:6px; }
.blocks-swatch { flex:none; max-width:100%; }
.blocks-workspace .backdrop-scene-reading { display:flex; flex-direction:column; gap:5px; }
"#;

const KEY: u64 = 0x626c_6f63;
const SOURCE_LABEL: &str = "Document source";
const PATTERN_LABEL: &str = "Rhyme scheme";
const BAR_LABEL: &str = "Bar grouping";
const TEMPO_LABEL: &str = "Notation tempo (BPM)";
const QUERY_LABEL: &str = "Search block commands";
const INITIAL: &str = "# Composition\n\n{#verse}\n::: metre\nThe path runs quiet through the night\nThe rain is gathering on the stone\nA window holds its little light\nAnd leaves the farthest field alone\n:::\n";

#[derive(Default)]
struct Display {
    input: Option<SceneInput>,
    surface: Option<BackdropSurface>,
    generation: u64,
}
type Saved = Result<(Vec<u8>, String), String>;
pub(super) struct Pane {
    document: Document,
    backend: Option<IndexedDbBackend>,
    slot: String,
    expected: Option<Vec<u8>>,
    blocked: Option<String>,
    completed: Rc<RefCell<Option<Saved>>>,
    busy: bool,
    open: bool,
    selected: usize,
    selected_rows: Option<Vec<usize>>,
    pattern: TextInput,
    query: TextInput,
    bar_pattern: TextInput,
    preview: Option<Preview>,
    preview_pattern: String,
    display: Rc<RefCell<Display>>,
    canvas: bool,
    notation: Option<NotationView>,
    notation_selected: (usize, usize),
    notation_selected_revision: Option<String>,
    notation_selected_block: usize,
    notation_tempo: TextInput,
    notation_signature: Option<String>,
    status: String,
    export_url: Option<String>,
    wake: Option<cambium_rootstock::HostWake>,
    refresh_key: Option<String>,
}
impl Pane {
    pub(super) async fn load(product: Option<&product::SavedProduct>) -> Self {
        let source = product.and_then(|p| p.forme_source());
        let mut pane = Self {
            document: Document::new(
                &format!("graphshell:document/{}", uuid::Uuid::new_v4()),
                INITIAL,
            ),
            backend: source.as_ref().map(|(b, _)| b.clone()),
            slot: source.map_or_else(
                || "document/unavailable".into(),
                |(_, id)| format!("graphshell/block-document/v1/{id}"),
            ),
            expected: None,
            blocked: None,
            completed: Default::default(),
            busy: false,
            open: false,
            selected: 0,
            selected_rows: None,
            pattern: TextInput::new("ABAB"),
            query: TextInput::default(),
            bar_pattern: TextInput::new("2 + 2"),
            preview: None,
            preview_pattern: String::new(),
            display: Default::default(),
            canvas: false,
            notation: None,
            notation_selected: (usize::MAX, usize::MAX),
            notation_selected_revision: None,
            notation_selected_block: usize::MAX,
            notation_tempo: TextInput::new("96"),
            notation_signature: None,
            status: String::new(),
            export_url: None,
            wake: None,
            refresh_key: None,
        };
        if let Some(backend) = &pane.backend {
            match store::load(backend, &pane.slot).await {
                Ok(Some((stored, bytes))) => {
                    pane.document = Document::new(&stored.id, &stored.source);
                    pane.expected = Some(bytes);
                    pane.status = "Saved document reopened".into();
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
    fn target(&self) -> Result<graphshell::block_editing::Div, String> {
        self.document
            .blocks()
            .get(self.selected)
            .cloned()
            .ok_or("Select a current document block".into())
    }
    fn rows(&self, body: &str) -> Vec<usize> {
        self.selected_rows.clone().unwrap_or_else(|| {
            body.lines()
                .enumerate()
                .filter_map(|(i, s)| (!s.trim().is_empty()).then_some(i))
                .collect()
        })
    }
    fn progression(&self) -> bool {
        self.target().is_ok_and(|d| has_class(&d, "progression"))
    }
    fn pattern_text(&self) -> &str {
        if self.progression() {
            self.bar_pattern.text()
        } else {
            self.pattern.text()
        }
    }
    fn selection(
        &self,
        div: &graphshell::block_editing::Div,
        body: &str,
    ) -> Result<Vec<usize>, String> {
        if has_class(div, "progression") {
            let rows = bars::rows(&div.attributes)?;
            Ok(self
                .selected_rows
                .clone()
                .unwrap_or_else(|| (0..rows.len()).collect()))
        } else {
            Ok(self.rows(body))
        }
    }
    fn commands(&self) -> CommandSet {
        let mut set = CommandSet::new();
        let (id, label) = if self.progression() {
            ("woodshed:group-bars", "Apply bar grouping…")
        } else {
            ("rhyme:scheme", "Apply rhyme scheme…")
        };
        set.register(Command::new(id, label, "block"));
        let reason = self.proposals().map(|_| ()).err();
        set.set_disabled(id, reason);
        set
    }
    fn proposals(&self) -> Result<Vec<graphshell::block_editing::Change>, String> {
        let div = self.target()?;
        if !div.children.is_empty() {
            return Err("Select a leaf block; nested block contents need their own adapter".into());
        }
        let body = &self.document.editor.source()[div.body.clone()];
        let selected = self.selection(&div, body)?;
        if has_class(&div, "progression") {
            bars::choices(&div.attributes, &selected, self.bar_pattern.text())
        } else if has_class(&div, "metre") {
            rhyme::choices(body, &div.attributes, &selected, self.pattern.text())
        } else {
            Err("This block has no registered editing adapter".into())
        }
    }
    fn preview(&mut self) -> Result<(), String> {
        let changes = self.proposals()?;
        self.preview = Some(Preview::new(
            self.document.editor.source(),
            self.target()?,
            changes,
        )?);
        self.preview_pattern = self.pattern_text().to_string();
        self.status = "Preview only · review changes, then Apply".into();
        Ok(())
    }
    fn preview_attributes(
        &self,
        preview: &Preview,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        let body = &preview.source[preview.block.body.clone()];
        if has_class(&preview.block, "progression") {
            bars::attributes(
                &preview.block.attributes,
                &self.selection(&preview.block, body)?,
                &preview.changes,
            )
        } else {
            rhyme::attributes(body, &preview.block.attributes, &preview.changes)
        }
    }
    fn apply(&mut self) -> Result<(), String> {
        if self.pattern_text() != self.preview_pattern {
            return Err("Pattern changed; preview it again before applying".into());
        }
        let preview = self.preview.as_ref().ok_or("Preview a change first")?;
        let attrs = self.preview_attributes(preview)?;
        let changed = attrs != preview.block.attributes;
        let progression = has_class(&preview.block, "progression");
        self.document.apply(preview, attrs)?;
        self.preview = None;
        self.status = if !changed {
            "No changes accepted · document unchanged"
        } else if progression {
            "Bar grouping applied · musical timing unchanged"
        } else {
            "Rhyme scheme applied · Undo and Save use this document’s history"
        }
        .into();
        Ok(())
    }
    fn insert_progression(&mut self) -> Result<(), String> {
        use cambium::{CaretAffinity, CaretPosition, CaretSelection};
        let attrs = std::collections::BTreeMap::from([
            ("class".into(), "progression".into()),
            ("id".into(), format!("progression-{}", uuid::Uuid::new_v4())),
            (
                bars::SOURCE.into(),
                serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../support/woodshed-block-example.json"
                ))
                .map_err(|e| e.to_string())?
                .to_string(),
            ),
        ]);
        bars::rows(&attrs)?;
        let block =
            graphshell::block_editing::wrap(&attrs, "Woodshed ii–V–I · structured arrangement\n");
        let end = self.document.editor.source().len();
        if end + block.len() + 2 > graphshell::block_editing::MAX_DOCUMENT_BYTES {
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
        self.status =
            "Inserted an independent Woodshed example · 4/4 is authored in its source".into();
        Ok(())
    }
    fn write_notation(&mut self) -> Result<(), String> {
        if self.preview.is_some() {
            return Err("Apply or cancel the current preview before writing notation".into());
        }
        let div = self.target()?;
        if !has_class(&div, "progression") || !div.children.is_empty() {
            return Err("Select a progression leaf block".into());
        }
        let bpm = self
            .notation_tempo
            .text()
            .trim()
            .parse::<u16>()
            .map_err(|_| "Enter a whole-number notation tempo from 20 to 400 BPM")?;
        let attributes = bars::write_music(&div.attributes, bpm)?;
        let preview = Preview::new(self.document.editor.source(), div, Vec::new())?;
        self.document.apply(&preview, attributes)?;
        self.notation = Some(NotationView::Chart);
        self.notation_selected = (usize::MAX, usize::MAX);
        self.status = "Notation written · saved MusicXML drives Chart, Tab and Score · Save document to keep it".into();
        Ok(())
    }
    fn notation_selection(&self) -> (usize, usize) {
        if self.notation_selected_block == self.selected
            && self.notation_selected_revision.as_ref()
                == Some(&source_revision(self.document.editor.source()))
        {
            self.notation_selected
        } else {
            (usize::MAX, usize::MAX)
        }
    }
    fn notation_leaf(
        &self,
        width: f32,
        ink: ColorF,
        accent: ColorF,
    ) -> Result<NotationLeaf, String> {
        let div = self.target()?;
        if !has_class(&div, "progression") || !div.children.is_empty() {
            return Err("Select a progression leaf block".into());
        }
        let score = bars::music(&div.attributes)?
            .ok_or("This Set has no written notation. Choose a tempo, then Write notation.")?;
        Ok(NotationLeaf::new(
            score,
            self.notation.ok_or("Select a notation reading")?,
            self.notation_selection(),
            width,
            ink,
            accent,
        ))
    }
    fn save(&mut self) -> Result<(), String> {
        if self.busy {
            return Err("Document save is pending".into());
        }
        if let Some(error) = &self.blocked {
            return Err(error.clone());
        }
        let backend = self
            .backend
            .clone()
            .ok_or("Document storage is unavailable")?;
        let document = StoredDocument {
            version: 1,
            id: self.document.id.clone(),
            source: self.document.editor.source().into(),
        };
        document.validate()?;
        let expected = self.expected.clone();
        let slot = self.slot.clone();
        let completion = self.completed.clone();
        let wake = self.wake.clone();
        self.busy = true;
        self.status = "Saving document…".into();
        wasm_bindgen_futures::spawn_local(async move {
            let source = document.source.clone();
            *completion.borrow_mut() = Some(
                store::save(&backend, &slot, expected, document)
                    .await
                    .map(|bytes| (bytes, source)),
            );
            if let Some(wake) = wake {
                wake.wake();
            }
        });
        Ok(())
    }
    fn refresh(&mut self) {
        let result = (|| {
            let div = self.target()?;
            let source = self.document.editor.source();
            let mut attrs = div.attributes.clone();
            if let Some(preview) = &self.preview {
                if preview.source != source || preview.block != div {
                    return Err("Document changed since this preview. Cancel or preview again before applying.".into());
                }
                attrs = self.preview_attributes(preview)?;
            }
            if !div.children.is_empty() {
                return Err(
                    "Select a leaf block; nested block contents need their own adapter".into(),
                );
            }
            let address = format!("{}#{}", self.document.id, self.selected);
            let revision = source_revision(source);
            if has_class(&div, "progression") {
                bars::scene(&address, &revision, &attrs)
            } else if has_class(&div, "metre") {
                rhyme::scene(&address, &revision, &source[div.body], &attrs)
            } else {
                Err("No scene adapter for this block. Its source stays editable.".into())
            }
        })();
        let mut display = self.display.borrow_mut();
        match result {
            Ok(input) if display.input.as_ref() != Some(&input) => {
                let accepted = if let Some(surface) = &mut display.surface {
                    surface.replace(&input).map_err(|e| e.to_string())
                } else {
                    BackdropSurface::new(&input)
                        .map(|mut surface| {
                            surface.register_font(
                                include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec(),
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
    }
}
fn has_class(div: &graphshell::block_editing::Div, class: &str) -> bool {
    div.attributes
        .get("class")
        .is_some_and(|s| s.split_whitespace().any(|s| s == class))
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
fn action(page: &mut TreePage, change: impl FnOnce(&mut Pane) -> Result<(), String>) {
    if let Err(error) = change(&mut page.blocks) {
        page.blocks.status = error;
    }
}
pub(super) fn control(page: &TreePage) -> Child {
    Box::new(
        button(
            if page.blocks.open {
                "Close document"
            } else {
                "Editable document"
            },
            |p: &mut TreePage, _| {
                p.blocks.open = !p.blocks.open;
            },
        )
        .attr("class", "blocks-open"),
    )
}
pub(super) fn active(page: &TreePage) -> bool {
    page.product.is_some() && page.blocks.open
}
pub(super) fn toolbar(page: &TreePage) -> Option<Child> {
    if !active(page) {
        return None;
    }
    let pane = &page.blocks;
    let set = pane.commands();
    let commands = set.menu(&CommandChoices::default(), Some("block"), pane.query.text());
    let mut children: Vec<Child> = vec![
        Box::new(
            button("Return to graph", |p: &mut TreePage, _| {
                p.blocks.open = false
            })
            .attr("class", "blocks-close"),
        ),
        Box::new(
            el(
                "div",
                (
                    button("Save document", |p: &mut TreePage, _| action(p, Pane::save))
                        .attr(
                            "aria-disabled",
                            (pane.busy || pane.blocked.is_some()).to_string(),
                        )
                        .attr("class", "blocks-save"),
                    button("Undo document edit", |p: &mut TreePage, _| {
                        p.blocks.document.editor.apply(TextCommand::Undo);
                        p.blocks.preview = None;
                    })
                    .attr("class", "blocks-undo"),
                    button("Redo document edit", |p: &mut TreePage, _| {
                        p.blocks.document.editor.apply(TextCommand::Redo);
                        p.blocks.preview = None;
                    })
                    .attr("class", "blocks-redo"),
                    button(
                        "Export document",
                        |p: &mut TreePage, _| match backdrop::export_material(
                            p.blocks.document.editor.source().as_bytes(),
                            "text/djot",
                            "composition.djot",
                        ) {
                            Ok(url) => {
                                if let Some(old) = p.blocks.export_url.replace(url) {
                                    let _ = web_sys::Url::revoke_object_url(&old);
                                }
                            },
                            Err(error) => p.blocks.status = error,
                        },
                    ),
                ),
            )
            .attr("class", "blocks-actions"),
        ),
        Box::new(
            button("Insert Woodshed example", |p: &mut TreePage, _| {
                action(p, Pane::insert_progression)
            })
            .attr("class", "blocks-insert-progression"),
        ),
    ];
    for command in commands {
        children.push(Box::new(
            button(command.label.clone(), |p: &mut TreePage, _| {
                action(p, Pane::preview)
            })
            .attr(
                "aria-disabled",
                command.disabled_reason.is_some().to_string(),
            )
            .attr("class", "blocks-preview-command"),
        ));
    }
    Some(Box::new(
        el("div", children)
            .attr("class", "blocks-toolbar blocks-actions")
            .attr("aria-label", "Document commands"),
    ))
}
pub(super) fn busy(page: &TreePage) -> bool {
    page.blocks.busy
}
pub(super) fn workspace(page: &TreePage) -> Option<Child> {
    if page.product.is_none() || !page.blocks.open {
        return None;
    }
    let pane = &page.blocks;
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Composition")),
        Box::new(button(
            "Create verse block from selected lines",
            |p: &mut TreePage, _| {
                action(p, |pane| {
                    let id = format!("verse-{}", uuid::Uuid::new_v4());
                    pane.document
                        .wrap_selection(std::collections::BTreeMap::from([
                            ("class".into(), "metre".into()),
                            ("id".into(), id.clone()),
                        ]))?;
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
        )),
        Box::new(el(
            "p",
            format!(
                "{} · {}",
                "Composition",
                if pane.document.editor.is_dirty() {
                    "Unsaved changes"
                } else if pane.expected.is_none() {
                    "Example composition · not saved"
                } else {
                    "Saved source"
                }
            ),
        )),
        Box::new(
            el("p", pane.status.clone())
                .attr("role", "status")
                .attr("class", "blocks-status"),
        ),
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
                    |page: &mut TreePage| page.blocks.document.editor.input_mut(),
                ),
            ),
        )),
    ];
    let blocks = pane.document.blocks();
    let mut block_buttons: Vec<Child> = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let label = if has_class(block, "progression") {
            format!("Woodshed progression {}", index + 1)
        } else {
            block
                .attributes
                .get("id")
                .cloned()
                .unwrap_or_else(|| format!("Block {}", index + 1))
        };
        block_buttons.push(Box::new(
            button(label, move |p: &mut TreePage, _| {
                p.blocks.selected = index;
                p.blocks.selected_rows = None;
                p.blocks.preview = None;
            })
            .attr("aria-pressed", (pane.selected == index).to_string())
            .attr("class", format!("blocks-select-{index}")),
        ));
    }
    children.push(Box::new(
        el("div", block_buttons).attr("class", "blocks-actions"),
    ));
    if let Ok(div) = pane.target() {
        let body = &pane.document.editor.source()[div.body.clone()];
        let selected = pane.selection(&div, body).unwrap_or_default();
        let mut rows: Vec<Child> = Vec::new();
        let lines: Vec<(usize, String)> = if has_class(&div, "progression") {
            bars::rows(&div.attributes)
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .collect()
        } else {
            body.lines()
                .enumerate()
                .filter(|(_, s)| !s.trim().is_empty())
                .map(|(i, s)| (i, s.to_string()))
                .collect()
        };
        for (index, line) in lines {
            rows.push(Box::new(
                button(
                    if has_class(&div, "progression") {
                        line
                    } else {
                        format!("{} · {}", index + 1, line)
                    },
                    move |p: &mut TreePage, _| {
                        if let Ok(div) = p.blocks.target() {
                            let mut selected = p
                                .blocks
                                .selection(
                                    &div,
                                    &p.blocks.document.editor.source()[div.body.clone()],
                                )
                                .unwrap_or_default();
                            if selected.contains(&index) {
                                selected.retain(|i| *i != index);
                            } else {
                                selected.push(index);
                                selected.sort();
                            }
                            p.blocks.selected_rows = Some(selected);
                            p.blocks.preview = None;
                        }
                    },
                )
                .attr("aria-pressed", selected.contains(&index).to_string())
                .attr("class", format!("blocks-row blocks-row-{index}")),
            ));
        }
        children.push(Box::new(el("div", rows).attr(
            "aria-label",
            if pane.progression() {
                "Select bars"
            } else {
                "Select composition lines"
            },
        )));
        let pattern_label = if pane.progression() {
            BAR_LABEL
        } else {
            PATTERN_LABEL
        };
        children.push(Box::new(el(
            "label",
            (
                if pane.progression() {
                    BAR_LABEL
                } else {
                    PATTERN_LABEL
                },
                lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input).attr("aria-label", pattern_label)
                    },
                    |p: &mut TreePage| {
                        if p.blocks.progression() {
                            &mut p.blocks.bar_pattern
                        } else {
                            &mut p.blocks.pattern
                        }
                    },
                ),
            ),
        )));
        children.push(Box::new(el(
            "label",
            (
                QUERY_LABEL,
                lens(
                    |input: &mut TextInput| text_field_typed(input).attr("aria-label", QUERY_LABEL),
                    |p: &mut TreePage| &mut p.blocks.query,
                ),
            ),
        )));
        let set = pane.commands();
        for command in set.menu(&CommandChoices::default(), Some("block"), pane.query.text()) {
            if let Some(reason) = &command.disabled_reason {
                children.push(Box::new(el("p", reason.clone())));
            }
        }
        if pane.progression() {
            children.push(Box::new(button("Export bar-line plaintext",|p:&mut TreePage,_|{
            action(p,|pane|{
                let text=bars::plaintext(&pane.target()?.attributes)?;
                let url=backdrop::export_material(text.as_bytes(),"text/plain","progression.txt")?;
                if let Some(old)=pane.export_url.replace(url){let _=web_sys::Url::revoke_object_url(&old);}
                pane.status="Plaintext chart exported · structured source retains timing and voicings".into();Ok(())
            });
        }).attr("class","blocks-export-plaintext")));
        }
    } else {
        children.push(Box::new(el("p", "Use an attributed Djot block to offer domain commands. Unrecognized content remains editable.")));
    }
    let mut preview_rows: Vec<Child> = Vec::new();
    if let Some(preview) = &pane.preview {
        let stale = preview.source != pane.document.editor.source()
            || pane.preview_pattern != pane.pattern_text();
        for (index, change) in preview.changes.iter().enumerate() {
            let label = format!(
                "{} · {} → {} · {}",
                change.label,
                change.before.as_deref().unwrap_or(if pane.progression() {
                    "Ungrouped"
                } else {
                    "No expected rhyme"
                }),
                change.after,
                if change.use_update {
                    "Use update"
                } else {
                    "Keep local"
                }
            );
            preview_rows.push(Box::new(
                button(label, move |p: &mut TreePage, _| {
                    if let Some(change) = p
                        .blocks
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
                    button("Apply to document", |p: &mut TreePage, _| {
                        action(p, Pane::apply)
                    })
                    .attr("aria-disabled", stale.to_string())
                    .attr("class", "blocks-apply"),
                    button("Cancel preview", |p: &mut TreePage, _| {
                        p.blocks.preview = None;
                        p.blocks.status = "Preview cancelled · source unchanged".into();
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
    let mut surface_children: Vec<Child> = Vec::new();
    surface_children.push(Box::new(el("p", if pane.progression(){"Bars follow Woodshed’s authored time-signature map. Grouping changes neither Card order nor musical timing. Plaintext export omits exact timing, voicings and identities."}else{"Rhyme labels are authored expectations. Unknown pronunciation and stress remain unknown. Unselected lines keep their existing expectations; an unset expectation is shown as ‘No expected rhyme’."})));
    if pane.progression() {
        surface_children.push(notation_readings(page));
    }
    let display = pane.display.borrow();
    if let Some(surface) = &display.surface
        && (!pane.progression() || pane.notation.is_none())
    {
        let available = if page.size.0 > 900 {
            page.size.0.saturating_sub(64) / 2
        } else {
            page.size.0.saturating_sub(44)
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
                |p: &mut TreePage, _| p.blocks.canvas = !p.blocks.canvas,
            )
            .attr("class", "blocks-expand"),
        ));
        surface_children.push(Box::new(
            custom_leaf::<TreePage, ()>(KEY, width, height)
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
            |p: &mut TreePage, event| {
                match &event {
                    cambium::BackdropSceneEvent::Select(key) => {
                        if let Some(index) = key.strip_prefix("line-").and_then(|s| s.parse().ok())
                        {
                            p.blocks.selected_rows = Some(vec![index]);
                            p.blocks.preview = None;
                        }
                    },
                    cambium::BackdropSceneEvent::Clear => {
                        p.blocks.selected_rows = Some(Vec::new());
                        p.blocks.preview = None;
                    },
                    _ => {},
                }
                let mut display = p.blocks.display.borrow_mut();
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
            if pane.progression() {
                "blocks-workspace blocks-musical"
            } else {
                "blocks-workspace"
            },
        )
        .attr("aria-label", "Editable document workspace"),
    ))
}

fn notation_width(page: &TreePage) -> f32 {
    if page.size.0 > 900 {
        page.size.0.saturating_sub(64) as f32 / 2.0
    } else {
        page.size.0.saturating_sub(44) as f32
    }
    .max(240.0)
}
fn notation_colors(page: &TreePage) -> (ColorF, ColorF) {
    let theme = page.shared.appearance.borrow().backdrop_theme();
    let color = |a: [f32; 4]| ColorF {
        r: a[0],
        g: a[1],
        b: a[2],
        a: a[3],
    };
    (color(theme.text), color(theme.root))
}
fn notation_readings(page: &TreePage) -> Child {
    let pane = &page.blocks;
    let mut children: Vec<Child> = Vec::new();
    let mut readings: Vec<Child> = Vec::new();
    for (view, label, key) in [
        (None, "Arrangement", "arrangement"),
        (Some(NotationView::Chart), "Chart", "chart"),
        (Some(NotationView::Tab), "Tab", "tab"),
        (Some(NotationView::Score), "Score", "score"),
    ] {
        readings.push(Box::new(
            button(label, move |p: &mut TreePage, _| {
                p.blocks.notation = view;
            })
            .attr("aria-pressed", (pane.notation == view).to_string())
            .attr("class", format!("blocks-reading-{key}")),
        ));
    }
    children.push(Box::new(
        el("div", readings)
            .attr("class", "blocks-actions")
            .attr("aria-label", "Musical reading"),
    ));
    let music = pane.target().and_then(|d| bars::music(&d.attributes));
    match music {
        Ok(None) => {
            children.push(Box::new(el("p", "Chart, Tab and Score read the Set’s written MusicXML. Write notation explicitly from the authored arrangement and selected Card shapes. The tempo below applies where a Card has no tempo.")));
            children.push(Box::new(el(
                "label",
                (
                    TEMPO_LABEL,
                    lens(
                        |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", TEMPO_LABEL)
                        },
                        |p: &mut TreePage| &mut p.blocks.notation_tempo,
                    ),
                ),
            )));
            children.push(Box::new(
                button("Write notation", |p: &mut TreePage, _| {
                    action(p, Pane::write_notation)
                })
                .attr("class", "blocks-write-notation")
                .attr("aria-disabled", pane.preview.is_some().to_string()),
            ));
        },
        Err(error) => children.push(Box::new(
            el(
                "p",
                format!("Notation unavailable: {error}. Source retained."),
            )
            .attr("role", "status"),
        )),
        Ok(Some(score)) if pane.notation.is_some() => {
            let (ink, accent) = notation_colors(page);
            match pane.notation_leaf(notation_width(page), ink, accent) {
                Ok(leaf) => {
                    let size = leaf.intrinsic_size();
                    let mut overlay: Vec<Child> = vec![Box::new(
                        custom_leaf::<TreePage, ()>(
                            NOTATION_KEY,
                            size.width as u32,
                            size.height.ceil() as u32,
                        )
                        .attr(
                            "aria-label",
                            format!(
                                "{} notation · {}",
                                pane.notation.unwrap().label(),
                                score.title
                            ),
                        )
                        .attr("class", "blocks-notation-leaf"),
                    )];
                    for label in leaf.labels() {
                        overlay.push(Box::new(
                            el("span", label.text).attr("aria-hidden", "true").attr(
                                "style",
                                format!(
                                    "position:absolute;left:{}px;top:{}px;pointer-events:none;",
                                    label.x, label.y
                                ),
                            ),
                        ));
                    }
                    let revision = source_revision(pane.document.editor.source());
                    let block_index = pane.selected;
                    for (bar, event, (x, y, width, height)) in leaf.event_rects() {
                        let revision = revision.clone();
                        let note = &score.measures[bar].events[event];
                        let label = format!(
                            "Bar {}, event {} · {} · {}",
                            bar + 1,
                            event + 1,
                            note.value.label(),
                            if note.rest {
                                "Rest".into()
                            } else {
                                note.notes
                                    .iter()
                                    .map(|n| format!("string {} fret {}", n.string, n.fret))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        );
                        overlay.push(Box::new(button(label.clone(),move |p: &mut TreePage,_| {
                            if p.blocks.selected == block_index && source_revision(p.blocks.document.editor.source()) == revision {
                                p.blocks.notation_selected = (bar,event);
                                p.blocks.notation_selected_revision = Some(revision.clone());
                                p.blocks.notation_selected_block = block_index;
                            }
                        }).attr("aria-label",label).attr("aria-pressed",(pane.notation_selection()==(bar,event)).to_string()).attr("class",format!("blocks-notation-event blocks-notation-event-{bar}-{event}")).attr("style",format!("position:absolute;left:{x}px;top:{y}px;width:{width}px;height:{height}px;padding:0;border:0;background:transparent;color:transparent;"))));
                    }
                    children.push(Box::new(
                        el("div", overlay).attr("class", "blocks-notation").attr(
                            "style",
                            format!(
                                "position:relative;width:{}px;height:{}px;flex:none;",
                                size.width, size.height
                            ),
                        ),
                    ));
                    let selection = score
                        .measures
                        .get(pane.notation_selection().0)
                        .and_then(|m| m.events.get(pane.notation_selection().1));
                    children.push(Box::new(el("p", selection.map_or_else(|| "Select an event to carry attention across Chart, Tab and Score.".into(), |e|format!("Selected bar {}, event {} · {}{}", pane.notation_selected.0+1,pane.notation_selected.1+1,e.value.label(),if e.rest {" rest"} else {""}))).attr("role","status").attr("class","blocks-notation-selection")));
                },
                Err(error) => children.push(Box::new(el("p", error))),
            }
        },
        Ok(Some(_)) => {},
    }
    Box::new(el("div", children).attr("class", "blocks-notation-reading"))
}

pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    if !runner.state().blocks.open {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let index = [
        SOURCE_LABEL,
        PATTERN_LABEL,
        QUERY_LABEL,
        BAR_LABEL,
        TEMPO_LABEL,
    ]
    .iter()
    .position(|label| focused_field_has_label(&dom, node, label))?;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |p| match index {
            0 => p.blocks.document.editor.input(),
            1 => &p.blocks.pattern,
            2 => &p.blocks.query,
            3 => &p.blocks.bar_pattern,
            _ => &p.blocks.notation_tempo,
        }),
        get_mut: Box::new(move |p| match index {
            0 => p.blocks.document.editor.input_mut(),
            1 => &mut p.blocks.pattern,
            2 => &mut p.blocks.query,
            3 => &mut p.blocks.bar_pattern,
            _ => &mut p.blocks.notation_tempo,
        }),
    })
}

struct Producer {
    display: Rc<RefCell<Display>>,
    appearance: crate::web_appearance::Handle,
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
        let appearance_revision = self.appearance.borrow().revision;
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
        let appearance = self.appearance.borrow();
        surface.set_font_roles(&appearance.font_roles());
        let scene = surface
            .paint(
                frame.logical_size.0.max(1.0) as u32,
                frame.logical_size.1.max(1.0) as u32,
                &appearance.backdrop_theme(),
            )
            .map_err(|_| cambium_rootstock::PortableProducerError::InvalidSource)?;
        let output = cambium_rootstock::ProducedScene {
            scene,
            source_identity: KEY,
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
            KEY,
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
pub(super) fn register(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    let completion = ctx.runner.state().blocks.completed.borrow_mut().take();
    if let Some(completion) = completion {
        ctx.runner.update(|p| {
            p.blocks.busy = false;
            match completion {
                Ok((bytes, source)) => {
                    p.blocks.expected = Some(bytes);
                    p.blocks.document.editor.accept_saved_source(&source);
                    p.blocks.status = "Document saved".into();
                },
                Err(error) => p.blocks.status = error,
            }
        });
    }
    let page = ctx.runner.state();
    let visible = page.blocks.open && page.blocks.progression() && page.blocks.notation.is_some();
    let signature = format!(
        "{}:{}:{:?}:{:?}:{}:{}",
        page.blocks.selected,
        source_revision(page.blocks.document.editor.source()),
        page.blocks.notation,
        page.blocks.notation_selected,
        notation_width(page),
        page.shared.appearance.borrow().revision
    );
    if visible && page.blocks.notation_signature.as_ref() != Some(&signature) {
        let (ink, accent) = notation_colors(page);
        match page.blocks.notation_leaf(notation_width(page), ink, accent) {
            Ok(leaf) => {
                ctx.leaves.insert(NOTATION_KEY, Box::new(leaf));
            },
            Err(_) => {
                ctx.leaves.remove(&NOTATION_KEY);
            },
        }
        ctx.runner
            .update(|p| p.blocks.notation_signature = Some(signature));
    } else if !visible {
        ctx.leaves.remove(&NOTATION_KEY);
        if page.blocks.notation_signature.is_some() {
            ctx.runner.update(|p| p.blocks.notation_signature = None);
        }
    }
    if !ctx.runner.state().blocks.open {
        ctx.producers.remove(KEY);
        return;
    }
    let pane = &ctx.runner.state().blocks;
    let refresh_key = format!(
        "{}:{}:{}:{}",
        source_revision(pane.document.editor.source()),
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
            p.blocks.wake = Some(wake);
            p.blocks.refresh_key = Some(refresh_key);
            p.blocks.refresh();
        });
    }
    if ctx.runner.state().blocks.display.borrow().surface.is_none() {
        ctx.producers.remove(KEY);
        return;
    }
    if !ctx.producers.contains(KEY) {
        let producer = Producer {
            display: ctx.runner.state().blocks.display.clone(),
            appearance: ctx.runner.state().shared.appearance.clone(),
            texture: None,
            generation: 0,
            last: None,
        };
        if let Err(error) = ctx.producers.register(KEY, producer, &[]) {
            web_sys::console::error_1(&format!("document Swatch: {error:?}").into());
        }
    }
}
impl Drop for Pane {
    fn drop(&mut self) {
        if let Some(url) = self.export_url.take() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    }
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.blocks;
    snapshot
        .with_field(
            "blocks-notation-view",
            pane.notation.map_or("Arrangement", NotationView::label),
        )
        .with_field(
            "blocks-notation-selected",
            format!(
                "{}:{}",
                pane.notation_selection().0,
                pane.notation_selection().1
            ),
        )
        .with_field("blocks-open", pane.open.to_string())
        .with_field("blocks-source", pane.document.editor.source().to_string())
        .with_field("blocks-dirty", pane.document.editor.is_dirty().to_string())
        .with_field("blocks-saving", pane.busy.to_string())
        .with_field("blocks-preview", pane.preview.is_some().to_string())
        .with_field("blocks-status", pane.status.clone())
}
