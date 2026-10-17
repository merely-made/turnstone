// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The projection editor and its two views share one typed draft and occurrence selection.
use super::*;
use cambium::{button, el, lens, text_field_typed};
use graphshell::local_projection::{compile_editor, draft_from_definition, snapshot_editor};
use graphshell::projection_compile::initial_projection_draft;
use graphshell::projection_compile::{
    CompiledProjection, ProjectionDataset, ProjectionSnapshot, default_definition,
};
use graphshell::projection_dynamics_compare::{
    DynamicsComparison, compare_arrangements_dynamics, compare_dynamics,
};
use mere::canvas::PhysicsLaw;
use mere::canvas::dynamics_recipe::preset_slot;
use mere::canvas::projection_dynamics::ProjectionDynamics;
use sceno::InstanceId;
use scenograph::Channel;
use scenomise::authoring::{EditorAction, ProjectionEditor, ProjectionPanel};
mod comparison;
mod options;
const PRACTICE: &str = include_str!("../../../web/fixtures/woodshed-stage.json");

pub(super) const FIELDS: [(&str, &str, ProjectionPanel); 17] = [
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
    ("dynamics.spec", "Dynamics spec", ProjectionPanel::Preview),
    (
        "dynamics.bound",
        "Preview step limit",
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
    dynamics: Option<ProjectionDynamics>,
    dynamics_comparison: Option<DynamicsComparison>,
    compare_dynamics: bool,
    settle_bound: Option<u32>,
    motion_paused: bool,
    compare_column: usize,
    compare_row: usize,
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
            dynamics: None,
            dynamics_comparison: None,
            compare_dynamics: false,
            settle_bound: Some(60),
            motion_paused: false,
            compare_column: 0,
            compare_row: 0,
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
            d.dynamics
                .as_ref()
                .map_or_else(String::new, |slot| slot.spec.clone()),
            self.settle_bound
                .map_or_else(String::new, |bound| bound.to_string()),
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
                if FIELDS[index].0 == "dynamics.bound" {
                    self.settle_bound = input.text().parse().ok();
                    continue;
                }
                self.status = match self.editor.edit_field(
                    FIELDS[index].0,
                    input.text(),
                    js_sys::Date::now() as u64,
                    graphshell::projection_compile::practice_compiler().registry(),
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
        )
        .and_then(|compiled| {
            let definition = self.editor.draft().to_definition().map_err(|issues| {
                issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.field, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            })?;
            let dynamics = definition
                .dynamics
                .as_ref()
                .map(|slot| {
                    ProjectionDynamics::new(&definition, &compiled, slot)
                        .map_err(|error| error.to_string())
                })
                .transpose()?;
            Ok((compiled, dynamics))
        }) {
            Ok((compiled, dynamics)) => {
                if compiled.placement_reused {
                    self.reuses += 1;
                } else {
                    self.solves += 1;
                }
                let keep_dynamics = compiled.placement_reused
                    && self.editor.draft().dynamics.as_ref().is_some_and(|slot| {
                        self.dynamics
                            .as_mut()
                            .is_some_and(|preview| preview.refresh_display(slot, &compiled.scene))
                    });
                self.compiled = Some(compiled);
                if !keep_dynamics {
                    self.dynamics = dynamics;
                }
            },
            Err(error) => {
                self.compiled = None;
                self.dynamics = None;
                self.status = error;
            },
        }
        self.refresh_comparison();
    }
    pub(super) fn needs_frame(&self) -> bool {
        !self.motion_paused
            && if self.comparing {
                self.dynamics_comparison
                    .as_ref()
                    .is_some_and(DynamicsComparison::is_running)
            } else {
                self.dynamics
                    .as_ref()
                    .is_some_and(ProjectionDynamics::is_running)
            }
    }
    pub(super) fn advance(&mut self) -> bool {
        if !self.needs_frame() {
            return false;
        }
        if self.comparing {
            if let Some(preview) = &mut self.dynamics_comparison {
                match preview.tick() {
                    Ok(_) => self.comparison = Some(preview.comparison.clone()),
                    Err(error) => {
                        self.status = error;
                        self.motion_paused = true;
                    },
                }
            }
        } else if let Some(preview) = &mut self.dynamics {
            if let Err(error) = preview.tick() {
                self.status = error.to_string();
                self.motion_paused = true;
            }
        }
        true
    }
    fn refresh_comparison(&mut self) {
        self.comparison = None;
        self.dynamics_comparison = None;
        if !self.comparing {
            return;
        }
        let Some(dataset) = &self.dataset else {
            return;
        };
        let layout = graphshell::projection_compare::FacetLayout {
            cell: sceno::Size2::new(220.0, 150.0),
            gap: 16.0,
            heading: 28.0,
        };
        let draft = self.editor.draft();
        if self.compare_dynamics || draft.dynamics.is_some() {
            let result = self
                .settle_bound
                .ok_or_else(|| "Choose a preview step limit to compare dynamics".to_string())
                .and_then(|bound| {
                    if self.compare_dynamics {
                        let choices = PhysicsLaw::ALL
                            .iter()
                            .map(|law| {
                                preset_slot(law.id()).map(|slot| (law.label().to_string(), slot))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        compare_dynamics(
                            draft,
                            dataset,
                            self.selected.as_deref(),
                            &choices,
                            bound,
                            &layout,
                        )
                    } else {
                        compare_arrangements_dynamics(
                            draft,
                            dataset,
                            self.selected.as_deref(),
                            bound,
                            &layout,
                        )
                    }
                });
            match result {
                Ok(preview) => {
                    self.comparison = Some(preview.comparison.clone());
                    self.dynamics_comparison = Some(preview);
                },
                Err(error) => self.status = error,
            }
        } else {
            match graphshell::projection_compare::compare_arrangements(
                draft,
                dataset,
                self.selected.as_deref(),
                &layout,
            ) {
                Ok(comparison) => self.comparison = Some(comparison),
                Err(error) => self.status = error,
            }
        }
        if let Some(comparison) = &self.comparison {
            (self.compare_column, self.compare_row) =
                graphshell::retained_policy::clamp_comparison_page(
                    (self.compare_column, self.compare_row),
                    comparison.facet.columns.labels.len(),
                    comparison
                        .facet
                        .rows
                        .as_ref()
                        .map_or(1, |axis| axis.labels.len()),
                );
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
            let dynamics = definition
                .dynamics
                .as_ref()
                .map(|slot| {
                    ProjectionDynamics::new(&definition, &compiled, slot)
                        .map_err(|error| error.to_string())
                })
                .transpose()?;
            Ok::<_, String>((dataset, editor, compiled, dynamics))
        })();
        match result {
            Ok((dataset, editor, compiled, dynamics)) => {
                let p = &mut self.projection;
                p.editor = editor;
                p.dataset = Some(dataset);
                p.compiled = Some(compiled);
                p.dynamics = dynamics;
                p.selected = None;
                p.refresh_comparison();
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
            (
                "projection-dynamics",
                if p.dynamics.is_some() { "on" } else { "off" }.into(),
            ),
            (
                "projection-dynamics-ticks",
                p.dynamics
                    .as_ref()
                    .map_or(0, ProjectionDynamics::ticks)
                    .to_string(),
            ),
            ("projection-motion-paused", p.motion_paused.to_string()),
            (
                "projection-step-limit",
                p.settle_bound
                    .map_or_else(String::new, |bound| bound.to_string()),
            ),
            (
                "projection-compare-axis",
                if p.compare_dynamics {
                    "dynamics"
                } else {
                    "scope"
                }
                .into(),
            ),
            (
                "projection-compare-refused",
                p.comparison
                    .as_ref()
                    .map_or(0, |c| c.refused.len())
                    .to_string(),
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
                let arrangement = scenomise::arrangement::with_kind(
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
            "compare" | "compare-dynamics" => {
                let dynamics = action == "compare-dynamics";
                p.comparing = !p.comparing || p.compare_dynamics != dynamics;
                p.compare_dynamics = dynamics;
                p.refresh_comparison();
            },
            "motion" => p.motion_paused = !p.motion_paused,
            "compare-left" | "compare-right" | "compare-up" | "compare-down" => {
                if let Some(comparison) = &p.comparison {
                    match action {
                        "compare-left" => p.compare_column = p.compare_column.saturating_sub(1),
                        "compare-right" => {
                            p.compare_column = (p.compare_column + 1)
                                .min(comparison.facet.columns.labels.len().saturating_sub(1))
                        },
                        "compare-up" => p.compare_row = p.compare_row.saturating_sub(1),
                        _ => {
                            p.compare_row = (p.compare_row + 1).min(
                                comparison
                                    .facet
                                    .rows
                                    .as_ref()
                                    .map_or(1, |axis| axis.labels.len())
                                    .saturating_sub(1),
                            )
                        },
                    }
                }
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
                if p.dataset.is_some()
                    && p.editor.draft().dynamics.is_some()
                    && p.compiled.is_none()
                {
                    return Err(format!("Cannot execute: {}", p.status));
                }
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
                    let dynamics = snapshot
                        .definition
                        .dynamics
                        .as_ref()
                        .map(|slot| {
                            ProjectionDynamics::new(&snapshot.definition, &compiled, slot)
                                .map_err(|error| format!("Cannot reopen: {error}"))
                        })
                        .transpose()?;
                    p.compiled = Some(compiled);
                    p.dynamics = dynamics;
                    p.solves += 1;
                }
                p.editor
                    .load_stored(draft_from_definition(&snapshot.definition));
                p.selected = snapshot.selected_occurrence;
                p.refresh_comparison();
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
                .attr("data-projection-panel", panel.label().to_lowercase())
                .attr("aria-selected", (p.editor.panel() == panel).to_string()),
            ) as Child
        })
        .collect();
    children.push(Box::new(
        el("div", tabs)
            .attr("role", "tablist")
            .attr("aria-label", "Projection panels"),
    ));
    for (index, (field, label, panel)) in FIELDS.iter().enumerate() {
        if *panel != p.editor.panel() {
            continue;
        }
        let label = *label;
        let field = *field;
        children.push(Box::new(el(
            "label",
            (
                label,
                lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input)
                            .attr("aria-label", label)
                            .attr("data-projection-field", field)
                    },
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
        ("Compare dynamics", "compare-dynamics"),
        ("Pause or resume preview", "motion"),
        ("Earlier arrangements", "compare-left"),
        ("Later arrangements", "compare-right"),
        ("Earlier comparison rows", "compare-up"),
        ("Later comparison rows", "compare-down"),
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
        let scene = p
            .dynamics
            .as_ref()
            .map_or(&compiled.scene, ProjectionDynamics::scene);
        let bounds = scene.bounds;
        let width = (page.size.0 as f32 - 420.0).max(280.0);
        let scale = ((width - 24.0) / bounds.size.w.max(1.0)).min(1.0);
        for (index, item) in scene.items.iter().enumerate() {
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
