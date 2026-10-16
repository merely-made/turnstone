// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Arrangement declarations supply both the shared controls and their refusals.
use super::*;
use cambium::select;
use graphshell::projection_compile::{PRACTICE_CARD, option_placeholder, practice_compiler};
use scenograph::options::{OptionKind, OptionSpec};
use scenomise::arrangement::{arrangement_defaults, arrangement_options, with_option};

pub(in crate::web_tree::product) struct OptionRow {
    pub(in crate::web_tree::product) spec: OptionSpec,
    pub(in crate::web_tree::product) input: TextInput,
    choice: SelectState,
    names: Vec<String>,
    pub(super) previous: String,
    placeholder: String,
}
impl OptionRow {
    pub(super) fn value(&self) -> &str {
        if self.names.is_empty() {
            self.input.text()
        } else {
            self.names
                .get(self.choice.selected)
                .map(String::as_str)
                .unwrap_or("")
        }
    }
}
impl ProjectionControls {
    pub(super) fn sync_options(&mut self) {
        let draft = self.editor.draft();
        self.options_kind = draft.arrangement.kind.clone();
        let defaults = self
            .dataset
            .as_ref()
            .map(|dataset| {
                arrangement_defaults(
                    &draft.arrangement.kind,
                    PRACTICE_CARD,
                    dataset.occurrences.len(),
                    draft.arrangement.spacing as f32,
                )
            })
            .unwrap_or_default();
        self.options = arrangement_options(&draft.arrangement.kind, practice_compiler().registry())
            .unwrap_or_default()
            .into_iter()
            .map(|spec| {
                let previous = draft
                    .arrangement
                    .options
                    .get(&spec.key)
                    .cloned()
                    .unwrap_or_default();
                let names = match &spec.kind {
                    OptionKind::Choice { names } => std::iter::once(String::new())
                        .chain(names.iter().cloned())
                        .collect(),
                    OptionKind::Flag => vec![String::new(), "true".into(), "false".into()],
                    _ => Vec::new(),
                };
                let choice =
                    SelectState::new(names.iter().position(|name| name == &previous).unwrap_or(0))
                        .with_label(&spec.label);
                let placeholder =
                    option_placeholder(&spec, defaults.get(&spec.key).map(String::as_str));
                OptionRow {
                    input: TextInput::new(&previous),
                    spec,
                    choice,
                    names,
                    previous,
                    placeholder,
                }
            })
            .collect();
    }
    pub(super) fn apply_options(&mut self) {
        if self.options_kind != self.editor.draft().arrangement.kind {
            self.sync_options();
            return;
        }
        for row in &mut self.options {
            let value = row.value().to_owned();
            if value == row.previous {
                continue;
            }
            row.previous = value.clone();
            self.editor.reduce(
                EditorAction::SetArrangement(with_option(
                    &self.editor.draft().arrangement,
                    &row.spec.key,
                    &value,
                )),
                js_sys::Date::now() as u64,
            );
            self.status = format!("Edited · arrangement.options.{}", row.spec.key);
        }
    }
}
pub(super) fn rows(p: &ProjectionControls) -> Vec<Child> {
    if p.options.is_empty() {
        let note = if arrangement_options(&p.options_kind, practice_compiler().registry()).is_some()
        {
            "This arrangement takes no options".into()
        } else {
            format!(
                "{} names no built-in arrangement or registered solver",
                p.options_kind
            )
        };
        return vec![Box::new(el("p", note))];
    }
    p.options
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let refusal = (!row.value().is_empty())
                .then(|| row.spec.kind.refusal(row.value()))
                .flatten();
            let invalid = refusal.is_some().to_string();
            let control: Child = if row.names.is_empty() {
                let label = row.spec.label.clone();
                let placeholder = row.placeholder.clone();
                let key = row.spec.key.clone();
                Box::new(lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input)
                            .attr("aria-label", label.clone())
                            .attr("placeholder", placeholder.clone())
                            .attr("aria-invalid", invalid.clone())
                            .attr("data-projection-option", key.clone())
                    },
                    move |page: &mut TreePage| {
                        &mut page
                            .product
                            .as_mut()
                            .expect("projection options")
                            .projection
                            .options[index]
                            .input
                    },
                ))
            } else {
                let mut names = row.names.clone();
                names[0] = format!("Default ({})", row.placeholder);
                Box::new(lens(
                    move |state: &mut SelectState| {
                        select(state, &names.iter().map(String::as_str).collect::<Vec<_>>())
                    },
                    move |page: &mut TreePage| {
                        &mut page
                            .product
                            .as_mut()
                            .expect("projection options")
                            .projection
                            .options[index]
                            .choice
                    },
                ))
            };
            Box::new(el(
                "label",
                vec![
                    Box::new(el("span", row.spec.label.clone())) as Child,
                    control,
                    Box::new(
                        el("p", refusal.unwrap_or_default())
                            .attr("aria-live", "polite")
                            .attr("role", "status")
                            .attr("aria-label", format!("{} refusal", row.spec.label)),
                    ),
                ],
            )) as Child
        })
        .collect()
}
