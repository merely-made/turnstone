// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The projection editor and its two views share one typed draft and occurrence selection.
use super::*;
use cambium::{
    TabBar, TabBarNames, TabItem, button, disclosure_trigger, el, lens, tab_bar_view,
    text_field_typed,
};
use cartography::canvas::PhysicsLaw;
use cartography::canvas::dynamics_recipe::preset_slot;
use cartography::canvas::projection_dynamics::ProjectionDynamics;
use graphshell::local_projection::{compile_editor, draft_from_definition, snapshot_editor};
use graphshell::product::ProductHost;
use graphshell::projection_compile::default_definition;
use graphshell::projection_compile::initial_projection_draft;
use graphshell::projection_dynamics_compare::{
    DynamicsComparison, compare_arrangements_dynamics, compare_dynamics,
};
use graphshell::projection_save::{PendingSave, SaveAck};
use sceno::InstanceId;
use scenograph::Channel;
use scenomise::authoring::{EditorAction, ProjectionEditor, ProjectionPanel};
use scenomise::projection::{CompiledProjection, ProjectionDataset, ProjectionSnapshot};
mod comparison;
mod options;
mod scene_marks;
use scenograph::swatch::AxisKind;
use scenomise::host_dataset::{parse_host_dataset, route_relationships};
use scenomise::projection::DisclosedRelationship;
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
    /// Counts editors installed, so a save names the one it was sent from.
    generation: u64,
    pending_save: Option<PendingSave>,
    pub(super) options: Vec<options::OptionRow>,
    options_kind: String,
    comparing: bool,
    comparison: Option<graphshell::projection_compare::Comparison>,
    dynamics: Option<ProjectionDynamics>,
    dynamics_comparison: Option<DynamicsComparison>,
    compare_dynamics: bool,
    compare_slots: Option<(AxisKind, sceno::Rect)>,
    relationships: Vec<DisclosedRelationship>,
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
            generation: 0,
            pending_save: None,
            options: Vec::new(),
            options_kind: String::new(),
            comparing: false,
            comparison: None,
            dynamics: None,
            dynamics_comparison: None,
            compare_dynamics: false,
            compare_slots: None,
            relationships: Vec::new(),
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
        .and_then(|mut compiled| {
            if !self.relationships.is_empty() {
                route_relationships(&mut compiled, dataset, &self.relationships)
                    .map_err(relationship_issues)?;
            }
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
        if let Some((axis, viewport)) = self.compare_slots {
            match graphshell::projection_compare::compare_slots(
                draft,
                dataset,
                &self.relationships,
                self.selected.as_deref(),
                &layout,
                axis,
                viewport,
            ) {
                Ok(comparison) => self.comparison = Some(comparison),
                Err(error) => self.status = error,
            }
        } else if self.compare_dynamics || draft.dynamics.is_some() {
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
    fn replace_editor(&mut self, editor: ProjectionEditor) {
        self.editor = editor;
        self.generation += 1;
    }
    /// The host stored a save. Only the editor the save was sent from becomes
    /// clean; one that replaced it meanwhile keeps its own state.
    pub(super) fn saved(&mut self) {
        let Some(pending) = self.pending_save.take() else {
            return;
        };
        self.status = match pending.acknowledge(self.dataset.as_ref(), self.generation) {
            SaveAck::Current => {
                self.editor.mark_saved();
                self.saves += 1;
                if self.dataset.is_some() {
                    "Saved executable projection".into()
                } else {
                    "Saved projection".into()
                }
            },
            SaveAck::EarlierDataset => "Saved projection was for an earlier dataset".into(),
            SaveAck::EarlierDraft => "Saved projection was for an earlier draft".into(),
        };
    }
}
impl SavedProduct {
    pub(in crate::web_tree) fn load_projection_input(
        &mut self,
        json: &str,
        definition: Option<&str>,
    ) {
        let result = (|| {
            let value: serde_json::Value =
                serde_json::from_str(json).map_err(|e| format!("Source load failed: {e}"))?;
            let (dataset, relationships) = if value.get("schema").is_some() {
                let host =
                    parse_host_dataset(json).map_err(|e| format!("Source load failed: {e}"))?;
                (host.dataset, host.relationships)
            } else {
                (
                    serde_json::from_value(value)
                        .map_err(|e| format!("Source load failed: {e}"))?,
                    Vec::new(),
                )
            };
            let definition = definition
                .map(serde_json::from_str)
                .transpose()
                .map_err(|e| format!("Recipe load failed: {e}"))?
                .unwrap_or_else(|| default_definition(&dataset));
            let editor = ProjectionEditor::new(draft_from_definition(&definition));
            let mut compiled = compile_editor(&editor, &dataset, None, None)?;
            if !relationships.is_empty() {
                route_relationships(&mut compiled, &dataset, &relationships)
                    .map_err(relationship_issues)?;
            }
            let dynamics = definition
                .dynamics
                .as_ref()
                .map(|slot| {
                    ProjectionDynamics::new(&definition, &compiled, slot)
                        .map_err(|error| error.to_string())
                })
                .transpose()?;
            Ok::<_, String>((dataset, relationships, editor, compiled, dynamics))
        })();
        match result {
            Ok((dataset, relationships, editor, compiled, dynamics)) => {
                let p = &mut self.projection;
                p.replace_editor(editor);
                p.dataset = Some(dataset);
                p.compiled = Some(compiled);
                p.relationships = relationships;
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
            (
                "projection-node-mark",
                p.editor
                    .draft()
                    .marks
                    .node
                    .as_ref()
                    .map_or_else(|| "default".into(), |mark| mark.label()),
            ),
            (
                "projection-relation-form",
                p.editor
                    .draft()
                    .marks
                    .relation
                    .map_or("default", |form| form.id())
                    .into(),
            ),
            (
                "projection-field-form",
                p.editor
                    .draft()
                    .marks
                    .field
                    .and_then(|form| serde_json::to_value(form).ok())
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "default".into()),
            ),
            (
                "projection-frame",
                p.editor
                    .draft()
                    .frame
                    .as_ref()
                    .map_or("default", |frame| frame.kind().id())
                    .into(),
            ),
            (
                "projection-frame-contained",
                p.compiled
                    .as_ref()
                    .is_some_and(|compiled| {
                        scene_marks::bounded_frame_contains_items(&compiled.scene)
                    })
                    .to_string(),
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
                p.compare_slots
                    .map(|(axis, _)| scene_marks::axis_id(axis))
                    .unwrap_or(if p.compare_dynamics {
                        "dynamics"
                    } else {
                        "scope"
                    })
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
        if matches!(action, "save" | "undo-save" | "redo-save") {
            self.view_only()?;
        }
        if self.selection_locked() {
            return Err("A save is pending".into());
        }
        let p = &mut self.projection;
        p.open = true;
        match action {
            "load" => {
                let dataset: ProjectionDataset =
                    serde_json::from_str(PRACTICE).map_err(|e| e.to_string())?;
                p.replace_editor(ProjectionEditor::new(draft_from_definition(
                    &default_definition(&dataset),
                )));
                p.editor.reduce(
                    EditorAction::SelectPanel(ProjectionPanel::Preview),
                    js_sys::Date::now() as u64,
                );
                p.relationships.clear();
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
                p.comparing =
                    !p.comparing || p.compare_slots.is_some() || p.compare_dynamics != dynamics;
                p.compare_slots = None;
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
                if p.dataset.is_some() && p.compiled.is_none() {
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
                p.pending_save = Some(PendingSave::sent(p.dataset.as_ref(), p.generation));
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
    let mut children: Vec<Child> = vec![Box::new(disclosure_trigger(
        "Projection editor",
        product.projection.open,
        |page: &mut TreePage| {
            if let Some(product) = &mut page.product {
                product.projection.open = !product.projection.open;
            }
        },
    ))];
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
    let product = page.product.as_ref()?;
    let p = &product.projection;
    if !p.open {
        return None;
    }
    // A follower may arrange a draft here but stores no projection.
    let stores = !product.following();
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Projection editor")),
        Box::new(status_note_labelled(
            "",
            "Projection status",
            p.status.clone(),
        )),
    ];
    let panels = ProjectionPanel::ALL;
    let items: Vec<TabItem> = panels
        .iter()
        .map(|panel| TabItem::new(panel.label()).with_key(panel.label().to_lowercase()))
        .collect();
    let selected = panels
        .iter()
        .position(|panel| *panel == p.editor.panel())
        .unwrap_or(0);
    children.push(Box::new(tab_bar_view::<
        TreePage,
        (),
        (),
        _,
        fn(&mut TreePage, usize) -> (),
    >(
        TabBar::new(&items, selected)
            .with_label("Projection panels")
            .with_names(TabBarNames {
                key_alias: Some("data-projection-panel"),
                ..TabBarNames::default()
            }),
        move |page: &mut TreePage, index: usize| {
            if let (Some(product), Some(panel)) = (&mut page.product, panels.get(index)) {
                product.projection.editor.reduce(
                    EditorAction::SelectPanel(*panel),
                    js_sys::Date::now() as u64,
                );
            }
        },
        None,
    )));
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
    children.push(Box::new(status_note_labelled(
        "",
        "Projection preview",
        format!(
            "{} · {}",
            p.editor.draft().appearance.title,
            p.editor.draft().arrangement.kind
        ),
    )));
    children.push(Box::new(status_note_labelled(
        "",
        "Projection saved state",
        if p.editor.is_dirty() {
            "Unsaved changes"
        } else {
            "No unsaved changes"
        },
    )));
    if !stores {
        children.push(Box::new(el("p", super::follow::VIEW_ONLY)));
    }
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
    ]
    .into_iter()
    .filter(|(_, action)| stores || !matches!(*action, "save" | "undo-save" | "redo-save"))
    {
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
    children.push(scene_marks::axis_controls(page, p));
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
        let subdivision = matches!(
            compiled.score.arrangement,
            sceno::Arrangement::Mosaic(_) | sceno::Arrangement::Partition(_)
        );
        let width = (page.size.0 as f32 - 420.0).max(280.0);
        let scale = ((width - 24.0) / bounds.size.w.max(1.0)).min(1.0);
        let form = p.editor.draft().marks.field;
        let to_screen = |point: sceno::Vec2| {
            [
                12.0 + (point.x - bounds.origin.x) * scale,
                12.0 + (point.y - bounds.origin.y) * scale,
            ]
        };
        if let Some(outline) = scene_marks::frame_outline(scene, &to_screen) {
            spatial.push(outline);
        }
        spatial.extend(scene_marks::fields(scene, &to_screen, |_| form));
        spatial.extend(scene_marks::relations(scene, &to_screen));
        for (id, row, column) in scenomise::relation_forms::matrix_headings(scene) {
            if let Some(label) = compiled.labels.get(&id) {
                for point in [row, column] {
                    let [x, y] = to_screen(point);
                    spatial.push(Box::new(el("span", label.clone())
                        .attr("style", format!("position:absolute;pointer-events:none;left:{x}px;top:{y}px;transform:translate(-50%,-50%);font-size:11px;"))));
                }
            }
        }
        for (index, item) in scene.items.iter().enumerate() {
            if !item.visible {
                continue;
            }
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
                let assigned = view == "spatial" && subdivision;
                let assigned_card = assigned && item.representation == sceno::Representation::Card;
                let caption = if assigned_card {
                    format!("{label}\n{details}")
                } else if assigned {
                    label.clone()
                } else {
                    format!("{label} · {details}")
                };
                let mut control = button(caption, move |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.projection.selected = Some(id.clone());
                        product.projection.refresh();
                    }
                })
                .attr("aria-label", format!("{view} {occurrence}: {label}"))
                .attr("aria-pressed", selected.to_string())
                .attr("data-projection-view", view)
                .attr("data-projection-occurrence", occurrence.clone());
                if view == "spatial"
                    && let Some(footprint) = item.footprint.bounds()
                {
                    let world = scene
                        .to_world(item.space)
                        .unwrap_or(sceno::Transform2::IDENTITY)
                        .then(&item.transform);
                    let [x, y] = to_screen(world.apply(footprint.origin));
                    let w = footprint.size.w * world.scale * scale;
                    let h = footprint.size.h * world.scale * scale;
                    let mut style = format!(
                        "position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h}px;overflow:hidden;"
                    );
                    if assigned {
                        style.push_str("box-sizing:border-box;min-width:0;min-height:0;");
                        assigned_clip(&mut style, &item.footprint, footprint);
                        if assigned_card {
                            control = control.attr("data-projection-card", "");
                            style.push_str("text-align:start;white-space:pre-line;background:#38556555;box-shadow:inset 0 0 0 1px #879cab;");
                        }
                    }
                    if form == Some(scenograph::FieldForm::MemberTint)
                        && scene
                            .regions
                            .iter()
                            .any(|region| region.members.contains(&instance))
                    {
                        style.push_str("background:#31594f;");
                    }
                    control = control.attr("style", style);
                }
                output.push(Box::new(control));
            }
        }
        for report in &scene.relation_constraints {
            let outcome = if report.honored { "Honoured" } else { "Unmet" };
            children.push(Box::new(el(
                "p",
                format!("{} · {outcome} · {}", report.constraint.id, report.reason),
            )));
        }
        children.push(Box::new(
            el("div", spatial)
                .attr("aria-label", "Spatial projection")
                .attr(
                    "data-projection-frame",
                    scene
                        .frame
                        .as_ref()
                        .map_or("default", |f| f.spec.kind().id()),
                )
                .attr("data-projection-frame-width", bounds.size.w.to_string())
                .attr("data-projection-frame-height", bounds.size.h.to_string())
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

/// The subdivision owns the box and, for Partition rings, the sector outline.
fn assigned_clip(style: &mut String, footprint: &sceno::Footprint, bounds: sceno::Rect) {
    use std::fmt::Write as _;
    let sceno::Footprint::Polygon { points } = footprint else {
        return;
    };
    if bounds.size.w <= 0.0 || bounds.size.h <= 0.0 {
        return;
    }
    style.push_str("clip-path:polygon(");
    for (index, point) in points.iter().enumerate() {
        if index > 0 {
            style.push(',');
        }
        let x = 100.0 * (point.x - bounds.origin.x) / bounds.size.w;
        let y = 100.0 * (point.y - bounds.origin.y) / bounds.size.h;
        let _ = write!(style, "{x}% {y}%");
    }
    style.push_str(");");
}

fn relationship_issues(issues: Vec<scenomise::projection::CompileIssue>) -> String {
    issues
        .into_iter()
        .map(|issue| issue.message)
        .collect::<Vec<_>>()
        .join(". ")
}
