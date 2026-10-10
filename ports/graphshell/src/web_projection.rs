// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Executable editor preview. The compiler owns placement; this browser host
//! realizes one scene as spatial cards and an occurrence list. Both use the
//! same selection and exactly the same rectangles for paint and semantic input.

use std::{cell::RefCell, rc::Rc};

use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement, el, text};
use genet_render::TextSystem;
use genet_scripted_dom::ScriptedDom;
use graphshell::projection_compile::{
    CompiledProjection, ProjectionDataset, ProjectionSnapshot, ProjectionValue,
    default_definition, practice_compiler,
};
use graphshell::product::{PROJECTION_EDITOR_VIA, kept_summary};
use graphshell::projection_compare::{
    Comparison, FACET_CELL_ADAPTER, FacetLayout, compare_arrangements,
};
use graphshell::projection_editor::{EditorAction, ProjectionEditor, ProjectionPanel, with_kind};
use graphshell::projection_dynamics_compare::{DynamicsComparison, compare_dynamics, compare_arrangements_dynamics};
use mere::canvas::projection_dynamics::ProjectionDynamics;
use mere::canvas::dynamics_recipe::preset_slot;
use mere::canvas::PhysicsLaw;
use netrender::Scene;
use sceno::InstanceId;

use super::web_session::SessionStore;
use super::{
    BrowserHost, document, draft_from_definition, element, now_ms, root, set_text,
};

const PRACTICE_DATA: &str = include_str!("../web/fixtures/woodshed-stage.json");

pub(super) struct LiveProjection {
    dataset: ProjectionDataset,
    compiled: Option<CompiledProjection>,
    selected: Option<String>,
    error: String,
    dirty: bool,
    scene: Scene,
    host_sheet: String,
    targets: Vec<Target>,
    extent: (u32, u32),
    title: String,
    axes: String,
    fields: [String; 2],
    solves: u64,
    placement_reuses: u64,
    /// The preview shows the comparison grid instead of the projection
    /// (Scenograph editor plan, SE80).
    comparing: bool,
    comparison: Option<Comparison>,
    dynamics_comparison: Option<DynamicsComparison>,
    dynamics: Option<ProjectionDynamics>,
    compare_dynamics: bool,
    settle_bound: Option<u32>,
    motion_paused: bool,
    compare_column: usize,
    compare_row: usize,
    /// The draft's arrangement kind, for the scenario lane.
    arrangement: String,
}

#[derive(Clone)]
struct Target {
    occurrence: String,
    view: &'static str,
    label: String,
    detail: String,
    rect: [f32; 4],
    selected: bool,
}

impl BrowserHost {
    pub(super) fn load_practice_projection(&mut self) {
        self.load_projection_dataset(PRACTICE_DATA);
    }

    pub(super) fn load_projection_dataset(&mut self, json: &str) {
        self.load_projection_input(json, None);
    }

    pub(super) fn load_projection_input(&mut self, json: &str, definition_json: Option<&str>) {
        let result = serde_json::from_str::<ProjectionDataset>(json);
        match result {
            Ok(dataset) => {
                let definition = match definition_json.map(serde_json::from_str).transpose() {
                    Ok(definition) => definition.unwrap_or_else(|| default_definition(&dataset)),
                    Err(error) => {
                        self.projection_editor_status = format!("Recipe load failed: {error}");
                        return;
                    },
                };
                // Validate before replacing a working scene with host-supplied facts.
                let compiled = match practice_compiler().compile(&definition, &dataset) {
                    Ok(compiled) => compiled,
                    Err(issues) => {
                        self.projection_editor_status = format!("Source load failed: {issues:?}");
                        return;
                    },
                };
                let dynamics = match definition.dynamics.as_ref().map(|slot| ProjectionDynamics::new(&definition, &compiled, slot)).transpose() {
                    Ok(dynamics) => dynamics,
                    Err(error) => { self.projection_editor_status = format!("Dynamics load failed: {error}"); return; },
                };
                if let Ok(container) = element("projection-live") {
                    let _ = container.remove_attribute("data-target-count");
                }
                self.projection_editor = ProjectionEditor::new(draft_from_definition(&definition));
                self.projection_editor
                    .reduce(EditorAction::SelectPanel(ProjectionPanel::Preview), now_ms());
                self.live_projection = Some(LiveProjection {
                    dataset,
                    compiled: Some(compiled),
                    selected: None,
                    error: String::new(),
                    dirty: true,
                    scene: Scene::new(self.width, self.height),
                    host_sheet: String::new(),
                    targets: Vec::new(),
                    extent: (0, 0),
                    title: String::new(),
                    axes: String::new(),
                    fields: [String::new(), String::new()],
                    solves: 1,
                    placement_reuses: 0,
                    comparing: false,
                    comparison: None,
                    dynamics_comparison: None,
                    dynamics,
                    compare_dynamics: false,
                    settle_bound: Some(60),
                    motion_paused: false,
                    compare_column: 0,
                    compare_row: 0,
                    arrangement: String::new(),
                });
                self.projection_editor_open = true;
                self.detail_open = false;
                self.recompile_projection();
                self.projection_editor_status = "Practice Set loaded · unsaved".into();
                self.chrome_dirty = true;
            },
            Err(error) => self.projection_editor_status = format!("Source load failed: {error}"),
        }
    }

    pub(super) fn recompile_projection(&mut self) {
        let Some(live) = &mut self.live_projection else {
            return;
        };
        let draft = self.projection_editor.draft();
        live.title = draft.appearance.title.clone();
        live.arrangement = draft.arrangement.kind.clone();
        let channel_name = |channel: &graphshell::projection_editor::Channel| match channel {
            graphshell::projection_editor::Channel::Field(name)
            | graphshell::projection_editor::Channel::Constant(name) => name.clone(),
        };
        live.axes = format!(
            "{} · x: {} · y: {}",
            draft.arrangement.kind,
            channel_name(&draft.encoding.x),
            channel_name(&draft.encoding.y)
        );
        live.fields = [
            channel_name(&draft.encoding.x),
            channel_name(&draft.encoding.y),
        ];
        let result = self
            .projection_editor
            .draft()
            .to_definition()
            .map_err(|issues| {
                issues
                    .iter()
                    .map(|i| format!("{}: {}", i.field, i.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .and_then(|definition| {
                let compiled = match &live.compiled {
                    Some(previous) => practice_compiler().refresh(previous, &definition, &live.dataset),
                    None => practice_compiler().compile(&definition, &live.dataset),
                };
                let compiled = compiled.map_err(|issues| {
                    issues
                        .iter()
                        .map(|i| format!("{}: {}", i.field, i.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                })?;
                let dynamics = definition.dynamics.as_ref().map(|slot| {
                    ProjectionDynamics::new(&definition, &compiled, slot).map_err(|e| e.to_string())
                }).transpose()?;
                Ok((compiled, dynamics))
            });
        match result {
            Ok((mut compiled, dynamics)) => {
                if compiled.placement_reused {
                    live.placement_reuses += 1;
                } else {
                    live.solves += 1;
                }
                compiled.selected = live
                    .selected
                    .as_ref()
                    .and_then(|id| compiled.instance_by_occurrence.get(id))
                    .copied();
                let keep_dynamics = compiled.placement_reused
                    && draft.dynamics.as_ref().is_some_and(|slot| {
                        live.dynamics.as_mut().is_some_and(|preview| preview.refresh_display(slot, &compiled.scene))
                    });
                live.compiled = Some(compiled);
                if !keep_dynamics { live.dynamics = dynamics; }
                live.error.clear();
            },
            Err(error) => {
                live.compiled = None;
                live.dynamics = None;
                live.error = error;
            },
        }
        live.dirty = true;
        if let Ok(container) = element("projection-live") {
            let _ = container.set_attribute("data-placement-solves", &live.solves.to_string());
            let _ = container
                .set_attribute("data-placement-reuses", &live.placement_reuses.to_string());
        }
        self.refresh_projection_comparison();
    }

    /// Show or hide the comparison grid in the preview (SE80).
    pub(super) fn toggle_projection_compare(&mut self) {
        let Some(live) = &mut self.live_projection else {
            return;
        };
        live.comparing = !live.comparing || live.compare_dynamics;
        live.compare_dynamics = false;
        live.dirty = true;
        self.refresh_projection_comparison();
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }

    pub(super) fn toggle_projection_dynamics_compare(&mut self) {
        let Some(live) = &mut self.live_projection else { return; };
        live.comparing = !live.comparing || !live.compare_dynamics;
        live.compare_dynamics = true;
        live.dirty = true;
        self.refresh_projection_comparison();
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }

    pub(super) fn projection_settle_bound(&self) -> Option<u32> {
        self.live_projection.as_ref().and_then(|live| live.settle_bound)
    }

    pub(super) fn set_projection_settle_bound(&mut self, bound: Option<u32>) {
        if let Some(live) = &mut self.live_projection { live.settle_bound = bound; }
        self.refresh_projection_comparison();
        self.chrome_dirty = true;
    }

    pub(super) fn toggle_projection_motion(&mut self) {
        if let Some(live) = &mut self.live_projection {
            live.motion_paused = !live.motion_paused;
            live.dirty = true;
        }
        self.chrome_dirty = true;
    }

    pub(super) fn move_projection_comparison(&mut self, columns: isize, rows: isize) {
        if let Some(live) = &mut self.live_projection {
            if let Some(comparison) = &live.comparison {
                live.compare_column = live.compare_column.saturating_add_signed(columns)
                    .min(comparison.facet.columns.labels.len().saturating_sub(1));
                live.compare_row = live.compare_row.saturating_add_signed(rows)
                    .min(comparison.facet.rows.as_ref().map_or(1, |axis| axis.labels.len()).saturating_sub(1));
                live.dirty = true;
            }
        }
    }

    /// Rebuild the grid from the working draft, when it is showing.
    fn refresh_projection_comparison(&mut self) {
        let draft = self.projection_editor.draft().clone();
        let Some(live) = &mut self.live_projection else {
            return;
        };
        if !live.comparing {
            live.comparison = None;
            live.dynamics_comparison = None;
            return;
        }
        let layout = FacetLayout {
            cell: sceno::Size2::new(220.0, 150.0),
            gap: 16.0,
            heading: 28.0,
        };
        if live.compare_dynamics {
            let Some(bound) = live.settle_bound else {
                live.error = "Choose a preview step limit to compare dynamics".into();
                live.comparison = None;
                live.dynamics_comparison = None;
                live.dirty = true;
                return;
            };
            let choices: Vec<_> = PhysicsLaw::ALL.iter().map(|law| {
                preset_slot(law.id()).map(|slot| (law.label().to_string(), slot))
            }).collect::<Result<_, _>>().expect("the host catalog's presets bind");
            match compare_dynamics(&draft, &live.dataset, live.selected.as_deref(), &choices, bound, &layout) {
                Ok(comparison) => {
                    live.comparison = Some(comparison.comparison.clone());
                    live.dynamics_comparison = Some(comparison);
                    live.error.clear();
                },
                Err(error) => { live.comparison = None; live.dynamics_comparison = None; live.error = error; },
            }
            live.dirty = true;
            return;
        }
        live.dynamics_comparison = None;
        if draft.dynamics.is_some() {
            let result = live.settle_bound.ok_or_else(|| "Choose a preview step limit to compare dynamics".to_string())
                .and_then(|bound| compare_arrangements_dynamics(&draft, &live.dataset, live.selected.as_deref(), bound, &layout));
            match result {
                Ok(comparison) => {
                    live.comparison = Some(comparison.comparison.clone());
                    live.dynamics_comparison = Some(comparison);
                    live.error.clear();
                },
                Err(error) => { live.comparison = None; live.error = error; },
            }
            live.dirty = true;
            return;
        }
        match compare_arrangements(&draft, &live.dataset, live.selected.as_deref(), &layout) {
            Ok(comparison) => { live.comparison = Some(comparison); live.error.clear(); },
            Err(error) => {
                live.comparison = None;
                live.error = error;
            },
        }
        live.dirty = true;
    }

    /// Picking a cell applies its family to the draft as one undo step
    /// (SE81); the working cell is already the draft.
    pub(super) fn pick_projection_compare(&mut self, swatch_id: &str) {
        let Some(cell) = self
            .live_projection
            .as_ref()
            .and_then(|live| live.comparison.as_ref())
            .and_then(|comparison| {
                comparison
                    .cells
                    .iter()
                    .find(|cell| cell.swatch_id == swatch_id && !cell.working)
            })
            .cloned()
        else {
            return;
        };
        let arrangement = with_kind(
            &self.projection_editor.draft().arrangement,
            &cell.family,
            practice_compiler().registry(),
        );
        self.projection_editor.break_run();
        self.projection_editor.reduce(EditorAction::ApplyComparison { arrangement, dynamics: cell.dynamics }, now_ms());
        self.projection_editor.break_run();
        if let Some(live) = &mut self.live_projection {
            live.compare_column = 0;
            live.compare_row = 0;
        }
        self.recompile_projection();
        self.projection_editor_status = format!("Recipe · {} (from the comparison)", cell.family);
        self.chrome_dirty = true;
    }

    pub(super) fn projection_arrangement(&mut self, id: &str) {
        let mut arrangement = self.projection_editor.draft().arrangement.clone();
        arrangement.kind = id.into();
        self.projection_editor
            .reduce(EditorAction::SetArrangement(arrangement), now_ms());
        self.recompile_projection();
    }

    pub(super) fn select_projection_occurrence(&mut self, occurrence: &str) {
        let Some(live) = &mut self.live_projection else {
            return;
        };
        let Some(compiled) = &mut live.compiled else {
            return;
        };
        let Some(instance) = compiled.instance_by_occurrence.get(occurrence).copied() else {
            return;
        };
        compiled.selected = Some(instance);
        live.selected = Some(occurrence.into());
        live.dirty = true;
        self.chrome_dirty = true;
        self.refresh_projection_comparison();
    }

    pub(super) fn save_live_projection(&mut self) {
        self.recompile_projection();
        let result = (|| -> Result<ProjectionSnapshot, String> {
            let live = self
                .live_projection
                .as_ref()
                .ok_or("No executable source loaded")?;
            if live.compiled.is_none() {
                return Err(live.error.clone());
            }
            let definition = self
                .projection_editor
                .draft()
                .to_definition()
                .map_err(|_| "Invalid draft")?;
            let snapshot = ProjectionSnapshot {
                definition,
                selected_occurrence: live.selected.clone(),
            };
            Ok(snapshot)
        })()
        .and_then(|snapshot| {
            self.app
                .host
                .save_projection(&snapshot)
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(_) => {
                self.session_store = SessionStore::Pending;
                self.projection_editor.mark_saved();
                self.projection_editor_save_count += 1;
                self.projection_editor_status =
                    "Saved executable projection and occurrence selection".into();
            },
            Err(error) => self.projection_editor_status = format!("Save failed: {error}"),
        }
    }

    pub(super) fn reload_live_projection(&mut self) {
        let result = (|| -> Result<ProjectionSnapshot, String> {
            let live = self
                .live_projection
                .as_ref()
                .ok_or("Load the source before reopening")?;
            let saved = self
                .app
                .host
                .saved_projection(&self.projection_editor.draft().id)
                .map_err(|error| error.to_string())?
                .ok_or("No saved executable projection")?;
            let compiled = practice_compiler().compile_snapshot(&saved, &live.dataset).map_err(|issues| {
                issues
                    .iter()
                    .map(|i| format!("{}: {}", i.field, i.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            })?;
            if let Some(slot) = &saved.definition.dynamics {
                ProjectionDynamics::new(&saved.definition, &compiled, slot).map_err(|e| e.to_string())?;
            }
            Ok(saved)
        })();
        match result {
            Ok(saved) => {
                self.projection_editor =
                    ProjectionEditor::new(draft_from_definition(&saved.definition));
                self.projection_editor
                    .reduce(EditorAction::SelectPanel(ProjectionPanel::Preview), now_ms());
                self.live_projection.as_mut().unwrap().selected = saved.selected_occurrence;
                self.recompile_projection();
                self.projection_editor_status =
                    "Reopened executable projection and occurrence selection".into();
            },
            Err(error) => self.projection_editor_status = format!("Reopen failed: {error}"),
        }
    }
}

impl BrowserHost {
    /// How many items the arrangement rows measure defaults against: the
    /// loaded executable dataset's, or none (E5).
    pub(super) fn option_item_count(&self) -> Option<usize> {
        self.live_projection
            .as_ref()
            .map(|live| live.dataset.occurrences.len())
    }

    /// Undo (`back`) or redo the editor's latest save in the session, through
    /// its own channel, and take what the store now holds for this draft as
    /// one step on the draft's history (SE18, SE20, SE21).
    pub(super) fn step_projection_save(&mut self, back: bool) {
        let verb = if back { "undo" } else { "redo" };
        let id = self.projection_editor.draft().id.clone();
        let stepped = if back {
            self.app.host.undo_now(PROJECTION_EDITOR_VIA)
        } else {
            self.app.host.redo_now(PROJECTION_EDITOR_VIA)
        };
        let status = match stepped {
            Ok(Some(reverted)) => {
                self.session_store = SessionStore::Pending;
                let done = if back { "Undid save" } else { "Redid save" };
                let stored = match self.app.host.saved_projection(&id) {
                    Ok(Some(snapshot)) => {
                        self.projection_editor
                            .load_stored(draft_from_definition(&snapshot.definition));
                        if let Some(live) = self.live_projection.as_mut() {
                            live.selected = snapshot.selected_occurrence;
                        }
                        self.recompile_projection();
                        format!("{done} · {}", snapshot.definition.label)
                    },
                    Ok(None) => {
                        self.projection_editor.forget_saved();
                        format!("{done} · nothing stored for {id}")
                    },
                    Err(error) => format!("{done} · could not read it back: {error}"),
                };
                match kept_summary(&reverted.kept) {
                    Some(kept) => format!("{stored} · {kept}"),
                    None => stored,
                }
            },
            Ok(None) => format!("No save to {verb}"),
            Err(error) => format!("Save {verb} failed · {error}"),
        };
        self.projection_editor_status = status;
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }
}

impl LiveProjection {
    pub(super) fn decorate_chrome(&self, model: &mut super::ChromeModel) {
        model.active_session = format!(
            "Woodshed Set · {} occurrences",
            self.dataset.occurrences.len()
        );
        model.selection = self
            .selected
            .clone()
            .unwrap_or_else(|| "Choose a practice card".into());
        model.detail_address = self.dataset.source.resource.clone();
        model.arrangement = self.axes.clone();
        model.physics_law = if self.dynamics.is_some() { "recipe dynamics" } else { "fixed placement" }.into();
        model.physics_paused = self.dynamics.is_none() || self.motion_paused;
        model.product_status = format!("Projection source · {}", self.dataset.source.resource);
        model.action_status = if self.error.is_empty() {
            "Executable projection".into()
        } else {
            self.error.clone()
        };
        model.action_draft = None;
    }

    pub(super) fn frame(
        &mut self,
        width: u32,
        height: u32,
        text_system: &mut TextSystem,
        host_sheet: &str,
    ) -> Result<Scene, String> {
        if self.comparing && !self.motion_paused {
            if let Some(comparison) = &mut self.dynamics_comparison {
                if comparison.is_running() {
                    comparison.tick()?;
                    self.dirty = true;
                    self.comparison = Some(comparison.comparison.clone());
                }
            }
        } else if !self.comparing && !self.motion_paused {
            if let Some(dynamics) = &mut self.dynamics {
                if dynamics.is_running() {
                    dynamics.tick().map_err(|e| e.to_string())?;
                    self.dirty = true;
                }
            }
        }
        if self.host_sheet != host_sheet {
            self.host_sheet = host_sheet.to_string();
            self.dirty = true;
        }
        if !self.dirty && self.extent == (width, height) {
            return Ok(self.scene.clone());
        }
        self.extent = (width, height);
        self.targets.clear();
        let left = if width < 720 { 24.0 } else { 308.0 };
        let available = (width as f32 - left - 24.0).max(180.0);
        let stacked = available < 640.0;
        let spatial_w = if stacked {
            available
        } else {
            available - 250.0
        };
        let spatial_h = if stacked {
            240.0
        } else {
            (height as f32 - 240.0).max(200.0)
        };
        let list_x = if stacked {
            left
        } else {
            left + spatial_w + 20.0
        };
        let list_y = if stacked { 165.0 + spatial_h } else { 170.0 };
        if let (true, Some(comparison)) = (self.comparing, &self.comparison) {
            self.compare_targets(comparison.clone(), left, available, height as f32);
        } else if let Some(compiled) = &self.compiled {
            let projected_scene = self.dynamics.as_ref().map_or(&compiled.scene, |d| d.scene());
            let bounds = projected_scene.bounds;
            let scale = ((spatial_w - 24.0) / bounds.size.w.max(1.0))
                .min((spatial_h - 24.0) / bounds.size.h.max(1.0))
                .min(1.0);
            for (index, item) in projected_scene.items.iter().enumerate() {
                let instance = InstanceId(index as u32);
                let Some(occurrence) = compiled.occurrence_by_instance.get(&instance) else {
                    continue;
                };
                let label = compiled
                    .labels
                    .get(&instance)
                    .cloned()
                    .unwrap_or_else(|| occurrence.clone());
                let footprint = item
                    .footprint
                    .bounds()
                    .ok_or("Preview item has no extent")?;
                let rect = [
                    left + 12.0
                        + (item.transform.translate.x + footprint.origin.x - bounds.origin.x)
                            * scale,
                    160.0
                        + (item.transform.translate.y + footprint.origin.y - bounds.origin.y)
                            * scale,
                    footprint.size.w * scale,
                    footprint.size.h * scale,
                ];
                let source = &projected_scene.sources[item.source.0 as usize];
                let values = self
                    .dataset
                    .occurrences
                    .iter()
                    .find(|item| &item.occurrence_id == occurrence)
                    .map(|item| &item.values);
                let number = |field: &str| match values.and_then(|values| values.get(field)) {
                    Some(ProjectionValue::Number(value)) => value.to_string(),
                    _ => "?".into(),
                };
                let source_label = if source.id.chars().count() > 20 {
                    format!("{}...", source.id.chars().take(17).collect::<String>())
                } else {
                    source.id.clone()
                };
                let detail = format!(
                    "{} · {}={} · {}={}",
                    source_label,
                    self.fields[0],
                    number(&self.fields[0]),
                    self.fields[1],
                    number(&self.fields[1])
                );
                let selected = compiled.selected == Some(instance);
                self.targets.push(Target {
                    occurrence: occurrence.clone(),
                    view: "spatial",
                    label: label.clone(),
                    detail: detail.clone(),
                    rect,
                    selected,
                });
                self.targets.push(Target {
                    occurrence: occurrence.clone(),
                    view: "list",
                    label,
                    detail,
                    rect: [
                        list_x,
                        list_y + index as f32 * 78.0,
                        if stacked { available } else { 230.0 },
                        68.0,
                    ],
                    selected,
                });
            }
        }
        let status = if !self.error.is_empty() {
            format!("Cannot execute: {}", self.error)
        } else if self.comparing {
            format!("Preview step limit: {}", self.settle_bound.map_or_else(|| "unset".into(), |bound| bound.to_string()))
        } else {
            format!("{} · {} occurrences", self.axes, self.dataset.occurrences.len())
        };
        self.scene = paint(
            &self.targets,
            &self.title,
            &status,
            width,
            height,
            text_system,
            host_sheet,
        )?;
        self.dirty = false;
        self.sync_semantics()?;
        Ok(self.scene.clone())
    }

    /// Page through the complete matrix without shrinking labels or recomputing
    /// any settled cell. All cards retain the facet's shared content scale.
    fn compare_targets(&mut self, comparison: Comparison, left: f32, available: f32, height: f32) {
        let scene = &comparison.scene;
        let columns = comparison.facet.columns.labels.len();
        let rows = comparison.facet.rows.as_ref().map_or(1, |axis| axis.labels.len());
        self.compare_column = self.compare_column.min(columns.saturating_sub(1));
        self.compare_row = self.compare_row.min(rows.saturating_sub(1));
        let shown_columns = ((available / 220.0) as usize).max(1)
            .min(columns.saturating_sub(self.compare_column).max(1));
        let gap = 12.0;
        let card_w = ((available - gap * (shown_columns - 1) as f32) / shown_columns as f32).min(280.0);
        let bottom = if self.extent.0 < 720 { 260.0 } else { 112.0 };
        let room = (height - bottom - 174.0).max(120.0);
        let card_h = room.min(190.0);
        let shown_rows = ((room + gap) / (card_h + gap)) as usize;
        let shown_rows = shown_rows.max(1).min(rows.saturating_sub(self.compare_row).max(1));
        self.targets.push(Target {
            occurrence: "page".into(), view: "compare-heading",
            label: format!("Arrangements {}-{} / {}  ·  rows {}-{} / {}",
                self.compare_column + 1, self.compare_column + shown_columns, columns,
                self.compare_row + 1, self.compare_row + shown_rows, rows),
            detail: String::new(), rect: [left, 146.0, available, 24.0], selected: false,
        });
        for cell in &comparison.cells {
            if cell.column < self.compare_column || cell.column >= self.compare_column + shown_columns
                || cell.row < self.compare_row || cell.row >= self.compare_row + shown_rows { continue; }
            let x = left + (cell.column - self.compare_column) as f32 * (card_w + gap);
            let y = 174.0 + (cell.row - self.compare_row) as f32 * (card_h + gap);
            let row = comparison.facet.rows.as_ref().and_then(|axis| axis.labels.get(cell.row));
            let label = format!("{}{}{}", cell.family,
                if cell.working { " (working)" } else { "" },
                row.map_or(String::new(), |row| format!("  ·  {row}")));
            let mut detail = self.dynamics_comparison.as_ref()
                .and_then(|d| d.stops.iter().find(|(id, _)| id == &cell.swatch_id))
                .map(|(_, stop)| stop.clone())
                .unwrap_or_else(|| if cell.working { "Your draft".into() } else { "Pick to apply".into() });
            if cell.working {
                if let Some(current) = self.dynamics_comparison.as_ref()
                    .and_then(|d| d.working_detail(self.motion_paused)) {
                    detail = current;
                }
            }
            self.targets.push(Target { occurrence: cell.swatch_id.clone(), view: "compare",
                label, detail, rect: [x, y, card_w, card_h], selected: cell.working });
            let Some(frame) = scene.items.iter().find(|item| {
                let source = &scene.sources[item.source.0 as usize];
                source.adapter == FACET_CELL_ADAPTER && source.id == cell.swatch_id
            }) else { continue; };
            let frame_size = frame.footprint.bounds().unwrap().size;
            let factor = ((card_w - 12.0) / frame_size.w).min((card_h - 76.0).max(1.0) / frame_size.h);
            let origin_x = x + (card_w - frame_size.w * factor) / 2.0;
            let origin_y = y + 34.0;
            let frame_x = frame.transform.translate.x - frame_size.w / 2.0;
            let frame_y = frame.transform.translate.y - frame_size.h / 2.0;
            for item in &scene.items {
                let mut space = Some(item.space);
                let mut belongs = false;
                while let Some(id) = space {
                    let Some(s) = scene.spaces.get(id.0 as usize) else { break; };
                    if s.name.as_deref() == Some(&format!("cell: {}", cell.swatch_id)) { belongs = true; break; }
                    space = s.parent;
                }
                if !belongs { continue; }
                let Some(footprint) = item.footprint.bounds() else { continue; };
                let world = scene.to_world(item.space).unwrap_or(sceno::Transform2::IDENTITY).then(&item.transform);
                let rect = [origin_x + (world.translate.x + footprint.origin.x * world.scale - frame_x) * factor,
                    origin_y + (world.translate.y + footprint.origin.y * world.scale - frame_y) * factor,
                    footprint.size.w * world.scale * factor, footprint.size.h * world.scale * factor];
                // A focused live body may leave its frozen comparison frame.
                // Keep its paint inside the body area, away from the labels.
                if rect[0] < x || rect[1] < y + 32.0 || rect[0] + rect[2] > x + card_w
                    || rect[1] + rect[3] > y + card_h - 40.0 { continue; }
                let source = &scene.sources[item.source.0 as usize];
                self.targets.push(Target { occurrence: format!("{}:{}", source.adapter, source.id),
                    view: "compare-item", label: String::new(), detail: String::new(), rect, selected: false });
            }
        }
        // Refused cells occupy their actual matrix coordinates and explain why.
        for row in self.compare_row..self.compare_row + shown_rows {
            for column in self.compare_column..self.compare_column + shown_columns {
                if comparison.cells.iter().any(|cell| cell.row == row && cell.column == column) { continue; }
                let family = &comparison.facet.columns.labels[column];
                let row_label = comparison.facet.rows.as_ref().and_then(|axis| axis.labels.get(row));
                let key = row_label.map_or(family.clone(), |label| format!("{family} / {label}"));
                let reason = comparison.refused.iter().find(|(id, _)| id == &key || id == family)
                    .map_or("Unavailable for this input", |(_, reason)| reason.as_str());
                self.targets.push(Target { occurrence: format!("refused:{column}:{row}"), view: "compare-refused",
                    label: row_label.map_or(family.clone(), |label| format!("{family} / {label}")),
                    detail: format!("Refused: {reason}"),
                    rect: [left + (column - self.compare_column) as f32 * (card_w + gap),
                        174.0 + (row - self.compare_row) as f32 * (card_h + gap), card_w, card_h], selected: false });
            }
        }
    }

    fn sync_semantics(&self) -> Result<(), String> {
        let document = document()?;
        let container = element("projection-live")?;
        container
            .remove_attribute("hidden")
            .map_err(|_| "Could not expose preview")?;
        // Keep the actual focused button alive across selection changes.
        let pressable: Vec<&Target> = self
            .targets
            .iter()
            .filter(|target| !matches!(target.view, "compare-item" | "compare-heading"))
            .collect();
        // Rebuild when the set of targets changes, not only their number: a
        // picked comparison cell renames cells while their count holds.
        let expected = pressable
            .iter()
            .fold(0xcbf2_9ce4_8422_2325u64, |hash, target| {
                target_id(target).bytes().fold(hash, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
                })
            })
            .to_string();
        if container.get_attribute("data-target-count").as_deref() != Some(&expected) {
            container.set_text_content(None);
            for target in &pressable {
                let button = document
                    .create_element("button")
                    .map_err(|_| "Could not create preview target")?;
                if target.view == "compare-refused" { button.set_attribute("disabled", "").map_err(|_| "Refusal state")?; }
                button
                    .set_attribute("type", "button")
                    .map_err(|_| "Target type")?;
                let pick = if matches!(target.view, "compare" | "compare-refused") {
                    "data-projection-compare-cell"
                } else {
                    "data-projection-occurrence"
                };
                button
                    .set_attribute(pick, &target.occurrence)
                    .map_err(|_| "Target occurrence")?;
                button
                    .set_attribute("data-projection-view", target.view)
                    .map_err(|_| "Target view")?;
                button
                    .set_attribute("id", &format!("gs-{}", target_id(target)))
                    .map_err(|_| "Target identity")?;
                container
                    .append_child(&button)
                    .map_err(|_| "Could not append preview target")?;
            }
            container
                .set_attribute("data-target-count", &expected)
                .map_err(|_| "Target count")?;
        }
        for target in &pressable {
            let button = element(&target_id(target))?;
            button.set_text_content(Some(&format!("{} · {}", target.label, target.detail)));
            button
                .set_attribute(
                    "aria-label",
                    &format!(
                        "{} view: {} · occurrence {}",
                        target.view, format!("{} · {}", target.label, target.detail), target.occurrence
                    ),
                )
                .map_err(|_| "Target label")?;
            button
                .set_attribute(
                    "aria-pressed",
                    if target.selected { "true" } else { "false" },
                )
                .map_err(|_| "Target selection")?;
            let [x, y, w, h] = target.rect;
            button
                .set_attribute(
                    "style",
                    &format!("left:{x}px;top:{y}px;width:{w}px;height:{h}px"),
                )
                .map_err(|_| "Target bounds")?;
        }
        set_text(
            "projection-execution-status",
            if self.error.is_empty() {
                "Executable · spatial and list views"
            } else {
                &self.error
            },
        );
        let body = root()?;
        for (name, value) in [
            (
                "data-projection-executable",
                self.compiled.is_some().to_string(),
            ),
            (
                "data-projection-selected-occurrence",
                self.selected.clone().unwrap_or_default(),
            ),
            (
                "data-projection-occurrences",
                self.dataset.occurrences.len().to_string(),
            ),
            (
                "data-projection-compare",
                if self.comparing { "on" } else { "off" }.to_string(),
            ),
            (
                "data-projection-compare-cells",
                self.comparison
                    .as_ref()
                    .map_or(0, |comparison| comparison.cells.len())
                    .to_string(),
            ),
            (
                "data-projection-compare-refused",
                self.comparison
                    .as_ref()
                    .map_or(0, |comparison| comparison.refused.len())
                    .to_string(),
            ),
            ("data-projection-arrangement", self.arrangement.clone()),
            ("data-projection-dynamics", if self.dynamics.is_some() { "on" } else { "off" }.into()),
            ("data-projection-motion-paused", self.motion_paused.to_string()),
            ("data-projection-dynamics-ticks", self.dynamics.as_ref().map_or(0, |preview| preview.ticks()).to_string()),
            ("data-projection-compare-axis", if self.compare_dynamics { "dynamics" } else { "scope" }.into()),
            ("data-projection-step-limit", self.settle_bound.map_or_else(String::new, |bound| bound.to_string())),
        ] {
            body.set_attribute(name, &value)
                .map_err(|_| "Preview state")?;
        }
        Ok(())
    }
}

type Child = Box<dyn AnyView<(), (), GenetCtx, GenetElement>>;

fn target_id(target: &Target) -> String {
    let encoded: String = target
        .occurrence
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("projection-{}-{encoded}", target.view)
}

fn paint(
    targets: &[Target],
    title: &str,
    status: &str,
    width: u32,
    height: u32,
    text_system: &mut TextSystem,
    host_sheet: &str,
) -> Result<Scene, String> {
    let targets = targets.to_vec();
    let title = title.to_string();
    let status = status.to_string();
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let runner = GenetAppRunner::new(
        dom,
        move |_: &()| {
            let cards: Vec<Child> = targets
                .iter()
                .map(|target| {
                    let [x, y, w, h] = target.rect;
                    // A comparison cell's name is its headings'; its button
                    // keeps the full label for the accessibility tree.
                    let (title, detail) = match target.view {
                        "compare-item" => (String::new(), String::new()),
                        _ => (target.label.clone(), target.detail.clone()),
                    };
                    Box::new(
                        el(
                            "div",
                            (
                                el("div", text(title)).attr("class", "card-title"),
                                el("div", text(detail)).attr("class", "card-detail"),
                            ),
                        )
                        .attr(
                            "class",
                            match (target.view, target.selected) {
                                ("compare", true) => "preview-card compare-cell selected",
                                ("compare", false) => "preview-card compare-cell",
                                ("compare-item", _) => "preview-card compare-item",
                                ("compare-heading", _) => "preview-card compare-heading",
                                ("compare-refused", _) => "preview-card compare-refused",
                                (_, true) => "preview-card selected",
                                (_, false) => "preview-card",
                            },
                        )
                        .attr(
                            "style",
                            format!("left:{x}px;top:{y}px;width:{w}px;height:{h}px;"),
                        ),
                    ) as Child
                })
                .collect();
            el(
                "div",
                (
                    el("div", text(title.clone())).attr("class", "preview-title"),
                    el("div", text(status.clone())).attr("class", "preview-status"),
                    el("div", cards),
                ),
            )
            .attr("class", "preview-root")
        },
        (),
    );
    let preview_left = if width < 720 { 24 } else { 308 };
    let sheet = format!(
        r#"
      .preview-root {{ width:{width}px; height:{height}px; background-color:#091219; color:#dce5e8; font-family:Roboto; font-size:13px; }}
      .preview-title {{ position:absolute; left:{preview_left}px; top:108px; font-size:20px; color:#f0dfb8; }}
      .preview-status {{ position:absolute; left:{preview_left}px; top:135px; width:{}px; color:#91a9b3; font-size:11px; }}
      .preview-card {{ position:absolute; box-sizing:border-box; background-color:#172a35; border:1px solid #385565; border-radius:6px; padding:9px; overflow:hidden; }}
      .preview-card.selected {{ background-color:#304953; border:2px solid #f0c674; }}
      .card-title {{ font-size:15px; color:#f0dfb8; }}
      .compare-cell {{ background-color:transparent; padding:6px; }}
      .compare-cell .card-detail {{ position:absolute; left:6px; right:6px; bottom:6px; margin:0; font-size:11px; }}
      .compare-cell .card-title {{ font-size:12px; }}
      .compare-refused {{ background-color:#261f26; padding:6px; }}
      .compare-refused .card-title {{ font-size:12px; }}
      .compare-item {{ padding:0; border-radius:3px; }}
      .compare-heading {{ background-color:transparent; border:none; padding:2px; }}
      .compare-heading .card-title {{ font-size:12px; color:#c9d6db; }}
      .card-detail {{ font-size:10px; color:#a2bac5; margin-top:7px; }}
    "#,
        width.saturating_sub(preview_left + 24)
    );
    let dom = runner.dom();
    let dom = dom.borrow();
    genet_render::scene_from_scripted_dom_with_text_system(
        &*dom,
        &[sheet.as_str(), host_sheet],
        width,
        height,
        None,
        &Default::default(),
        text_system,
    )
    .map_err(|e| e.to_string())
}
