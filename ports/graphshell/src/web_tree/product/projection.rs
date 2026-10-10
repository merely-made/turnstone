// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The projection editor and its two views share one typed draft and occurrence selection.
use super::*;
use cambium::{button, el, lens, text_field_typed};
use graphshell::local_projection::{compile_editor, draft_from_definition, snapshot_editor};
use graphshell::projection_compile::{
    CompiledProjection, ProjectionDataset, ProjectionSnapshot, default_definition,
};
use graphshell::projection_editor::{
    Channel, EditorAction, ProjectionEditor, ProjectionPanel, initial_projection_draft,
};
use sceno::InstanceId;
mod comparison;
mod options;
const PRACTICE: &str = include_str!("../../../web/fixtures/woodshed-stage.json");

pub(super) const FIELDS: [(&str, &str, ProjectionPanel); 15] = [
    (
        "source.authority",
        "Source authority",
        ProjectionPanel::Source,
    ),
    ("source.domain", "Source domain", ProjectionPanel::Source),
    (
        "source.resource",
        "Source resource",
        ProjectionPanel::Source,
    ),
    ("reading.key", "Reading key", ProjectionPanel::Reading),
    ("encoding.x", "X field", ProjectionPanel::Encoding),
    ("encoding.y", "Y field", ProjectionPanel::Encoding),
    ("encoding.label", "Label field", ProjectionPanel::Encoding),
    (
        "arrangement.kind",
        "Arrangement kind",
        ProjectionPanel::Arrangement,
    ),
    (
        "arrangement.direction",
        "Arrangement direction",
        ProjectionPanel::Arrangement,
    ),
    (
        "arrangement.spacing",
        "Arrangement spacing",
        ProjectionPanel::Arrangement,
    ),
    (
        "appearance.realization",
        "Realization",
        ProjectionPanel::Preview,
    ),
    (
        "appearance.title",
        "Projection title",
        ProjectionPanel::Preview,
    ),
    ("provenance.author", "Author", ProjectionPanel::Provenance),
    (
        "provenance.source_revision",
        "Source revision",
        ProjectionPanel::Provenance,
    ),
    (
        "provenance.note",
        "Provenance note",
        ProjectionPanel::Provenance,
    ),
];

pub(super) struct ProjectionControls {
    pub(super) open: bool,
    editor: ProjectionEditor,
    pub(super) inputs: Vec<TextInput>,
    values: Vec<String>,
    dataset: Option<ProjectionDataset>,
    compiled: Option<CompiledProjection>,
    selected: Option<String>,
    status: String,
    solves: u64,
    reuses: u64,
    saves: u64,
    pending_save: bool,
    pub(super) options: Vec<options::OptionRow>,
    options_kind: String,
    comparing: bool,
    comparison: Option<graphshell::projection_compare::Comparison>,
}
impl Default for ProjectionControls {
    fn default() -> Self {
        let mut controls = Self {
            open: false,
            editor: ProjectionEditor::new(initial_projection_draft()),
            inputs: Vec::new(),
            values: Vec::new(),
            dataset: None,
            compiled: None,
            selected: None,
            status: String::new(),
            solves: 0,
            reuses: 0,
            saves: 0,
            pending_save: false,
            options: Vec::new(),
            options_kind: String::new(),
            comparing: false,
            comparison: None,
        };
        controls.sync_fields();
        controls
    }
}
impl ProjectionControls {
    fn sync_fields(&mut self) {
        let d = self.editor.draft();
        let channel = |c: &Channel| match c {
            Channel::Field(v) | Channel::Constant(v) => v.clone(),
        };
        self.values = vec![
            d.source.authority.clone(),
            d.source.domain.clone(),
            d.source.resource.clone(),
            d.reading.key.clone(),
            channel(&d.encoding.x),
            channel(&d.encoding.y),
            d.encoding.label.as_ref().map(channel).unwrap_or_default(),
            d.arrangement.kind.clone(),
            d.arrangement.direction.clone(),
            d.arrangement.spacing.to_string(),
            d.appearance.realization.clone(),
            d.appearance.title.clone(),
            d.provenance.author.clone(),
            d.provenance
                .source_revision
                .as_ref()
                .map(|revision| revision.as_str().to_owned())
                .unwrap_or_default(),
            d.provenance.note.clone(),
        ];
        self.inputs = self.values.iter().map(TextInput::new).collect();
        self.sync_options();
    }
    pub(super) fn changed(&self) -> bool {
        self.inputs
            .iter()
            .zip(&self.values)
            .any(|(input, value)| input.text() != value)
            || self.options.iter().any(|row| row.value() != row.previous)
    }
    pub(super) fn apply_fields(&mut self) {
        for (index, input) in self.inputs.iter().enumerate() {
            if input.text() != self.values[index] {
                self.values[index] = input.text().into();
                self.status = match self.editor.edit_field(
                    FIELDS[index].0,
                    input.text(),
                    js_sys::Date::now() as u64,
                ) {
                    Ok(()) => format!("Edited · {}", FIELDS[index].0),
                    Err(error) => error,
                };
            }
        }
        self.apply_options();
        self.refresh();
    }
    fn refresh(&mut self) {
        let Some(dataset) = &self.dataset else { return };
        match compile_editor(
            &self.editor,
            dataset,
            self.compiled.as_ref(),
            self.selected.as_deref(),
        ) {
            Ok(compiled) => {
                if compiled.placement_reused {
                    self.reuses += 1;
                } else {
                    self.solves += 1;
                }
                self.compiled = Some(compiled);
            },
            Err(error) => {
                self.compiled = None;
                self.status = error;
            },
        }
        self.refresh_comparison();
    }
    fn refresh_comparison(&mut self) {
        self.comparison = None;
        if !self.comparing {
            return;
        }
        let Some(dataset) = &self.dataset else {
            return;
        };
        match graphshell::projection_compare::compare_arrangements(
            self.editor.draft(),
            dataset,
            self.selected.as_deref(),
            &graphshell::projection_compare::FacetLayout {
                cell: sceno::Size2::new(220.0, 150.0),
                gap: 16.0,
                heading: 28.0,
            },
        ) {
            Ok(comparison) => self.comparison = Some(comparison),
            Err(error) => self.status = error,
        }
    }
    pub(super) fn saved(&mut self) {
        if self.pending_save {
            self.pending_save = false;
            self.editor.mark_saved();
            self.saves += 1;
            self.status = if self.dataset.is_some() {
                "Saved executable projection".into()
            } else {
                "Saved projection".into()
            };
        }
    }
}
impl SavedProduct {
    pub(in crate::web_tree) fn load_projection_input(
        &mut self,
        json: &str,
        definition: Option<&str>,
    ) {
        let result = (|| {
            let dataset: ProjectionDataset =
                serde_json::from_str(json).map_err(|e| format!("Source load failed: {e}"))?;
            let definition = definition
                .map(serde_json::from_str)
                .transpose()
                .map_err(|e| format!("Recipe load failed: {e}"))?
                .unwrap_or_else(|| default_definition(&dataset));
            let editor = ProjectionEditor::new(draft_from_definition(&definition));
            let compiled = compile_editor(&editor, &dataset, None, None)?;
            Ok::<_, String>((dataset, editor, compiled))
        })();
        match result {
            Ok((dataset, editor, compiled)) => {
                let p = &mut self.projection;
                p.editor = editor;
                p.dataset = Some(dataset);
                p.compiled = Some(compiled);
                p.selected = None;
                p.open = true;
                p.solves += 1;
                p.sync_fields();
                p.status = "Host dataset loaded".into();
            },
            Err(error) => self.projection.status = error,
        }
    }

    pub(in crate::web_tree) fn projection_fields(
        &self,
        mut snapshot: ProbeSnapshot,
    ) -> ProbeSnapshot {
        let p = &self.projection;
        for (name, value) in [
            (
                "projection-compare",
                if p.comparing { "on" } else { "off" }.into(),
            ),
            (
                "projection-compare-cells",
                p.comparison
                    .as_ref()
                    .map_or(0, |c| c.cells.len())
                    .to_string(),
            ),
            (
                "projection-arrangement",
                p.editor
                    .draft()
                    .arrangement
                    .kind
                    .trim_end_matches(".default")
                    .into(),
            ),
            ("projection-executable", p.compiled.is_some().to_string()),
            (
                "projection-occurrences",
                p.dataset
                    .as_ref()
                    .map_or(0, |d| d.occurrences.len())
                    .to_string(),
            ),
            (
                "projection-selected-occurrence",
                p.selected.clone().unwrap_or_default(),
            ),
            ("projection-placement-solves", p.solves.to_string()),
            ("projection-placement-reuses", p.reuses.to_string()),
            ("projection-editor-save-count", p.saves.to_string()),
            ("projection-editor-dirty", p.editor.is_dirty().to_string()),
            (
                "session-store",
                if self.saving {
                    "pending"
                } else if self.save_state == "error" {
                    "failed"
                } else if self.app.as_ref().is_some_and(|app| app.host.has_unstored()) {
                    "unstored"
                } else if self.app.is_none() {
                    "pending"
                } else {
                    "stored"
                }
                .into(),
            ),
        ] {
            snapshot = snapshot.with_field(name, value);
        }
        snapshot
    }
    pub(super) fn projection_action(&mut self, action: &str) -> Result<(), String> {
        if self.selection_locked() {
            return Err("A save is pending".into());
        }
        let p = &mut self.projection;
        p.open = true;
        match action {
            "load" => {
                let dataset: ProjectionDataset =
                    serde_json::from_str(PRACTICE).map_err(|e| e.to_string())?;
                p.editor =
                    ProjectionEditor::new(draft_from_definition(&default_definition(&dataset)));
                p.editor.reduce(
                    EditorAction::SelectPanel(ProjectionPanel::Preview),
                    js_sys::Date::now() as u64,
                );
                p.dataset = Some(dataset);
                p.compiled = None;
                p.selected = None;
                p.refresh();
                p.sync_fields();
                p.status = "Practice Set loaded · unsaved".into();
            },
            "undo" | "redo" => {
                let changed = if action == "undo" {
                    p.editor.undo()
                } else {
                    p.editor.redo()
                };
                if changed {
                    p.refresh();
                    p.sync_fields();
                }
                p.status = if changed {
                    format!(
                        "{} · {}",
                        if action == "undo" { "Undone" } else { "Redone" },
                        p.editor.panel().label()
                    )
                } else {
                    format!("Nothing to {action}")
                };
            },
            "grid" | "scatter" => {
                let arrangement = graphshell::projection_editor::with_kind(
                    &p.editor.draft().arrangement,
                    if action == "grid" {
                        "grid.default"
                    } else {
                        "scatter.default"
                    },
                    graphshell::projection_compile::practice_compiler().registry(),
                );
                p.editor.reduce(
                    EditorAction::SetArrangement(arrangement),
                    js_sys::Date::now() as u64,
                );
                p.editor.break_run();
                p.refresh();
                p.sync_fields();
            },
            "compare" => {
                p.comparing = !p.comparing;
                p.refresh_comparison();
            },
            "undo-save" | "redo-save" => {
                let app = self.app.as_mut().ok_or("Local graph is busy")?;
                let back = action == "undo-save";
                let reverted = if back {
                    app.host
                        .undo_now(graphshell::product::PROJECTION_EDITOR_VIA)
                } else {
                    app.host
                        .redo_now(graphshell::product::PROJECTION_EDITOR_VIA)
                }
                .map_err(|e| e.to_string())?;
                if let Some(reverted) = reverted {
                    let done = if back { "Undid save" } else { "Redid save" };
                    p.status = match app
                        .host
                        .saved_projection(&p.editor.draft().id)
                        .map_err(|e| e.to_string())?
                    {
                        Some(snapshot) => {
                            p.editor
                                .load_stored(draft_from_definition(&snapshot.definition));
                            p.selected = snapshot.selected_occurrence;
                            p.refresh();
                            p.sync_fields();
                            format!("{done} · {}", snapshot.definition.label)
                        },
                        None => {
                            p.editor.forget_saved();
                            format!("{done} · nothing stored for {}", p.editor.draft().id)
                        },
                    };
                    if let Some(kept) = graphshell::product::kept_summary(&reverted.kept) {
                        p.status.push_str(&format!(" · {kept}"));
                    }
                    self.persist_operation();
                } else {
                    p.status = format!("No save to {}", if back { "undo" } else { "redo" });
                }
            },
            "save" => {
                let snapshot = if let Some(dataset) = &p.dataset {
                    snapshot_editor(&p.editor, dataset, p.selected.as_deref())?
                } else {
                    ProjectionSnapshot {
                        definition: p
                            .editor
                            .draft()
                            .to_definition()
                            .map_err(|e| format!("Invalid: {e:?}"))?,
                        selected_occurrence: None,
                    }
                };
                self.app
                    .as_mut()
                    .ok_or("Local graph is busy")?
                    .host
                    .save_projection(&snapshot)
                    .map_err(|e| e.to_string())?;
                p.pending_save = true;
                self.persist_operation();
            },
            "reopen" => {
                let snapshot = self
                    .app
                    .as_ref()
                    .ok_or("Local graph is busy")?
                    .host
                    .saved_projection(&p.editor.draft().id)
                    .map_err(|e| e.to_string())?
                    .ok_or("No saved projection")?;
                if let Some(dataset) = &p.dataset {
                    let compiled = graphshell::projection_compile::practice_compiler()
                        .compile_snapshot(&snapshot, dataset)
                        .map_err(|e| format!("Cannot reopen: {e:?}"))?;
                    p.compiled = Some(compiled);
                    p.solves += 1;
                }
                p.editor
                    .load_stored(draft_from_definition(&snapshot.definition));
                p.selected = snapshot.selected_occurrence;
                p.sync_fields();
                p.status = if p.dataset.is_some() {
                    "Reopened executable projection".into()
                } else {
                    format!("Reloaded · {}", p.editor.draft().id)
                };
            },
            _ => return Err(format!("Unknown projection action: {action}")),
        }
        Ok(())
    }
}

pub(in crate::web_tree) fn section(page: &TreePage) -> Child {
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = vec![Box::new(
        button("Projection editor", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.projection.open = !product.projection.open;
            }
        })
        .attr("aria-expanded", product.projection.open.to_string()),
    )];
    if product.projection.open {
        children.push(action_button("Load practice set", "load"));
    }
    Box::new(el("section", children).attr("aria-label", "Projection editor"))
}
fn action_button(label: &'static str, action: &'static str) -> Child {
    action_button_state(label, action, true)
}
fn action_button_state(label: &'static str, action: &'static str, enabled: bool) -> Child {
    let mut control = button(label, move |page: &mut TreePage, _| {
        if let Some(product) = &mut page.product {
            if let Err(error) = product.projection_action(action) {
                product.projection.status = format!(
                    "{} failed · {error}",
                    if action == "save" { "Save" } else { "Action" }
                );
            }
        }
    });
    if !enabled {
        control = control.attr("disabled", "");
    }
    Box::new(control)
}

pub(in crate::web_tree) fn editor(page: &TreePage) -> Option<Child> {
    let p = &page.product.as_ref()?.projection;
    if !p.open {
        return None;
    }
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Projection editor")),
        Box::new(
            el("p", p.status.clone())
                .attr("role", "status")
                .attr("aria-label", "Projection status"),
        ),
    ];
    let tabs: Vec<Child> = ProjectionPanel::ALL
        .into_iter()
        .map(|panel| {
            Box::new(
                button(panel.label(), move |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product
                            .projection
                            .editor
                            .reduce(EditorAction::SelectPanel(panel), js_sys::Date::now() as u64);
                    }
                })
                .attr("role", "tab")
                .attr("aria-selected", (p.editor.panel() == panel).to_string()),
            ) as Child
        })
        .collect();
    children.push(Box::new(
        el("div", tabs)
            .attr("role", "tablist")
            .attr("aria-label", "Projection panels"),
    ));
    for (index, (_, label, panel)) in FIELDS.iter().enumerate() {
        if *panel != p.editor.panel() {
            continue;
        }
        let label = *label;
        children.push(Box::new(el(
            "label",
            (
                label,
                lens(
                    move |input: &mut TextInput| text_field_typed(input).attr("aria-label", label),
                    move |page: &mut TreePage| {
                        &mut page.product.as_mut().expect("projection").projection.inputs[index]
                    },
                ),
            ),
        )));
    }
    if p.editor.panel() == ProjectionPanel::Interaction {
        children.push(Box::new(el(
            "p",
            "Selection, pan, and zoom remain host interaction policy.",
        )));
    }
    if p.editor.panel() == ProjectionPanel::Arrangement {
        children.push(Box::new(
            el("div", options::rows(p))
                .attr("role", "group")
                .attr("aria-label", "Arrangement options"),
        ));
    }
    children.push(Box::new(
        el(
            "p",
            format!(
                "{} · {}",
                p.editor.draft().appearance.title,
                p.editor.draft().arrangement.kind
            ),
        )
        .attr("role", "status")
        .attr("aria-label", "Projection preview"),
    ));
    children.push(Box::new(
        el(
            "p",
            if p.editor.is_dirty() {
                "Unsaved changes"
            } else {
                "No unsaved changes"
            },
        )
        .attr("role", "status")
        .attr("aria-label", "Projection saved state"),
    ));
    for (label, action) in [
        ("Undo projection", "undo"),
        ("Redo projection", "redo"),
        ("Save projection", "save"),
        ("Reopen projection", "reopen"),
        ("Undo save", "undo-save"),
        ("Redo save", "redo-save"),
        ("Compare arrangements", "compare"),
        ("Projection: grid", "grid"),
        ("Projection: order by tempo", "scatter"),
    ] {
        children.push(action_button_state(
            label,
            action,
            match action {
                "undo" => p.editor.can_undo(),
                "redo" => p.editor.can_redo(),
                _ => true,
            },
        ));
    }
    if let Some(comparison) = comparison::view(p, (page.size.0 as f32 - 420.0).max(280.0)) {
        children.push(comparison);
    }
    if let (Some(compiled), Some(dataset)) = (&p.compiled, &p.dataset) {
        let mut spatial: Vec<Child> = Vec::new();
        let mut list: Vec<Child> = Vec::new();
        let bounds = compiled.scene.bounds;
        let width = (page.size.0 as f32 - 420.0).max(280.0);
        let scale = ((width - 24.0) / bounds.size.w.max(1.0)).min(1.0);
        for (index, item) in compiled.scene.items.iter().enumerate() {
            let instance = InstanceId(index as u32);
            let Some(occurrence) = compiled.occurrence_by_instance.get(&instance) else {
                continue;
            };
            let label = compiled
                .labels
                .get(&instance)
                .cloned()
                .unwrap_or_else(|| occurrence.clone());
            let details = dataset
                .occurrences
                .iter()
                .find(|row| &row.occurrence_id == occurrence)
                .map(|row| format!("{:?}", row.values))
                .unwrap_or_default();
            let selected = p.selected.as_ref() == Some(occurrence);
            for (view, output) in [("spatial", &mut spatial), ("list", &mut list)] {
                let id = occurrence.clone();
                let mut control = button(
                    format!("{label} · {details}"),
                    move |page: &mut TreePage, _| {
                        if let Some(product) = &mut page.product {
                            product.projection.selected = Some(id.clone());
                            product.projection.refresh();
                        }
                    },
                )
                .attr("aria-label", format!("{view} {occurrence}: {label}"))
                .attr("aria-pressed", selected.to_string())
                .attr("data-projection-view", view)
                .attr("data-projection-occurrence", occurrence.clone());
                if view == "spatial" {
                    if let Some(footprint) = item.footprint.bounds() {
                        control = control.attr("style", format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:hidden;", 12.0+(item.transform.translate.x+footprint.origin.x-bounds.origin.x)*scale,12.0+(item.transform.translate.y+footprint.origin.y-bounds.origin.y)*scale,footprint.size.w*scale,footprint.size.h*scale));
                    }
                }
                output.push(Box::new(control));
            }
        }
        children.push(Box::new(
            el("div", spatial)
                .attr("aria-label", "Spatial projection")
                .attr(
                    "style",
                    format!(
                        "position:relative;height:{}px;width:{}px;",
                        bounds.size.h * scale + 24.0,
                        width
                    ),
                ),
        ));
        children.push(Box::new(
            el("div", list).attr("aria-label", "Projection occurrences"),
        ));
    }
    Some(Box::new(
        el("section", children)
            .attr("class", "tree-projection")
            .attr(
                "style",
                format!("width:{}px;", page.size.0.saturating_sub(346).max(260)),
            )
            .attr("aria-label", "Projection editor workspace"),
    ))
}
