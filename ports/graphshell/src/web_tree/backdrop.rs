// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's reference embedding. The recipes, paint and semantic reading
//! are shared; this module owns only admission, view state and host effects.
use super::*;
use cambium::{BackdropSceneEvent, backdrop_scene_reading, button, on_key};
use mere::canvas::backdrop_surface::BackdropSurface;
use scenomise::document::SceneDocument;
use scenomise::{
    backdrops::SceneInput,
    span_edit::{SpanEdit, SpanEditIntent},
};

const KEY: u64 = 0x6261_636b;
const RELATED_KEY: u64 = KEY + 1;
const SAMPLES: [(&str, &str); 7] = [
    (
        "Metre grid",
        include_str!("../../../../support/design-studies/backdrops/metre.json"),
    ),
    (
        "Progression in C",
        include_str!("../../../../support/design-studies/backdrops/templates/progression.json"),
    ),
    (
        "Progression in G",
        include_str!("../../../../support/design-studies/backdrops/templates/progression-g.json"),
    ),
    (
        "Regular arrangement",
        include_str!(
            "../../../../support/design-studies/backdrops/templates/arrangement-regular.json"
        ),
    ),
    (
        "Varied arrangement",
        include_str!(
            "../../../../support/design-studies/backdrops/templates/arrangement-varied.json"
        ),
    ),
    (
        "Fretboard",
        include_str!("../../../../support/design-studies/backdrops/fretboard.json"),
    ),
    (
        "Terrain and coverage",
        include_str!("../../../../support/design-studies/backdrops/terrain.json"),
    ),
];
struct ImportPicker {
    input: web_sys::HtmlInputElement,
    _changed: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for ImportPicker {
    fn drop(&mut self) {
        self.input.set_onchange(None);
        self.input.remove();
    }
}

fn begin_import(pane: &Rc<RefCell<Pane>>) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("No browser document")?;
    let input: web_sys::HtmlInputElement = document
        .create_element("input")
        .map_err(|e| format!("Cannot create recipe picker: {e:?}"))?
        .dyn_into()
        .map_err(|_| "Recipe picker unavailable")?;
    input.set_type("file");
    input.set_accept("application/json,.json");
    input
        .set_attribute("hidden", "")
        .map_err(|e| format!("Cannot prepare picker: {e:?}"))?;
    let generation = {
        let mut pane = pane.borrow_mut();
        pane.import_generation = pane.import_generation.wrapping_add(1);
        pane.pending = None;
        pane.import_generation
    };
    let weak = Rc::downgrade(pane);
    let field = input.clone();
    let changed = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
        let Some(file) = field.files().and_then(|files| files.item(0)) else {
            return;
        };
        let weak = weak.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = if file.size() > scenomise::backdrops::MAX_SCENE_INPUT_BYTES as f64 {
                Err("Recipe exceeds one MiB".into())
            } else {
                wasm_bindgen_futures::JsFuture::from(file.text())
                    .await
                    .map_err(|e| format!("Cannot read recipe: {e:?}"))
                    .and_then(|value| value.as_string().ok_or("Recipe is not text".into()))
            };
            if let Some(pane) = weak.upgrade() {
                let mut pane = pane.borrow_mut();
                if pane.open && pane.import_generation == generation {
                    pane.pending = Some(result);
                }
            }
        });
    }) as Box<dyn FnMut(web_sys::Event)>);
    input.set_onchange(Some(changed.as_ref().unchecked_ref()));
    document
        .body()
        .ok_or("No document body")?
        .append_child(&input)
        .map_err(|e| format!("Cannot attach recipe picker: {e:?}"))?;
    pane.borrow_mut().import = Some(ImportPicker {
        input: input.clone(),
        _changed: changed,
    });
    input.click();
    Ok(())
}

struct SpanDrag {
    baseline: SceneInput,
    intent: SpanEditIntent,
    edit: SpanEdit,
    x: f32,
    ticks_per_pixel: f32,
    size: (f32, f32),
    pointer: i32,
}
pub(super) struct Pane {
    pub open: bool,
    input: String,
    import: Option<ImportPicker>,
    pending: Option<Result<String, String>>,
    import_generation: u64,
    surface: Option<BackdropSurface>,
    document: Option<SceneDocument>,
    related: Option<BackdropSurface>,
    related_key: Option<String>,
    sample: Option<usize>,
    error: String,
    revision: u64,
    export_url: Option<String>,
    original: Option<SceneDocument>,
    drag: Option<SpanDrag>,
}
impl Default for Pane {
    fn default() -> Self {
        Self {
            open: false,
            input: SAMPLES[0].1.into(),
            import: None,
            pending: None,
            import_generation: 0,
            surface: None,
            document: None,
            related: None,
            related_key: None,
            sample: Some(0),
            error: String::new(),
            revision: 0,
            export_url: None,
            original: None,
            drag: None,
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
impl Pane {
    fn install(&mut self, json: &str) -> Result<(), String> {
        let document = SceneDocument::parse(json)?;
        let input = &document.primary;
        if let Some(surface) = self.surface.as_mut() {
            surface.replace(input).map_err(|e| e.to_string())?;
        } else {
            let mut surface = BackdropSurface::new(input).map_err(|e| e.to_string())?;
            surface.register_font(include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec());
            self.surface = Some(surface);
        }
        self.document = Some(document);
        self.original = None;
        self.drag = None;
        if let Some(surface) = &mut self.surface {
            surface.show_span_handles = false;
        }
        self.related = None;
        self.related_key = None;
        self.sync_related()?;
        self.input = json.into();
        self.error.clear();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    fn sample(&mut self, index: usize) {
        self.import_generation = self.import_generation.wrapping_add(1);
        self.import = None;
        self.pending = None;
        match self.install(SAMPLES[index].1) {
            Ok(()) => {
                self.sample = Some(index);
                self.open = true;
            },
            Err(error) => self.error = error,
        }
    }
    fn sync_related(&mut self) -> Result<(), String> {
        let selected = self.surface.as_ref().and_then(|s| s.selected());
        let view = self
            .document
            .as_ref()
            .and_then(|d| d.for_occurrence(selected));
        let key = view.map(|v| v.occurrence.clone());
        if key == self.related_key {
            return Ok(());
        }
        self.related = view
            .map(|v| {
                let mut surface = BackdropSurface::new(&v.scene).map_err(|e| e.to_string())?;
                surface.register_font(include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec());
                Ok::<_, String>(surface)
            })
            .transpose()?;
        self.related_key = key;
        Ok(())
    }
    fn edit_copy(&mut self) {
        self.cancel_drag();
        if self.original.is_some() {
            return;
        }
        if let Some(doc) = &mut self.document {
            self.original = Some(doc.clone());
            doc.primary.source = sceno::SourceRef {
                adapter: "graphshell.scene-copy".into(),
                id: format!(
                    "{}:{}@{}",
                    doc.primary.source.adapter, doc.primary.source.id, doc.primary.revision
                ),
            };
            doc.primary.revision = format!("copy-{}", self.revision);
            if let Some(surface) = &mut self.surface {
                let _ = surface.replace(&doc.primary);
                surface.show_span_handles = true;
            }
            self.input = serde_json::to_string(doc).unwrap();
            self.error.clear();
            self.revision = self.revision.wrapping_add(1);
        }
    }
    fn cancel_drag(&mut self) {
        if self.drag.take().is_some() {
            if let (Some(surface), Some(doc)) = (&mut self.surface, &self.document) {
                let _ = surface.replace(&doc.primary);
            }
            self.revision = self.revision.wrapping_add(1);
        }
    }
    fn accept(&mut self, intent: &SpanEditIntent) -> Result<(), String> {
        if self.original.is_none() {
            return Err("Choose Edit a copy before changing a recipe".into());
        }
        let doc = self.document.as_ref().ok_or("No scene")?;
        let facts = intent.proposed_facts(&doc.primary)?;
        if intent.before == intent.after {
            self.surface
                .as_mut()
                .ok_or("No surface")?
                .replace(&doc.primary)
                .map_err(|e| e.to_string())?;
            self.revision = self.revision.wrapping_add(1);
            return Ok(());
        }
        let mut next = doc.clone();
        next.primary.facts = facts;
        next.primary.revision = format!("copy-{}", self.revision);
        next.validate()?;
        self.surface
            .as_mut()
            .ok_or("No surface")?
            .replace(&next.primary)
            .map_err(|e| e.to_string())?;
        self.input = serde_json::to_string(&next).map_err(|e| e.to_string())?;
        self.document = Some(next);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    fn change_span(&mut self, edit: SpanEdit, steps: i32) {
        self.cancel_drag();
        let result = (|| {
            let input = &self.document.as_ref().ok_or("No scene")?.primary;
            let key = self
                .surface
                .as_ref()
                .and_then(|s| s.selected())
                .ok_or("Select a placed span")?;
            let quantum = u32::from(
                input
                    .span_target(key)
                    .ok_or("No source span selected")?
                    .ticks_per_unit,
            );
            SpanEditIntent::propose(input, key, edit, steps as f32 * quantum as f32, quantum)
        })()
        .and_then(|intent| self.accept(&intent));
        if let Err(e) = result {
            self.error = e;
        }
    }
    fn span_key(&mut self, event: cambium::KeyEvent) {
        use cambium::{Key, NamedKey};
        match event.key {
            Key::Named(NamedKey::Escape) if self.drag.is_some() => self.cancel_drag(),
            Key::Named(NamedKey::Enter) if self.drag.is_some() => {
                if let Some(g) = self.drag.take() {
                    if let Err(e) = self.accept(&g.intent) {
                        self.error = e;
                    }
                }
            },
            Key::Named(k @ (NamedKey::ArrowLeft | NamedKey::ArrowRight))
                if self.original.is_some() && !event.mods.ctrl && !event.mods.meta =>
            {
                self.change_span(
                    if event.mods.alt {
                        SpanEdit::Start
                    } else if event.mods.shift {
                        SpanEdit::End
                    } else {
                        SpanEdit::Move
                    },
                    if k == NamedKey::ArrowLeft { -1 } else { 1 },
                )
            },
            _ => return,
        }
        event.prevent_default();
        event.stop_propagation();
    }
    fn pointer(&mut self, event: cambium::PointerEvent) {
        use cambium::{PointerButton, PointerPhase as Phase};
        if event.button != PointerButton::Primary {
            return;
        }
        if event.phase == Phase::Cancel {
            self.cancel_drag();
            return;
        }
        if !event.local.0.is_finite()
            || !event.local.1.is_finite()
            || !event.size.0.is_finite()
            || !event.size.1.is_finite()
            || event.size.0 <= 0.0
            || event.size.1 <= 0.0
        {
            self.cancel_drag();
            return;
        }
        if event.phase == Phase::Down {
            self.cancel_drag();
            let Some(s) = &self.surface else {
                return;
            };
            let (w, h) = (event.size.0.round() as u32, event.size.1.round() as u32);
            let key = s
                .pick(event.local.0, event.local.1, w, h)
                .map(str::to_owned);
            let geometry = key
                .as_ref()
                .and_then(|k| s.occurrence_rect(k, w, h))
                .and_then(|r| {
                    Some((
                        r,
                        s.scene_point(1.0, 0.0, w, h)?.x - s.scene_point(0.0, 0.0, w, h)?.x,
                    ))
                });
            if let Some(s) = &mut self.surface {
                let _ = s.select(key.as_deref());
            }
            if self.original.is_some() {
                if let (Some(key), Some((r, scale)), Some(doc)) = (key, geometry, &self.document) {
                    if let Some(target) = doc.primary.span_target(&key) {
                        let edge = 8.0_f32.min(r.width / 3.0);
                        let edit = if event.local.0 <= r.x + edge {
                            SpanEdit::Start
                        } else if event.local.0 >= r.x + r.width - edge {
                            SpanEdit::End
                        } else {
                            SpanEdit::Move
                        };
                        if let Ok(intent) = SpanEditIntent::propose(
                            &doc.primary,
                            &key,
                            edit,
                            0.0,
                            u32::from(target.ticks_per_unit),
                        ) {
                            self.drag = Some(SpanDrag {
                                baseline: doc.primary.clone(),
                                intent,
                                edit,
                                x: event.local.0,
                                ticks_per_pixel: scale * f32::from(target.ticks_per_unit) / 47.0,
                                size: event.size,
                                pointer: event.pointer_id,
                            });
                        }
                    }
                }
            }
            let _ = self.sync_related();
            self.revision = self.revision.wrapping_add(1);
        } else if let Some(mut g) = self.drag.take() {
            if event.pointer_id != g.pointer {
                self.drag = Some(g);
                return;
            }
            if event.size == g.size
                && self
                    .document
                    .as_ref()
                    .is_some_and(|d| d.primary == g.baseline)
            {
                let quantum = u32::from(
                    g.baseline
                        .span_target(&g.intent.occurrence)
                        .unwrap()
                        .ticks_per_unit,
                );
                let dx = event.local.0 - g.x;
                match SpanEditIntent::propose(
                    &g.baseline,
                    &g.intent.occurrence,
                    g.edit,
                    if dx.abs() < 3.0 {
                        0.0
                    } else {
                        dx * g.ticks_per_pixel
                    },
                    quantum,
                ) {
                    Ok(intent) => {
                        g.intent = intent;
                        if event.phase == Phase::Up {
                            if let Err(e) = self.accept(&g.intent) {
                                self.error = e;
                            }
                        } else {
                            let mut preview = g.baseline.clone();
                            preview.facts = g.intent.proposed_facts(&g.baseline).unwrap();
                            preview.revision = format!("preview-{}", self.revision);
                            let _ = self.surface.as_mut().unwrap().replace(&preview);
                            self.drag = Some(g);
                        }
                    },
                    Err(e) => self.error = e,
                }
            } else if let (Some(s), Some(d)) = (&mut self.surface, &self.document) {
                let _ = s.replace(&d.primary);
            }
            self.revision = self.revision.wrapping_add(1);
        }
        event.prop.prevent_default();
    }
    fn event(&mut self, event: BackdropSceneEvent) {
        self.cancel_drag();
        if let Some(surface) = self.surface.as_mut() {
            let result = match event {
                BackdropSceneEvent::Select(key) => surface.select(Some(&key)),
                BackdropSceneEvent::Clear => surface.select(None),
                BackdropSceneEvent::DismissRetained => {
                    surface.dismiss_retained();
                    Ok(())
                },
            };
            if let Err(error) = result {
                self.error = error;
            }
            if let Err(error) = self.sync_related() {
                self.error = error;
            }
            self.revision = self.revision.wrapping_add(1);
        }
    }
}
pub(super) fn control(page: &TreePage) -> Child {
    let open = page.backdrop.borrow().open;
    Box::new(
        button(
            if open {
                "Close portable scene"
            } else {
                "Portable scenes"
            },
            |page: &mut TreePage, _| {
                let mut pane = page.backdrop.borrow_mut();
                if pane.surface.is_none() {
                    pane.sample(0);
                } else {
                    pane.open = !pane.open;
                    if !pane.open {
                        pane.cancel_drag();
                        pane.import_generation = pane.import_generation.wrapping_add(1);
                        pane.import = None;
                        pane.pending = None;
                        if let Some(url) = pane.export_url.take() {
                            let _ = web_sys::Url::revoke_object_url(&url);
                        }
                    }
                }
            },
        )
        .attr("aria-expanded", open.to_string()),
    )
}
fn export_recipe(json: &str) -> Result<String, String> {
    let input = SceneDocument::parse(json)?;
    let bare = serde_json::from_str::<serde_json::Value>(json)
        .map_err(|e| e.to_string())?
        .get("format")
        .is_none();
    let bytes = if bare {
        serde_json::to_vec_pretty(&input.primary)
    } else {
        serde_json::to_vec_pretty(&input)
    }
    .map_err(|e| e.to_string())?;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("No browser document")?;
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes.as_slice()));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("Cannot prepare recipe export: {e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Cannot create recipe URL: {e:?}"))?;
    let prepare = || -> Result<(), String> {
        let anchor: web_sys::HtmlElement = document
            .create_element("a")
            .map_err(|e| format!("Cannot create download: {e:?}"))?
            .dyn_into()
            .map_err(|_| "Download element unavailable")?;
        anchor
            .set_attribute("href", &url)
            .map_err(|e| format!("Cannot set download URL: {e:?}"))?;
        anchor
            .set_attribute("download", "scene-recipe.json")
            .map_err(|e| format!("Cannot name recipe: {e:?}"))?;
        document
            .body()
            .ok_or("No document body")?
            .append_child(&anchor)
            .map_err(|e| format!("Cannot attach download: {e:?}"))?;
        anchor.click();
        anchor.remove();
        Ok(())
    };
    if let Err(error) = prepare() {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(error);
    }
    Ok(url)
}
pub(super) fn workspace(page: &TreePage) -> Option<Child> {
    if page.product.is_none() || !page.backdrop.borrow().open {
        return None;
    }
    let pane = page.backdrop.borrow();
    let mut children: Vec<Child> = Vec::new();
    let mut bar: Vec<Child> = vec![Box::new(el("span", "Graphshell / portable scene"))];
    for (index, (label, _)) in SAMPLES.iter().enumerate() {
        bar.push(Box::new(
            button(*label, move |page: &mut TreePage, _| {
                page.backdrop.borrow_mut().sample(index);
            })
            .attr("aria-pressed", (pane.sample == Some(index)).to_string()),
        ));
    }
    for (label, coverage) in [("Coverage", true), ("Links", false)] {
        if coverage
            && !pane
                .surface
                .as_ref()
                .is_some_and(|s| !s.scene().fields.is_empty())
        {
            continue;
        }
        let enabled = pane.surface.as_ref().is_some_and(|s| {
            if coverage {
                s.show_coverage
            } else {
                s.show_links
            }
        });
        bar.push(Box::new(
            button(label, move |page: &mut TreePage, _| {
                let mut pane = page.backdrop.borrow_mut();
                if let Some(s) = pane.surface.as_mut() {
                    if coverage {
                        s.show_coverage = !s.show_coverage;
                    } else {
                        s.show_links = !s.show_links;
                    }
                }
                pane.revision = pane.revision.wrapping_add(1);
            })
            .attr("aria-pressed", enabled.to_string()),
        ));
    }
    if pane.original.is_none() {
        if pane.document.as_ref().is_some_and(|d| {
            d.primary.compile().ok().is_some_and(|s| {
                s.marks
                    .iter()
                    .any(|m| d.primary.span_target(&m.key).is_some())
            })
        }) {
            bar.push(Box::new(
                button("Edit a copy", |page: &mut TreePage, _| {
                    page.backdrop.borrow_mut().edit_copy()
                })
                .attr("class", "backdrop-edit-copy"),
            ));
        }
    } else {
        bar.push(Box::new(el(
            "span",
            "Editing a local recipe copy · original source unchanged",
        )));
        bar.push(Box::new(
            button("Discard copy", |page: &mut TreePage, _| {
                let mut pane = page.backdrop.borrow_mut();
                pane.cancel_drag();
                if let Some(original) = pane.original.take() {
                    let json = serde_json::to_string(&original).unwrap();
                    if let Err(e) = pane.install(&json) {
                        pane.error = e;
                    }
                }
            })
            .attr("class", "backdrop-discard-copy"),
        ));
    }
    bar.push(Box::new(button(
        "Import scene recipe",
        |page: &mut TreePage, _| {
            if let Err(error) = begin_import(&page.backdrop) {
                page.backdrop.borrow_mut().error = error;
            }
        },
    )));
    bar.push(Box::new(button(
        "Export scene recipe",
        |page: &mut TreePage, _| {
            let mut pane = page.backdrop.borrow_mut();
            match export_recipe(&pane.input) {
                Ok(url) => {
                    if let Some(old) = pane.export_url.replace(url) {
                        let _ = web_sys::Url::revoke_object_url(&old);
                    }
                },
                Err(error) => pane.error = error,
            }
        },
    )));
    children.push(Box::new(el("div", bar).attr("class", "backdrop-scene-bar")));
    // Keep the stage child stable while notices change during a gesture.
    children.push(Box::new(el("p", pane.error.clone()).attr("role", "alert")));
    if let Some(surface) = &pane.surface {
        children.push(Box::new(el("h2", surface.scene().title.clone())));
        // Keep structural labels legible. Narrow hosts scroll the stage rather
        // than compressing nested groups into overlapping text.
        let width = page.size.0.saturating_sub(590).max(680);
        let bounds = surface.scene().scene.bounds.size;
        let height = ((width.saturating_sub(48) as f32 * bounds.h / bounds.w) + 48.0)
            .ceil()
            .clamp(
                if pane.document.as_ref().is_some_and(|d| !d.linked.is_empty()) {
                    220.0
                } else {
                    440.0
                },
                1000.0,
            ) as u32;
        let stage = on_key(on_pointer(
            custom_leaf::<TreePage, ()>(KEY,width,height).attr("class","backdrop-span-stage").attr("aria-label","Portable scene stage. Arrows move; Shift changes end; Alt changes start; Escape cancels a copy edit."),
            |page:&mut TreePage,event|page.backdrop.borrow_mut().pointer(event),
        ),|page:&mut TreePage,event|page.backdrop.borrow_mut().span_key(event));
        let reading = backdrop_scene_reading(
            surface.scene(),
            surface.selected(),
            surface.retained(),
            |page: &mut TreePage, event| page.backdrop.borrow_mut().event(event),
        );
        let mut stages: Vec<Child> = vec![Box::new(
            el("div", stage).attr("class", "backdrop-scene-stage"),
        )];
        if pane.original.is_some() {
            if let Some(key) = surface.selected() {
                if let Some(target) = pane
                    .document
                    .as_ref()
                    .and_then(|d| d.primary.span_target(key))
                {
                    let span = pane
                        .drag
                        .as_ref()
                        .map(|g| g.intent.after)
                        .unwrap_or(target.span);
                    stages.push(Box::new(
                        el(
                            "p",
                            format!(
                                "{key} · start {} / length {} ticks{}",
                                span.start,
                                span.length,
                                if pane.drag.is_some() {
                                    " · preview"
                                } else {
                                    ""
                                }
                            ),
                        )
                        .attr("class", "backdrop-span-status")
                        .attr("role", "status"),
                    ));
                    let mut edits: Vec<Child> = vec![];
                    for (edit, label) in [
                        (SpanEdit::Move, "Move"),
                        (SpanEdit::Start, "Start"),
                        (SpanEdit::End, "End"),
                    ] {
                        for (steps, dir) in [(-1, "earlier"), (1, "later")] {
                            edits.push(Box::new(
                                button(
                                    format!("{label} {dir} one unit"),
                                    move |page: &mut TreePage, _| {
                                        page.backdrop.borrow_mut().change_span(edit, steps)
                                    },
                                )
                                .attr("class", "backdrop-span-edit"),
                            ));
                        }
                    }
                    stages.push(Box::new(on_key(
                        el("div", edits).attr("class", "backdrop-scene-bar"),
                        |page: &mut TreePage, event| page.backdrop.borrow_mut().span_key(event),
                    )));
                }
            }
            stages.push(Box::new(el("p","Drag spans or their edges. Arrows move one unit; Shift changes end; Alt changes start; Escape cancels. Export retains this local copy only.")));
        }
        if let Some(related) = &pane.related {
            let view = pane
                .document
                .as_ref()
                .and_then(|d| d.for_occurrence(surface.selected()))
                .unwrap();
            stages.push(Box::new(el("h3", view.label.clone())));
            stages.push(Box::new(
                el("p", related.scene().title.clone()).attr("class", "tools-caption"),
            ));
            stages.push(Box::new(
                el(
                    "div",
                    custom_leaf::<TreePage, ()>(RELATED_KEY, width, 300)
                        .attr("aria-label", "Related scene stage"),
                )
                .attr("class", "backdrop-scene-stage"),
            ));
            let labels = related
                .scene()
                .marks
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>()
                .join(" · ");
            stages.push(Box::new(
                el(
                    "p",
                    format!(
                        "Supplied marks: {labels} · revision {}",
                        related.scene().revision
                    ),
                )
                .attr("class", "tools-caption"),
            ));
        } else if pane.document.as_ref().is_some_and(|d| !d.linked.is_empty()) {
            stages.push(Box::new(el("p","No related scene is supplied for this selection. Choose an occurrence with a source-supplied reading.")));
        }
        children.push(Box::new(
            el(
                "div",
                (
                    el("div", stages).attr("class", "backdrop-scene-stages"),
                    reading,
                ),
            )
            .attr("class", "backdrop-scene-body"),
        ));
        children.push(Box::new(
            el(
                "p",
                format!(
                    "Source revision: {} · supplied facts; selection does not edit the source",
                    surface.scene().revision
                ),
            )
            .attr("class", "tools-caption"),
        ));
    }
    Some(Box::new(
        el("section", children)
            .attr("class", "backdrop-scene-workspace")
            .attr("aria-label", "Portable scene workspace"),
    ))
}

struct Producer {
    related: bool,
    pane: Rc<RefCell<Pane>>,
    appearance: crate::web_appearance::Handle,
    texture: Option<wgpu::Texture>,
    last: Option<cambium_rootstock::ProducedScene>,
    last_view: u64,
    last_appearance: u64,
    last_size: (u32, u32),
    generation: u64,
}
impl Producer {
    fn produce(
        &mut self,
        frame: &cambium_rootstock::ProducerFrameInfo,
    ) -> Result<cambium_rootstock::ProducedScene, cambium_rootstock::PortableProducerError> {
        use cambium_rootstock::{PortableProducerError, ProducedScene, ProducerFramePurpose};
        let size = (
            frame.logical_size.0.round().max(1.0) as u32,
            frame.logical_size.1.round().max(1.0) as u32,
        );
        let mut pane = self.pane.borrow_mut();
        if frame.purpose == ProducerFramePurpose::Export {
            if self.last_view != pane.revision
                || size != self.last_size
                || self.last_appearance != self.appearance.borrow().revision
            {
                return Err(PortableProducerError::InvalidSource);
            }
            return self
                .last
                .clone()
                .ok_or(PortableProducerError::InvalidSource);
        }
        let appearance = self.appearance.borrow();
        let theme = appearance.backdrop_theme();
        let font_roles = appearance.font_roles();
        let appearance_revision = appearance.revision;
        drop(appearance);
        let active = if self.related {
            pane.related.as_mut()
        } else {
            pane.surface.as_mut()
        };
        let active = active.ok_or(PortableProducerError::InvalidSource)?;
        active.set_font_roles(&font_roles);
        let scene = active.paint(size.0, size.1, &theme).map_err(|error| {
            pane.error = error;
            PortableProducerError::InvalidSource
        })?;
        self.generation = self.generation.wrapping_add(1);
        let result = ProducedScene {
            scene,
            source_identity: (if self.related {
                pane.related.as_ref()
            } else {
                pane.surface.as_ref()
            })
            .map(|surface| {
                let source = &surface.scene().scene.sources[0];
                source
                    .adapter
                    .bytes()
                    .chain([0])
                    .chain(source.id.bytes())
                    .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
                    })
            })
            .unwrap_or(KEY),
            source_revision: self.generation,
            alpha: SourceAlpha::Premultiplied,
            encoding: SourceEncoding::Srgb,
        };
        self.last_view = pane.revision;
        self.last_appearance = appearance_revision;
        self.last_size = size;
        self.last = Some(result.clone());
        Ok(result)
    }
}
impl TextureProducer for Producer {
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        let output = self.produce(cx.frame).ok()?;
        let [w, h] = cx.frame.physical_size;
        let (texture, view) = cx.core.rasterize_scaled_for(
            if self.related { RELATED_KEY } else { KEY },
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
    let pending = ctx.runner.state().backdrop.borrow_mut().pending.take();
    if let Some(pending) = pending {
        ctx.runner.update(|page| {
            let mut pane = page.backdrop.borrow_mut();
            let result = pending.and_then(|json| pane.install(&json));
            match result {
                Ok(()) => pane.sample = None,
                Err(error) => pane.error = error,
            }
        });
    }
    if !ctx.runner.state().backdrop.borrow().open {
        ctx.producers.remove(KEY);
        ctx.producers.remove(RELATED_KEY);
        return;
    }
    for related in [false, true] {
        let key = if related { RELATED_KEY } else { KEY };
        if related && ctx.runner.state().backdrop.borrow().related.is_none() {
            ctx.producers.remove(key);
            continue;
        }
        if !ctx.producers.contains(key) {
            let producer = Producer {
                related,
                pane: ctx.runner.state().backdrop.clone(),
                appearance: ctx.runner.state().shared.appearance.clone(),
                texture: None,
                last: None,
                last_view: 0,
                last_appearance: 0,
                last_size: (0, 0),
                generation: 0,
            };
            if let Err(error) = ctx.producers.register(key, producer, &[]) {
                web_sys::console::error_1(&format!("portable scene producer: {error:?}").into());
            }
        }
    }
}
