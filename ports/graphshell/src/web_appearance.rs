// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Browser host adapter. CSS is evaluated by the browser in an isolated
//! document; Graphshell uses the resulting roles for semantic controls and
//! Canvas paint. The original sheet also reaches the existing Genet renderer.

use graphshell::appearance::{APPEARANCE_STORAGE_KEY, Appearance, Mode, ThemeChoice};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    CanvasRenderingContext2d, Element, HtmlCanvasElement, HtmlElement, HtmlIFrameElement,
    HtmlSelectElement, HtmlTextAreaElement,
};

pub(crate) type Handle = Rc<RefCell<Session>>;
const ROLES: &[&str] = &[
    "bg",
    "surface",
    "surface-2",
    "text",
    "text-dim",
    "primary",
    "secondary",
    "tertiary",
    "on-primary",
    "on-tertiary",
    "success",
    "danger",
];
const PAINT: &[&str] = &[
    "background",
    "edge",
    "text",
    "surface",
    "primary",
    "secondary",
    "tertiary",
    "on-primary",
    "success",
    "danger",
];

pub(crate) struct Session {
    root: Element,
    appearance: Appearance,
    owned: bool,
    probe: HtmlIFrameElement,
    inherited: String,
    sheet: String,
    colors: Vec<[f32; 4]>,
    pub revision: u64,
    status: String,
}

fn error(value: impl Into<JsValue>) -> String {
    format!("Browser appearance: {:?}", value.into())
}

pub(crate) fn mount(root: Element, owned: bool) -> Result<Handle, String> {
    let mut status = if owned {
        "Built-in appearance"
    } else {
        "Composing host appearance"
    }
    .to_string();
    let appearance = if owned {
        match crate::window()?
            .local_storage()
            .map_err(error)
            .and_then(|store| store.ok_or_else(|| "Browser appearance storage unavailable".into()))
            .and_then(|store| store.get_item(APPEARANCE_STORAGE_KEY).map_err(error))
        {
            Ok(Some(json)) => match Appearance::from_json(&json) {
                Ok(value) => {
                    status = "Saved appearance reopened".into();
                    value
                },
                Err(message) => {
                    status = message;
                    Appearance::default()
                },
            },
            Ok(None) => Appearance::default(),
            Err(message) => {
                status = message;
                Appearance::default()
            },
        }
    } else {
        Appearance::default()
    };
    let document = crate::document()?;
    let probe: HtmlIFrameElement = document
        .create_element("iframe")
        .map_err(error)?
        .dyn_into()
        .map_err(error)?;
    probe.set_attribute("aria-hidden", "true").map_err(error)?;
    probe.set_attribute("tabindex", "-1").map_err(error)?;
    probe
        .set_attribute("title", "Appearance color evaluation")
        .map_err(error)?;
    probe
        .set_attribute(
            "style",
            "position:absolute;width:1px;height:1px;opacity:0;pointer-events:none;border:0",
        )
        .map_err(error)?;
    root.append_child(&probe).map_err(error)?;
    let handle = Rc::new(RefCell::new(Session {
        root,
        appearance,
        owned,
        probe,
        inherited: String::new(),
        sheet: String::new(),
        colors: Vec::new(),
        revision: 0,
        status,
    }));
    handle.borrow_mut().refresh(true)?;
    if owned {
        install_controls(&handle)?;
    }
    Ok(handle)
}

impl Session {
    pub fn sheet(&self) -> &str {
        &self.sheet
    }
    pub fn apply_canvas(&self, canvas: &mut mere::canvas::Canvas) {
        if self.colors.len() >= 2 {
            canvas.set_palette(self.colors[0], self.colors[1]);
        }
        if self.colors.len() == PAINT.len() {
            let bytes =
                |index: usize| self.colors[index].map(|channel| (channel * 255.0).round() as u8);
            canvas.set_derived_face_palette(self.appearance.canvas_face_palette([
                bytes(4),
                bytes(5),
                bytes(6),
                bytes(8),
                bytes(9),
                bytes(2),
            ]));
        }
    }
    /// Embedded instances observe only inherited shared roles, never storage.
    pub fn refresh(&mut self, force: bool) -> Result<bool, String> {
        let mut inherited = String::new();
        if !self.owned {
            if let Some(style) = crate::window()?
                .get_computed_style(&self.root)
                .map_err(error)?
            {
                for role in ROLES {
                    let name = format!("--tabard-color-{role}");
                    let value = style.get_property_value(&name).map_err(error)?;
                    if !value.trim().is_empty() {
                        inherited.push_str(&format!("{name}:{value};"));
                    }
                }
                for name in ["--gs-canvas-background", "--gs-canvas-edge"] {
                    let value = style.get_property_value(name).map_err(error)?;
                    if !value.trim().is_empty() {
                        inherited.push_str(&format!("{name}:{value};"));
                    }
                }
            }
        }
        if !force && inherited == self.inherited {
            return Ok(false);
        }
        let mut sheet = self.appearance.stylesheet();
        if !self.owned {
            sheet.push_str(&format!("\n:root {{ {inherited} }}"));
        }
        let (colors, values) = self.evaluate(&sheet)?;
        self.inherited = inherited;
        self.sheet = sheet;
        self.colors = colors;
        self.publish(&values)?;
        Ok(true)
    }

    fn evaluate(
        &self,
        sheet: &str,
    ) -> Result<(Vec<[f32; 4]>, Vec<(&'static str, String)>), String> {
        let document = self
            .probe
            .content_document()
            .ok_or("Appearance color document unavailable")?;
        let body = document
            .body()
            .ok_or("Appearance color document body unavailable")?;
        body.set_inner_html("");
        let style = document.create_element("style").map_err(error)?;
        style.set_text_content(Some(sheet));
        body.append_child(&style).map_err(error)?;
        // Keep typed canvas/context casts in the application's realm. The
        // probe's elements belong to its own Window and fail outer-realm
        // instanceof checks; only its computed CSS values cross this seam.
        let sampler: HtmlCanvasElement = crate::document()?
            .create_element("canvas")
            .map_err(error)?
            .dyn_into()
            .map_err(error)?;
        sampler.set_width(1);
        sampler.set_height(1);
        let context: CanvasRenderingContext2d = sampler
            .get_context("2d")
            .map_err(error)?
            .ok_or("Appearance color sampler unavailable")?
            .dyn_into()
            .map_err(error)?;
        let mut colors = Vec::new();
        let mut values = Vec::new();
        for role in PAINT {
            let node = document.create_element("div").map_err(error)?;
            node.set_attribute("data-graphshell-color-role", role)
                .map_err(error)?;
            body.append_child(&node).map_err(error)?;
            let color = self
                .probe
                .content_window()
                .ok_or("Appearance probe window unavailable")?
                .get_computed_style(&node)
                .map_err(error)?
                .ok_or("Appearance computed style unavailable")?
                .get_property_value("background-color")
                .map_err(error)?;
            context.clear_rect(0.0, 0.0, 1.0, 1.0);
            context.set_fill_style_str(&color);
            context.fill_rect(0.0, 0.0, 1.0, 1.0);
            let data = context
                .get_image_data(0.0, 0.0, 1.0, 1.0)
                .map_err(error)?
                .data();
            // An opaque sample avoids losing straight RGB to canvas's
            // premultiplication, especially for an alpha-zero authored role.
            // Native relative-color evaluation also handles non-RGB CSS
            // inputs. Older engines fall back to their original sample.
            context.clear_rect(0.0, 0.0, 1.0, 1.0);
            context.set_fill_style_str("transparent");
            context.set_fill_style_str(&format!("rgb(from {color} r g b / 1)"));
            context.fill_rect(0.0, 0.0, 1.0, 1.0);
            let opaque = context
                .get_image_data(0.0, 0.0, 1.0, 1.0)
                .map_err(error)?
                .data();
            let rgb = if opaque.0[3] == 255 {
                &opaque.0
            } else {
                &data.0
            };
            colors.push([
                rgb[0] as f32 / 255.0,
                rgb[1] as f32 / 255.0,
                rgb[2] as f32 / 255.0,
                data.0[3] as f32 / 255.0,
            ]);
            values.push((*role, color));
        }
        Ok((colors, values))
    }

    fn publish(&mut self, values: &[(&str, String)]) -> Result<(), String> {
        let root: &HtmlElement = self.root.unchecked_ref();
        if self.owned {
            let mode = self
                .appearance
                .resolved()
                .resolved
                .theme_mode
                .as_ref()
                .expect("resolved mode");
            root.style()
                .set_property("color-scheme", if mode.dark() { "dark" } else { "light" })
                .map_err(error)?;
        }
        for (role, color) in values {
            root.style()
                .set_property(&format!("--gs-app-{role}"), color)
                .map_err(error)?;
        }
        self.root
            .set_attribute(
                "data-appearance-owner",
                if self.owned { "application" } else { "host" },
            )
            .map_err(error)?;
        if self.owned {
            self.root
                .set_attribute(
                    "data-appearance-theme",
                    &self.appearance.resolved().resolved.theme_id,
                )
                .map_err(error)?;
            self.root
                .set_attribute(
                    "data-appearance-mode",
                    &self
                        .appearance
                        .resolved()
                        .resolved
                        .theme_mode
                        .as_ref()
                        .expect("resolved mode")
                        .as_key(),
                )
                .map_err(error)?;
            let source = if matches!(
                self.appearance.resolved().presentation,
                tabard::ThemePresentation::AuthoredStylesheet(_)
            ) {
                "authored"
            } else {
                "derived"
            };
            self.root
                .set_attribute("data-appearance-source", source)
                .map_err(error)?;
        } else {
            self.root
                .remove_attribute("data-appearance-theme")
                .map_err(error)?;
            self.root
                .remove_attribute("data-appearance-mode")
                .map_err(error)?;
            self.root
                .set_attribute("data-appearance-source", "host-roles")
                .map_err(error)?;
        }
        self.root
            .set_attribute("data-appearance-background", &values[0].1)
            .map_err(error)?;
        self.revision = self.revision.wrapping_add(1);
        self.report()?;
        Ok(())
    }
    fn report(&self) -> Result<(), String> {
        let diagnostics = self
            .appearance
            .resolved()
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        let status = if diagnostics.is_empty() {
            self.status.clone()
        } else {
            format!("{} · {diagnostics}", self.status)
        };
        self.root
            .set_attribute("data-appearance-status", &status)
            .map_err(error)?;
        if let Some(node) = self
            .root
            .query_selector("[data-appearance-status-text]")
            .map_err(error)?
        {
            node.set_text_content(Some(&status));
        }
        Ok(())
    }
    fn accept(&mut self, candidate: Result<Appearance, String>) -> Result<(), String> {
        let result = candidate.and_then(|candidate| {
            let sheet = candidate.stylesheet();
            // Stage browser evaluation before writing storage or replacing
            // active state. A failed probe never publishes candidate paint.
            let (colors, values) = self.evaluate(&sheet)?;
            self.appearance.accept(candidate, |json| {
                crate::window()?
                    .local_storage()
                    .map_err(error)?
                    .ok_or("Browser appearance storage unavailable")?
                    .set_item(APPEARANCE_STORAGE_KEY, json)
                    .map_err(error)
            })?;
            Ok((sheet, colors, values))
        });
        match result {
            Ok((sheet, colors, values)) => {
                self.status = "Appearance saved".into();
                self.sheet = sheet;
                self.colors = colors;
                self.publish(&values)?;
                self.populate()?;
            },
            Err(message) => {
                self.status = message;
                self.populate()?;
                self.report()?;
            },
        }
        Ok(())
    }
    fn populate(&self) -> Result<(), String> {
        let document = crate::document()?;
        for (selector, options, selected) in [
            (
                "[data-appearance-theme-select]",
                self.appearance.themes(),
                self.appearance.resolved().resolved.theme_id.clone(),
            ),
            (
                "[data-appearance-mode-select]",
                self.appearance
                    .modes()
                    .iter()
                    .map(|mode| (mode.as_key(), mode.label()))
                    .collect(),
                self.appearance
                    .resolved()
                    .resolved
                    .theme_mode
                    .as_ref()
                    .expect("resolved mode")
                    .as_key(),
            ),
        ] {
            let select = self
                .root
                .query_selector(selector)
                .map_err(error)?
                .ok_or("Appearance select unavailable")?;
            select.set_inner_html("");
            for (id, name) in options {
                let option = document.create_element("option").map_err(error)?;
                option.set_attribute("value", &id).map_err(error)?;
                option.set_text_content(Some(&name));
                select.append_child(&option).map_err(error)?;
            }
            select
                .unchecked_ref::<HtmlSelectElement>()
                .set_value(&selected);
        }
        Ok(())
    }
}

fn install_controls(handle: &Handle) -> Result<(), String> {
    let document = crate::document()?;
    let panel = document.create_element("details").map_err(error)?;
    panel
        .set_attribute("class", "graphshell-appearance")
        .map_err(error)?;
    panel.set_inner_html(r#"<summary>Appearance</summary><label>Theme <select data-appearance-theme-select aria-label="Appearance theme"></select></label><label>Mode <select data-appearance-mode-select aria-label="Appearance mode"></select></label><label>Tabard theme JSON <textarea data-appearance-import-json aria-label="Tabard theme JSON" rows="4" spellcheck="false"></textarea></label><button type="button" data-appearance-import>Import theme</button><p data-appearance-status-text role="status"></p>"#);
    handle.borrow().root.append_child(&panel).map_err(error)?;
    handle.borrow().populate()?;
    handle.borrow().report()?;
    // Native form editing belongs to this panel. App canvas shortcuts must
    // not pan, zoom or open a node while a reader edits imported JSON.
    let keyboard = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
        event.stop_propagation();
    });
    panel
        .add_event_listener_with_callback("keydown", keyboard.as_ref().unchecked_ref())
        .map_err(error)?;
    keyboard.forget();
    for selector in [
        "[data-appearance-theme-select]",
        "[data-appearance-mode-select]",
        "[data-appearance-import]",
    ] {
        let node = panel
            .query_selector(selector)
            .map_err(error)?
            .ok_or("Appearance control unavailable")?;
        let weak = Rc::downgrade(handle);
        let importing = selector == "[data-appearance-import]";
        let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let Some(handle) = weak.upgrade() else { return };
            let mut session = handle.borrow_mut();
            let candidate = (|| {
                if importing {
                    let input = session
                        .root
                        .query_selector("[data-appearance-import-json]")
                        .map_err(error)?
                        .ok_or("Import field unavailable")?;
                    session
                        .appearance
                        .with_import(&input.unchecked_ref::<HtmlTextAreaElement>().value())
                } else {
                    let theme = session
                        .root
                        .query_selector("[data-appearance-theme-select]")
                        .map_err(error)?
                        .ok_or("Theme select unavailable")?;
                    let mode = session
                        .root
                        .query_selector("[data-appearance-mode-select]")
                        .map_err(error)?
                        .ok_or("Mode select unavailable")?;
                    session.appearance.with_choice(ThemeChoice::new(
                        theme.unchecked_ref::<HtmlSelectElement>().value(),
                        Mode::from_key(&mode.unchecked_ref::<HtmlSelectElement>().value()),
                    ))
                }
            })();
            if let Err(message) = session.accept(candidate) {
                session.status = message;
                let _ = session.report();
            }
        });
        node.add_event_listener_with_callback(
            if importing { "click" } else { "change" },
            listener.as_ref().unchecked_ref(),
        )
        .map_err(error)?;
        listener.forget();
    }
    Ok(())
}
