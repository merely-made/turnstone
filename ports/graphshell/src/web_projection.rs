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
    Comparison, FACET_AXIS_ADAPTER, FACET_CELL_ADAPTER, FacetLayout, compare_arrangements,
};
use graphshell::projection_editor::{EditorAction, ProjectionEditor, ProjectionPanel, with_kind};
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
                    targets: Vec::new(),
                    extent: (0, 0),
                    title: String::new(),
                    axes: String::new(),
                    fields: [String::new(), String::new()],
                    solves: 1,
                    placement_reuses: 0,
                    comparing: false,
                    comparison: None,
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
                compiled.map_err(|issues| {
                    issues
                        .iter()
                        .map(|i| format!("{}: {}", i.field, i.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
            });
        match result {
            Ok(mut compiled) => {
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
                live.compiled = Some(compiled);
                live.error.clear();
            },
            Err(error) => {
                live.compiled = None;
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
        live.comparing = !live.comparing;
        live.dirty = true;
        self.refresh_projection_comparison();
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }

    /// Rebuild the grid from the working draft, when it is showing.
    fn refresh_projection_comparison(&mut self) {
        let draft = self.projection_editor.draft().clone();
        let Some(live) = &mut self.live_projection else {
            return;
        };
        if !live.comparing {
            live.comparison = None;
            return;
        }
        let layout = FacetLayout {
            cell: sceno::Size2::new(220.0, 150.0),
            gap: 16.0,
            heading: 28.0,
        };
        match compare_arrangements(&draft, &live.dataset, live.selected.as_deref(), &layout) {
            Ok(comparison) => live.comparison = Some(comparison),
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
        let Some(family) = self
            .live_projection
            .as_ref()
            .and_then(|live| live.comparison.as_ref())
            .and_then(|comparison| {
                comparison
                    .cells
                    .iter()
                    .find(|cell| cell.swatch_id == swatch_id && !cell.working)
            })
            .map(|cell| cell.family.clone())
        else {
            return;
        };
        let arrangement = with_kind(
            &self.projection_editor.draft().arrangement,
            &family,
            practice_compiler().registry(),
        );
        self.projection_editor
            .reduce(EditorAction::SetArrangement(arrangement), now_ms());
        self.projection_editor.break_run();
        self.recompile_projection();
        self.projection_editor_status = format!("Arrangement · {family} (from the comparison)");
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
            practice_compiler().compile_snapshot(&saved, &live.dataset).map_err(|issues| {
                issues
                    .iter()
                    .map(|i| format!("{}: {}", i.field, i.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            })?;
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
        model.physics_law = "fixed placement".into();
        model.physics_paused = true;
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
    ) -> Result<Scene, String> {
        if !self.dirty && self.extent == (width, height) {
            return Ok(self.scene.clone());
        }
        self.extent = (width, height);
        self.targets.clear();
        let left = 308.0;
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
            let bounds = compiled.scene.bounds;
            let scale = ((spatial_w - 24.0) / bounds.size.w.max(1.0))
                .min((spatial_h - 24.0) / bounds.size.h.max(1.0))
                .min(1.0);
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
                let source = &compiled.scene.sources[item.source.0 as usize];
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
        let status = if self.error.is_empty() {
            format!(
                "{} occurrences · source {} · selection shared across both views",
                self.dataset.occurrences.len(),
                self.dataset.revision
            )
        } else {
            format!("Cannot execute: {}", self.error)
        };
        self.scene = paint(
            &self.targets,
            &self.title,
            &format!("{} · {status}", self.axes),
            width,
            height,
            text_system,
        )?;
        self.dirty = false;
        self.sync_semantics()?;
        Ok(self.scene.clone())
    }

    /// The comparison grid as cards: a frame per cell (its heading in the
    /// title) and its items inside, scaled to the preview's space.
    fn compare_targets(&mut self, comparison: Comparison, left: f32, available: f32, height: f32) {
        let scene = &comparison.scene;
        let bounds = scene.bounds;
        // Headings keep a readable size however small the grid draws: a
        // margin for row labels, and a strip above the columns.
        let row_margin = if comparison.facet.rows.is_some() {
            64.0
        } else {
            0.0
        };
        let left = left + row_margin;
        let available = available - row_margin;
        let scale = ((available - 24.0) / bounds.size.w.max(1.0))
            .min((height - 200.0).max(160.0) / bounds.size.h.max(1.0))
            .min(1.0);
        let place = |x: f32, y: f32, w: f32, h: f32| {
            [
                left + 12.0 + (x - bounds.origin.x) * scale,
                160.0 + (y - bounds.origin.y) * scale,
                w * scale,
                h * scale,
            ]
        };
        let row_label = |row: usize| {
            comparison
                .facet
                .rows
                .as_ref()
                .and_then(|axis| axis.labels.get(row).cloned())
        };
        for item in &scene.items {
            let source = &scene.sources[item.source.0 as usize];
            let Some(footprint) = item.footprint.bounds() else {
                continue;
            };
            let world = scene
                .to_world(item.space)
                .unwrap_or(sceno::Transform2::IDENTITY)
                .then(&item.transform);
            let rect = place(
                world.translate.x + footprint.origin.x * world.scale,
                world.translate.y + footprint.origin.y * world.scale,
                footprint.size.w * world.scale,
                footprint.size.h * world.scale,
            );
            if source.adapter == FACET_AXIS_ADAPTER {
                // Headings name the columns and rows, outside the cells.
                let label = source
                    .id
                    .split_once(':')
                    .and_then(|(axis, index)| {
                        let index: usize = index.parse().ok()?;
                        match axis {
                            "columns" => comparison.facet.columns.labels.get(index).cloned(),
                            "rows" => row_label(index),
                            _ => None,
                        }
                    })
                    .unwrap_or_default();
                let [x, y, w, h] = rect;
                let rect = if source.id.starts_with("rows:") {
                    [x - row_margin, y, row_margin - 6.0 + w, h.max(18.0)]
                } else {
                    [x, y + h - 18.0, w, 18.0]
                };
                self.targets.push(Target {
                    occurrence: source.id.clone(),
                    view: "compare-heading",
                    label,
                    detail: String::new(),
                    rect,
                    selected: false,
                });
                continue;
            }
            if source.adapter == FACET_CELL_ADAPTER {
                let Some(cell) = comparison.cells.iter().find(|c| c.swatch_id == source.id) else {
                    continue;
                };
                let label = match (cell.working, row_label(cell.row)) {
                    (true, Some(row)) => format!("{} (working) · {row}", cell.family),
                    (true, None) => format!("{} (working)", cell.family),
                    (false, Some(row)) => format!("{} · {row}", cell.family),
                    (false, None) => cell.family.clone(),
                };
                self.targets.push(Target {
                    occurrence: cell.swatch_id.clone(),
                    view: "compare",
                    label,
                    detail: if cell.working {
                        "Your draft".into()
                    } else {
                        "Pick to apply".into()
                    },
                    rect,
                    selected: cell.working,
                });
            } else {
                self.targets.push(Target {
                    occurrence: format!("{}:{}", source.adapter, source.id),
                    view: "compare-item",
                    label: String::new(),
                    detail: String::new(),
                    rect,
                    selected: false,
                });
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
                button
                    .set_attribute("type", "button")
                    .map_err(|_| "Target type")?;
                let pick = if target.view == "compare" {
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
                        target.view, target.label, target.occurrence
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
                        "compare" | "compare-item" => (String::new(), String::new()),
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
    let sheet = format!(
        r#"
      .preview-root {{ width:{width}px; height:{height}px; background-color:#091219; color:#dce5e8; font-family:Roboto; font-size:13px; }}
      .preview-title {{ position:absolute; left:308px; top:91px; font-size:23px; color:#f0dfb8; }}
      .preview-status {{ position:absolute; left:308px; top:126px; width:{}px; color:#91a9b3; font-size:11px; }}
      .preview-card {{ position:absolute; box-sizing:border-box; background-color:#172a35; border:1px solid #385565; border-radius:6px; padding:9px; overflow:hidden; }}
      .preview-card.selected {{ background-color:#304953; border:2px solid #f0c674; }}
      .card-title {{ font-size:15px; color:#f0dfb8; }}
      .compare-cell {{ background-color:transparent; padding:4px; }}
      .compare-cell .card-title {{ font-size:12px; }}
      .compare-item {{ padding:0; border-radius:3px; }}
      .compare-heading {{ background-color:transparent; border:none; padding:2px; }}
      .compare-heading .card-title {{ font-size:12px; color:#c9d6db; }}
      .card-detail {{ font-size:10px; color:#a2bac5; margin-top:7px; }}
    "#,
        width.saturating_sub(335)
    );
    let dom = runner.dom();
    let dom = dom.borrow();
    genet_render::scene_from_scripted_dom_with_text_system(
        &*dom,
        &[sheet.as_str()],
        width,
        height,
        None,
        &Default::default(),
        text_system,
    )
    .map_err(|e| e.to_string())
}
