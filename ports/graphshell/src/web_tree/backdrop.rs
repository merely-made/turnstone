// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's reference embedding. The recipes, paint and semantic reading
//! are shared; this module owns only admission, view state and host effects.
use super::*;
use cambium::{BackdropSceneEvent, backdrop_scene_reading, button};
use mere::canvas::backdrop_surface::BackdropSurface;
use scenomise::backdrops::SceneInput;

const KEY: u64 = 0x6261_636b;
const SAMPLES: [(&str, &str); 3] = [
    (
        "Metre grid",
        include_str!("../../../../support/design-studies/backdrops/metre.json"),
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

pub(super) struct Pane {
    pub open: bool,
    input: String,
    import: Option<ImportPicker>,
    pending: Option<Result<String, String>>,
    import_generation: u64,
    surface: Option<BackdropSurface>,
    sample: Option<usize>,
    error: String,
    revision: u64,
    export_url: Option<String>,
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
            sample: Some(0),
            error: String::new(),
            revision: 0,
            export_url: None,
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
        let input = SceneInput::parse(json).map_err(|e| e.to_string())?;
        if let Some(surface) = self.surface.as_mut() {
            surface.replace(&input).map_err(|e| e.to_string())?;
        } else {
            let mut surface = BackdropSurface::new(&input).map_err(|e| e.to_string())?;
            surface.register_font(include_bytes!("../../web/GraphshellSans.ttf").to_vec());
            self.surface = Some(surface);
        }
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
    fn event(&mut self, event: BackdropSceneEvent) {
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
    let input = SceneInput::parse(json).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&input).map_err(|e| e.to_string())?;
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
    if !pane.error.is_empty() {
        children.push(Box::new(el("p", pane.error.clone()).attr("role", "alert")));
    }
    if let Some(surface) = &pane.surface {
        children.push(Box::new(el("h2", surface.scene().title.clone())));
        let width = page.size.0.saturating_sub(590).max(280);
        let height = 440;
        let stage = on_pointer(
            custom_leaf::<TreePage, ()>(KEY, width, height)
                .attr("aria-label", "Portable scene stage"),
            |page: &mut TreePage, event: cambium::PointerEvent| {
                if event.phase == cambium::PointerPhase::Up
                    && event.button == cambium::PointerButton::Primary
                {
                    let mut pane = page.backdrop.borrow_mut();
                    if let Some(surface) = pane.surface.as_mut() {
                        let key = surface
                            .pick(
                                event.local.0,
                                event.local.1,
                                event.size.0.round() as u32,
                                event.size.1.round() as u32,
                            )
                            .map(str::to_owned);
                        let _ = surface.select(key.as_deref());
                        pane.revision = pane.revision.wrapping_add(1);
                    }
                }
            },
        );
        let reading = backdrop_scene_reading(
            surface.scene(),
            surface.selected(),
            surface.retained(),
            |page: &mut TreePage, event| page.backdrop.borrow_mut().event(event),
        );
        children.push(Box::new(
            el("div", (stage, reading)).attr("class", "backdrop-scene-body"),
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
        let appearance_revision = appearance.revision;
        drop(appearance);
        let scene = pane
            .surface
            .as_mut()
            .ok_or(PortableProducerError::InvalidSource)?
            .paint(size.0, size.1, &theme)
            .map_err(|error| {
                pane.error = error;
                PortableProducerError::InvalidSource
            })?;
        self.generation = self.generation.wrapping_add(1);
        let result = ProducedScene {
            scene,
            source_identity: pane
                .surface
                .as_ref()
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
        return;
    }
    if !ctx.producers.contains(KEY) {
        let producer = Producer {
            pane: ctx.runner.state().backdrop.clone(),
            appearance: ctx.runner.state().shared.appearance.clone(),
            texture: None,
            last: None,
            last_view: 0,
            last_appearance: 0,
            last_size: (0, 0),
            generation: 0,
        };
        if let Err(error) = ctx.producers.register(KEY, producer, &[]) {
            web_sys::console::error_1(&format!("portable scene producer: {error:?}").into());
        }
    }
}
