// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The projection editor's arrangement rows (Scenograph editor plan, E5):
//! one row per option the arrangement declares, built from the declaration,
//! so the rows and the compiler's refusals cannot drift (ruling B). A row left
//! blank leaves its option out, and its placeholder says what that means.

use graphshell::projection_compile::{PRACTICE_CARD, practice_compiler};
use graphshell::projection_editor::{
    EditorAction, OptionKind, OptionSpec, arrangement_defaults, arrangement_options,
    option_placeholder, with_option,
};
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlInputElement, HtmlSelectElement};

use super::{BrowserHost, document, element, now_ms};

impl BrowserHost {
    /// A row's value changed: set the option, or leave it out when blank.
    pub(super) fn update_projection_option(&mut self, key: &str, value: &str) {
        let arrangement = with_option(&self.projection_editor.draft().arrangement, key, value);
        self.projection_editor
            .reduce(EditorAction::SetArrangement(arrangement), now_ms());
        self.recompile_projection();
        self.projection_editor_status = format!("Edited · arrangement.options.{key}");
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }
}

/// Draw the rows for the draft's arrangement: rebuilt when the arrangement
/// changes, and otherwise brought up to date without disturbing the row
/// being typed in.
pub(super) fn present_option_rows(host: &mut BrowserHost) -> Result<(), String> {
    let container = element("projection-arrangement-options")?;
    let draft = host.projection_editor.draft();
    let kind = draft.arrangement.kind.clone();
    let specs = arrangement_options(&kind, practice_compiler().registry());
    if host.option_rows_kind.as_deref() != Some(kind.as_str()) {
        let document = document()?;
        container.set_inner_html("");
        match &specs {
            None => {
                let note = make(&document, "p")?;
                note.set_text_content(Some(&format!(
                    "{kind} names no built-in arrangement or registered solver"
                )));
                append(&container, &note)?;
            },
            Some(specs) if specs.is_empty() => {
                let note = make(&document, "p")?;
                note.set_text_content(Some("This arrangement takes no options"));
                append(&container, &note)?;
            },
            Some(specs) => {
                // Measured defaults are numbers only when there are items to measure.
                let resolved = host
                    .option_item_count()
                    .map(|count| {
                        arrangement_defaults(
                            &kind,
                            PRACTICE_CARD,
                            count,
                            draft.arrangement.spacing as f32,
                        )
                    })
                    .unwrap_or_default();
                for spec in specs {
                    let placeholder =
                        option_placeholder(spec, resolved.get(&spec.key).map(String::as_str));
                    append(&container, &row(&document, spec, &placeholder)?)?;
                }
            },
        }
        host.option_rows_kind = Some(kind);
    }
    let Some(specs) = specs else {
        return Ok(());
    };
    let focused = document()?.active_element();
    for spec in &specs {
        let value = draft
            .arrangement
            .options
            .get(&spec.key)
            .cloned()
            .unwrap_or_default();
        let id = format!("gs-projection-option-{}", spec.key);
        let Some(control) = container.query_selector(&format!("#{id}")).ok().flatten() else {
            continue;
        };
        if focused.as_ref() != Some(&control) {
            if let Some(input) = control.dyn_ref::<HtmlInputElement>() {
                input.set_value(&value);
            } else if let Some(select) = control.dyn_ref::<HtmlSelectElement>() {
                select.set_value(&value);
            }
        }
        let refusal = (!value.is_empty())
            .then(|| spec.kind.refusal(&value))
            .flatten();
        if let Some(note) = container
            .query_selector(&format!("#{id}-refusal"))
            .ok()
            .flatten()
        {
            note.set_text_content(refusal.as_deref());
        }
        let _ = control.set_attribute(
            "aria-invalid",
            if refusal.is_some() { "true" } else { "false" },
        );
    }
    Ok(())
}

/// One row: its label, a control for its kind, and where a refusal shows.
fn row(document: &Document, spec: &OptionSpec, placeholder: &str) -> Result<Element, String> {
    let row = make(document, "div")?;
    row.set_class_name("option-row");
    let id = format!("gs-projection-option-{}", spec.key);
    let label = make(document, "label")?;
    label.set_attribute("for", &id).ok();
    label.set_text_content(Some(&spec.label));
    append(&row, &label)?;
    let control = match &spec.kind {
        OptionKind::Choice { names } => select(document, placeholder, names)?,
        OptionKind::Flag => select(document, placeholder, &["true".into(), "false".into()])?,
        _ => {
            let input = make(document, "input")?;
            input.set_attribute("placeholder", placeholder).ok();
            input.set_attribute("autocomplete", "off").ok();
            if !matches!(spec.kind, OptionKind::List) {
                input.set_attribute("inputmode", "decimal").ok();
            }
            input
        },
    };
    control.set_id(&id);
    control
        .set_attribute("data-projection-option", &spec.key)
        .ok();
    control
        .set_attribute("aria-describedby", &format!("{id}-refusal"))
        .ok();
    append(&row, &control)?;
    let refusal = make(document, "p")?;
    refusal.set_id(&format!("{id}-refusal"));
    refusal.set_class_name("option-refusal");
    refusal.set_attribute("aria-live", "polite").ok();
    append(&row, &refusal)?;
    Ok(row)
}

/// A choice: the default first, then each name.
fn select(document: &Document, placeholder: &str, names: &[String]) -> Result<Element, String> {
    let select = make(document, "select")?;
    let default = make(document, "option")?;
    default.set_attribute("value", "").ok();
    default.set_text_content(Some(&format!("Default ({placeholder})")));
    append(&select, &default)?;
    for name in names {
        let option = make(document, "option")?;
        option.set_attribute("value", name).ok();
        option.set_text_content(Some(name));
        append(&select, &option)?;
    }
    Ok(select)
}

fn make(document: &Document, tag: &str) -> Result<Element, String> {
    document
        .create_element(tag)
        .map_err(|_| "could not draw the arrangement rows".to_string())
}

fn append(parent: &Element, child: &Element) -> Result<(), String> {
    parent
        .append_child(child)
        .map(|_| ())
        .map_err(|_| "could not draw the arrangement rows".to_string())
}
