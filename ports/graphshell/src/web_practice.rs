// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A working session over disclosed source objects. Product evidence enters at
//! the edge; the grammar places occurrences, Seiche moves a transient view,
//! and the retained Cambium view realizes the same keyed Control records. An axis the recipe
//! encodes from a data field is pinned on that axis, so motion never moves a
//! card off its value there (dynamics grammar plan, G7, F28). Motion never
//! lays out text or recompiles a recipe. The browser's native controls are a
//! keyed realization of the very same Control records, including their
//! rectangles.

use super::window;
use graphshell::{
    practice_disclosure::tone_labels as tones,
    practice_workspace::{
        PracticeAction, PracticeRuntimeConfig, PracticeSource, PracticeView, PracticeWorkspace,
        PracticeWorkspaceSnapshot, Selection,
    },
    projection_compile::{PRACTICE_CARD, default_definition, practice_compiler},
};
use mere::canvas::{ArrangementAction, Axes, BoardItem, PermittedActions, PhysicsBoard};
use scenograph::{Channel, ProjectionDefinition};
use scenomise::{host_dataset::HostDatasetV1, projection::CompiledProjection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const STORAGE: &str = "graphshellPracticeWorkspaceV1";
const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Face {
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) kind: &'static str,
    pub(crate) selected: bool,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) struct Control {
    pub(crate) id: String,
    pub(crate) face: Face,
    pub(crate) rect: [f32; 4],
    pub(crate) command: Option<String>,
    pub(crate) body: Option<String>,
}

#[derive(Default, Serialize)]
struct Metrics {
    frames: u64,
    presents: u64,
    compiles: u64,
    placement_solves: u64,
    placement_reuses: u64,
    layouts: u64,
    physics_ticks: u64,
    semantic_creates: u64,
    semantic_updates: u64,
    frame_cpu_ms: f64,
    max_frame_cpu_ms: f64,
}

#[derive(Serialize, Deserialize)]
struct SavedPractice {
    workspace: PracticeWorkspaceSnapshot,
    definition: ProjectionDefinition,
}

pub(super) struct PracticeHost {
    workspace: PracticeWorkspace,
    /// The host dataset the page supplied (S1): the comparison's dataset and
    /// any relationships its source disclosed.
    host: HostDatasetV1,
    definition: ProjectionDefinition,
    compiled: CompiledProjection,
    board: PhysicsBoard,
    dirty: bool,
    extent: (u32, u32),
    metrics: Metrics,
    status: String,
    saved: bool,
    drawer: bool,
    history_start: usize,
    dragging: Option<(String, f32, f32)>,
    clock_ms: Option<f64>,
    accumulator_ms: f64,
    card_size: (f32, f32),
    selected_tone: Option<String>,
    focus_view: bool,
}

/// The axes `definition` encodes from data fields, declared from the
/// encoding rather than inferred from positions.
fn encoded_axes(definition: &ProjectionDefinition) -> Axes {
    let field = |channel: &Channel| matches!(channel, Channel::Field(f) if !f.trim().is_empty());
    Axes {
        x: field(&definition.encoding.x),
        y: field(&definition.encoding.y),
    }
}

impl PracticeHost {
    /// Open the workspace on the page's host dataset, which must carry one
    /// Woodshed comparison; there is no built-in fallback.
    pub(super) fn new(supplied: Option<Result<HostDatasetV1, String>>) -> Result<Self, String> {
        let host = supplied.ok_or(
            "The practice workspace needs a host dataset: data-dataset-src or ?dataset=",
        )??;
        let record = graphshell::practice_disclosure::comparison_from_host_dataset(&host)?;
        let source = PracticeSource {
            binding: record.source.clone(),
            revision: record.revision.clone(),
            occurrence_ids: [
                record.left.occurrence_id.clone(),
                record.right.occurrence_id.clone(),
            ]
            .into_iter()
            .collect(),
        };
        let workspace = PracticeWorkspace::new(
            source,
            record,
            PracticeRuntimeConfig {
                layout_id: "grid.default".into(),
                physics_enabled: true,
            },
            HISTORY_LIMIT,
        )
        .map_err(|e| format!("{e:?}"))?;
        let mut definition = default_definition(&host.dataset);
        definition.label = "Thursday practice".into();
        definition.appearance.title = "Thursday practice".into();
        definition.encoding.x = Channel::Field("x".into());
        definition.encoding.y = Channel::Field("y".into());
        let compiled = host
            .compile(practice_compiler(), &definition)
            .map_err(|e| format!("{e:?}"))?
            .projection;
        let mut board = PhysicsBoard::new();
        board.set_encoded_axes(encoded_axes(&definition));
        Ok(Self {
            workspace,
            host,
            definition,
            compiled,
            board,
            dirty: true,
            extent: (0, 0),
            metrics: Metrics {
                compiles: 1,
                placement_solves: 1,
                ..Default::default()
            },
            status: "Unsaved comparison · source objects stay in Woodshed".into(),
            saved: false,
            drawer: false,
            history_start: 0,
            dragging: None,
            clock_ms: None,
            accumulator_ms: 0.0,
            card_size: (PRACTICE_CARD.w, PRACTICE_CARD.h),
            selected_tone: None,
            focus_view: false,
        })
    }

    pub(super) fn needs_frame(&self) -> bool {
        self.dirty
            || (self.workspace.location().view == PracticeView::Relations
                && self.workspace.runtime().physics_enabled
                && self.board.is_settling())
    }

    fn sync_slots(&mut self, width: u32, height: u32) {
        let bounds = self.compiled.scene.bounds;
        let available_w = (width as f32 - 64.0).max(220.0);
        let available_h = (height as f32 - 400.0).max(190.0);
        let scale = (available_w / bounds.size.w.max(1.0))
            .min(available_h / bounds.size.h.max(1.0))
            .min(1.5);
        self.card_size = (PRACTICE_CARD.w * scale, PRACTICE_CARD.h * scale);
        let stacked = width < 480;
        if stacked {
            self.card_size = (width.saturating_sub(48) as f32, PRACTICE_CARD.h);
        }
        let origin_x = (width as f32 - bounds.size.w * scale) * 0.5;
        let origin_y = 216.0 + (available_h - bounds.size.h * scale) * 0.5;
        let items = self
            .compiled
            .scene
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let id = self.compiled.occurrence_by_instance[&sceno::InstanceId(i as u32)].clone();
                BoardItem {
                    id,
                    site: "practice".into(),
                    slot: if stacked {
                        (24.0, 240.0 + i as f32 * 86.0)
                    } else {
                        (
                            origin_x + (item.transform.translate.x - bounds.origin.x) * scale,
                            origin_y + (item.transform.translate.y - bounds.origin.y) * scale,
                        )
                    },
                }
            })
            .collect();
        if !self.workspace.runtime().physics_enabled {
            self.board = PhysicsBoard::new();
        }
        self.sync_permitted();
        self.board.set_encoded_axes(encoded_axes(&self.definition));
        self.board.sync(items);
        if !self.workspace.runtime().physics_enabled {
            self.board.halt();
        }
        self.clock_ms = None;
        self.accumulator_ms = 0.0;
    }

    /// With motion off the board withdraws drag, so a still board holds its
    /// cards; the pointer reads the board's advertisement (G9).
    fn sync_permitted(&mut self) {
        self.board
            .set_permitted_actions(if self.workspace.runtime().physics_enabled {
                PermittedActions::all()
            } else {
                PermittedActions::without(ArrangementAction::Drag)
            });
    }

    pub(super) fn command(&mut self, command: &str) -> Result<(), String> {
        let before = self.workspace.snapshot();
        let action = match command {
            "relations" => Some(PracticeAction::SelectView(PracticeView::Relations)),
            "compare" => Some(PracticeAction::SelectView(PracticeView::Compare)),
            "history" => {
                self.history_start = self.workspace.history().len().saturating_sub(4);
                Some(PracticeAction::SelectView(PracticeView::History))
            },
            "relation" => Some(PracticeAction::Select(Selection::Comparison)),
            _ => command
                .strip_prefix("select:")
                .map(|id| PracticeAction::Select(Selection::Occurrence(id.into()))),
        };
        if let Some(action) = action {
            self.workspace
                .reduce(action)
                .map_err(|e| format!("{e:?}"))?;
            self.selected_tone = None;
        } else if let Some(id) = command.strip_prefix("return:") {
            self.workspace
                .reduce(PracticeAction::ReturnTo(
                    id.parse().map_err(|_| "Invalid history entry")?,
                ))
                .map_err(|e| format!("{e:?}"))?;
            self.focus_view = true;
        } else if let Some(tone) = command.strip_prefix("tone:") {
            self.selected_tone = Some(tone.into());
            self.workspace
                .reduce(PracticeAction::Select(Selection::Comparison))
                .map_err(|e| format!("{e:?}"))?;
        } else {
            match command {
                "spaces" => self.drawer = !self.drawer,
                "older" => self.history_start = self.history_start.saturating_sub(4),
                "newer" => {
                    self.history_start = (self.history_start + 4)
                        .min(self.workspace.history().len().saturating_sub(1))
                },
                "grid" | "scatter" => {
                    let mut definition = self.definition.clone();
                    definition.arrangement.kind = format!("{command}.default");
                    let compiled = self
                        .host
                        .refresh(practice_compiler(), &self.compiled, &definition)
                        .map_err(|e| format!("{e:?}"))?
                        .projection;
                    self.workspace
                        .set_runtime(PracticeRuntimeConfig {
                            layout_id: definition.arrangement.kind.clone(),
                            physics_enabled: self.workspace.runtime().physics_enabled,
                        })
                        .map_err(|e| format!("{e:?}"))?;
                    self.definition = definition;
                    self.metrics.placement_reuses += u64::from(compiled.placement_reused);
                    self.metrics.placement_solves += u64::from(!compiled.placement_reused);
                    let placement_changed = !compiled.placement_reused;
                    self.compiled = compiled;
                    self.metrics.compiles += 1;
                    if placement_changed {
                        self.sync_slots(self.extent.0, self.extent.1);
                    }
                },
                "motion" => {
                    let enabled = !self.workspace.runtime().physics_enabled;
                    self.workspace
                        .set_runtime(PracticeRuntimeConfig {
                            layout_id: self.definition.arrangement.kind.clone(),
                            physics_enabled: enabled,
                        })
                        .map_err(|e| format!("{e:?}"))?;
                    if enabled {
                        self.sync_slots(self.extent.0, self.extent.1);
                    } else {
                        // Withdrawing drag ends one under way, as a release.
                        self.sync_permitted();
                        self.board.halt();
                        self.dragging = None;
                    }
                },
                "save" => {
                    let saved = SavedPractice {
                        workspace: self.workspace.snapshot(),
                        definition: self.definition.clone(),
                    };
                    let storage = window()?
                        .local_storage()
                        .map_err(|_| "Storage unavailable")?
                        .ok_or("Storage unavailable")?;
                    let json = serde_json::to_string(&saved).map_err(|e| e.to_string())?;
                    if json.len() > 256 * 1024 {
                        return Err("Workspace exceeds the bounded save size".into());
                    }
                    storage
                        .set_item(STORAGE, &json)
                        .map_err(|_| "Storage write failed")?;
                    self.status = "Saved comparison and workspace on this device".into();
                    self.saved = true;
                },
                "reopen" => {
                    let storage = window()?
                        .local_storage()
                        .map_err(|_| "Storage unavailable")?
                        .ok_or("Storage unavailable")?;
                    let json = storage
                        .get_item(STORAGE)
                        .map_err(|_| "Storage read failed")?
                        .ok_or("No saved practice workspace")?;
                    if json.len() > 256 * 1024 {
                        return Err("Saved workspace exceeds the bounded input size".into());
                    }
                    let saved: SavedPractice =
                        serde_json::from_str(&json).map_err(|e| e.to_string())?;
                    if saved.workspace.runtime.layout_id != saved.definition.arrangement.kind {
                        return Err("Saved layout disagrees with recipe".into());
                    }
                    let compiled = self
                        .host
                        .refresh(practice_compiler(), &self.compiled, &saved.definition)
                        .map_err(|e| format!("{e:?}"))?
                        .projection;
                    let workspace = PracticeWorkspace::reopen(
                        saved.workspace,
                        self.workspace.source().clone(),
                        self.workspace.comparison().clone(),
                        HISTORY_LIMIT,
                    )
                    .map_err(|e| format!("{e:?}"))?;
                    self.workspace = workspace;
                    self.definition = saved.definition;
                    self.metrics.placement_reuses += u64::from(compiled.placement_reused);
                    self.metrics.placement_solves += u64::from(!compiled.placement_reused);
                    self.compiled = compiled;
                    self.metrics.compiles += 1;
                    self.sync_slots(self.extent.0, self.extent.1);
                    self.status = "Reopened saved comparison, selection and history".into();
                    self.saved = true;
                    self.selected_tone = None;
                    self.focus_view = true;
                },
                _ => return Err(format!("Unknown practice action {command}")),
            }
        }
        if command != "save" && command != "reopen" && before != self.workspace.snapshot() {
            self.saved = false;
            self.status = "Workspace changed · Save to keep this view".into();
        }
        self.dirty = true;
        Ok(())
    }

    /// Each disclosed relationship as a reader hears it: its label and the
    /// two cards it joins.
    fn relations_spoken(&self) -> String {
        let label = |occurrence: &str| {
            self.compiled
                .instance_by_occurrence
                .get(occurrence)
                .and_then(|instance| self.compiled.labels.get(instance))
                .cloned()
                .unwrap_or_else(|| occurrence.to_owned())
        };
        let mut relationships: Vec<_> = self.host.relationships.iter().collect();
        relationships.sort_by(|a, b| a.id.cmp(&b.id));
        relationships
            .iter()
            .map(|relationship| {
                format!(
                    "{}: {} to {}",
                    relationship.label,
                    label(&relationship.from_occurrence),
                    label(&relationship.to_occurrence)
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub(crate) fn controls(&self, width: u32, height: u32) -> Vec<Control> {
        let w = width as f32;
        let narrow = width < 700;
        let mut controls = Vec::new();
        let mut add = |id: &str,
                       label: String,
                       detail: String,
                       kind: &'static str,
                       rect: [f32; 4],
                       command: Option<String>,
                       selected: bool,
                       body: Option<String>| {
            controls.push(Control {
                id: id.into(),
                face: Face {
                    label,
                    detail,
                    kind,
                    selected,
                    width: rect[2].ceil() as u32,
                    height: rect[3].ceil() as u32,
                },
                rect,
                command,
                body,
            });
        };
        add(
            "brand",
            "Graphshell / Woodshed".into(),
            String::new(),
            "quiet",
            [20.0, 12.0, w - 180.0, 36.0],
            None,
            false,
            None,
        );
        add(
            "spaces",
            "Spaces".into(),
            String::new(),
            "button",
            [w - 104.0, 12.0, 84.0, 36.0],
            Some("spaces".into()),
            self.drawer,
            None,
        );
        add(
            "title",
            "Thursday practice".into(),
            "What stays the same when the chord changes?".into(),
            "heading",
            [20.0, 57.0, w - 40.0, 70.0],
            None,
            false,
            None,
        );
        for (i, (name, view)) in [
            ("Relations", PracticeView::Relations),
            ("Compare", PracticeView::Compare),
            ("History", PracticeView::History),
        ]
        .into_iter()
        .enumerate()
        {
            add(
                &name.to_lowercase(),
                name.into(),
                String::new(),
                "button",
                [20.0 + i as f32 * 97.0, 134.0, 91.0, 36.0],
                Some(name.to_lowercase()),
                self.workspace.location().view == view,
                None,
            );
        }
        let action_y = if narrow { 178.0 } else { 134.0 };
        let action_x = if narrow { 20.0 } else { w - 350.0 };
        for (i, (id, label)) in [
            ("save", "Save"),
            ("reopen", "Reopen"),
            (
                "motion",
                if self.workspace.runtime().physics_enabled {
                    "Motion on"
                } else {
                    "Motion off"
                },
            ),
        ]
        .into_iter()
        .enumerate()
        {
            add(
                id,
                label.into(),
                String::new(),
                "button",
                [action_x + i as f32 * 107.0, action_y, 101.0, 36.0],
                Some(id.into()),
                id == "motion" && self.workspace.runtime().physics_enabled,
                None,
            );
        }
        let record = self.workspace.comparison();
        let relation = record.relation();
        let selection = &self.workspace.location().selection;
        match self.workspace.location().view {
            PracticeView::Relations => {
                for occurrence in &self.host.dataset.occurrences {
                    let is_relation = occurrence.occurrence_id == relation.id;
                    let subject = if occurrence.occurrence_id == record.left.occurrence_id {
                        &record.left
                    } else {
                        &record.right
                    };
                    let label = if is_relation {
                        "Common tones".into()
                    } else {
                        subject.label.clone()
                    };
                    let detail = if is_relation {
                        tones(&record.result["shared"])
                    } else {
                        tones(
                            subject
                                .disclosed
                                .get("pitch_set")
                                .unwrap_or(&serde_json::Value::Null),
                        )
                    };
                    let (x, y) = self
                        .board
                        .position(&occurrence.occurrence_id)
                        .unwrap_or((0.0, 0.0));
                    let selected = if is_relation {
                        *selection == Selection::Comparison
                    } else {
                        *selection == Selection::Occurrence(occurrence.occurrence_id.clone())
                    };
                    add(
                        &format!("object:{}", occurrence.occurrence_id),
                        label,
                        detail,
                        "card",
                        [x, y, self.card_size.0, self.card_size.1],
                        Some(if is_relation {
                            "relation".into()
                        } else {
                            format!("select:{}", occurrence.occurrence_id)
                        }),
                        selected,
                        Some(occurrence.occurrence_id.clone()),
                    );
                }
                let y = height as f32 - 173.0;
                if !self.host.relationships.is_empty() {
                    add(
                        "relations-list",
                        "Relations".into(),
                        self.relations_spoken(),
                        "quiet",
                        [20.0, y - 40.0, w - 40.0, 34.0],
                        None,
                        false,
                        None,
                    );
                }
                add(
                    "grid",
                    "Grid".into(),
                    String::new(),
                    "button",
                    [20.0, y, 75.0, 33.0],
                    Some("grid".into()),
                    self.definition.arrangement.kind == "grid.default",
                    None,
                );
                add(
                    "scatter",
                    "Scatter".into(),
                    String::new(),
                    "button",
                    [101.0, y, 88.0, 33.0],
                    Some("scatter".into()),
                    self.definition.arrangement.kind == "scatter.default",
                    None,
                );
                if width > 650 {
                    add(
                        "gesture",
                        "Drag a card; release to return it to its slot".into(),
                        String::new(),
                        "quiet",
                        [210.0, y, w - 230.0, 33.0],
                        None,
                        false,
                        None,
                    );
                }
            },
            PracticeView::Compare => {
                let mut pitches = BTreeMap::new();
                for subject in [&record.left, &record.right] {
                    for tone in subject
                        .disclosed
                        .get("pitch_set")
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                    {
                        if let (Some(pc), Some(label)) =
                            (tone["pitch_class"].as_u64(), tone["label"].as_str())
                        {
                            pitches.insert(pc, label.to_owned());
                        }
                    }
                }
                let row_w = (w - 150.0).max(160.0) / (pitches.len().max(1) as f32);
                let top = if narrow { 246.0 } else { 228.0 };
                add(
                    "matrix-title",
                    "Pitch membership".into(),
                    "Shared tones appear in both rows".into(),
                    "text",
                    [20.0, top, w - 40.0, 54.0],
                    None,
                    false,
                    None,
                );
                for (col, (pc, label)) in pitches.iter().enumerate() {
                    let x = 134.0 + col as f32 * row_w;
                    add(
                        &format!("pitch:{pc}"),
                        label.clone(),
                        String::new(),
                        "quiet",
                        [x, top + 61.0, row_w - 4.0, 28.0],
                        None,
                        false,
                        None,
                    );
                    for (row, subject) in [&record.left, &record.right].into_iter().enumerate() {
                        let present = subject
                            .disclosed
                            .get("pitch_set")
                            .and_then(|v| v.as_array())
                            .is_some_and(|set| {
                                set.iter().any(|v| v["pitch_class"].as_u64() == Some(*pc))
                            });
                        add(
                            &format!("cell:{row}:{pc}"),
                            if present { label.clone() } else { "–".into() },
                            String::new(),
                            "button",
                            [x, top + 97.0 + row as f32 * 62.0, row_w - 4.0, 52.0],
                            Some(format!("tone:{label}")),
                            present,
                            None,
                        );
                    }
                }
                for (row, subject) in [&record.left, &record.right].into_iter().enumerate() {
                    add(
                        &format!("row:{row}"),
                        subject.label.clone(),
                        String::new(),
                        "button",
                        [20.0, top + 97.0 + row as f32 * 62.0, 108.0, 52.0],
                        Some(format!("select:{}", subject.occurrence_id)),
                        *selection == Selection::Occurrence(subject.occurrence_id.clone()),
                        None,
                    );
                }
            },
            PracticeView::History => {
                let top = if narrow { 236.0 } else { 207.0 };
                if self.workspace.history().is_empty() {
                    add(
                        "empty",
                        "Your path begins here".into(),
                        "Select an object or switch views to record a step.".into(),
                        "text",
                        [20.0, top, w - 40.0, 74.0],
                        None,
                        false,
                        None,
                    );
                }
                for (row, entry) in self
                    .workspace
                    .history()
                    .iter()
                    .skip(self.history_start)
                    .take(4)
                    .enumerate()
                {
                    let label = match &entry.location.selection {
                        Selection::Comparison => "Comparison".into(),
                        Selection::Occurrence(id) => {
                            if id == &record.left.occurrence_id {
                                record.left.label.clone()
                            } else {
                                record.right.label.clone()
                            }
                        },
                    };
                    add(
                        &format!("history:{}", entry.id),
                        format!("{} · {label}", entry.id + 1),
                        format!("{:?} · Return to this view", entry.location.view),
                        "card",
                        [20.0, top + row as f32 * 67.0, w - 40.0, 59.0],
                        Some(format!("return:{}", entry.id)),
                        false,
                        None,
                    );
                }
                add(
                    "older",
                    "Earlier".into(),
                    String::new(),
                    "button",
                    [20.0, height as f32 - 174.0, 92.0, 34.0],
                    Some("older".into()),
                    false,
                    None,
                );
                add(
                    "newer",
                    "Later".into(),
                    String::new(),
                    "button",
                    [118.0, height as f32 - 174.0, 92.0, 34.0],
                    Some("newer".into()),
                    false,
                    None,
                );
            },
        }
        let (title, detail) = match selection {
            Selection::Comparison => (
                format!("{} × {}", record.left.label, record.right.label),
                format!(
                    "Shared: {}. Different: {} → {}.",
                    tones(&record.result["shared"]),
                    tones(&record.result["left_only"]),
                    tones(&record.result["right_only"])
                ),
            ),
            Selection::Occurrence(id) => {
                let subject = if id == &record.left.occurrence_id {
                    &record.left
                } else {
                    &record.right
                };
                (
                    subject.label.clone(),
                    format!(
                        "{} · Source {}",
                        tones(
                            subject
                                .disclosed
                                .get("pitch_set")
                                .unwrap_or(&serde_json::Value::Null)
                        ),
                        subject.source.id
                    ),
                )
            },
        };
        let detail = if let Some(tone) = &self.selected_tone {
            format!("Selected pitch: {tone}. {detail}")
        } else {
            detail
        };
        add(
            "selection",
            title,
            detail,
            "text",
            [20.0, height as f32 - 131.0, w - 40.0, 77.0],
            None,
            false,
            None,
        );
        add(
            "status",
            self.status.clone(),
            String::new(),
            "quiet",
            [20.0, height as f32 - 49.0, w - 40.0, 40.0],
            None,
            false,
            None,
        );
        if self.drawer {
            add(
                "space-card",
                "Thursday practice".into(),
                "Only this device · 2 source objects · 1 comparison".into(),
                "card",
                [20.0, 52.0, (w - 40.0).min(380.0), 90.0],
                Some("spaces".into()),
                true,
                None,
            );
        }
        controls
    }

    /// Advance the shared practice state on either host presentation.
    pub(crate) fn advance(&mut self, width: u32, height: u32, host_ms: f64) -> bool {
        self.metrics.frames += 1;
        if self.extent != (width, height) {
            self.extent = (width, height);
            self.sync_slots(width, height);
            self.dirty = true;
        }
        let active = self.workspace.location().view == PracticeView::Relations
            && self.workspace.runtime().physics_enabled;
        let mut moved = false;
        if active && self.board.is_settling() {
            let elapsed = self
                .clock_ms
                .map(|last| (host_ms - last).clamp(0.0, 50.0))
                .unwrap_or(1000.0 / 60.0);
            self.accumulator_ms += elapsed;
            while self.accumulator_ms >= 1000.0 / 60.0 && self.board.is_settling() {
                self.board.tick();
                self.metrics.physics_ticks += 1;
                self.accumulator_ms -= 1000.0 / 60.0;
                moved = true;
            }
        }
        self.clock_ms = Some(host_ms);
        if !self.dirty && !moved {
            return false;
        }
        self.dirty = false;
        true
    }

    /// The board owns drag permission, constraints and cancellation. Hosts only route coordinates.
    pub(crate) fn board_pointer(
        &mut self,
        id: &str,
        phase: cambium::PointerPhase,
        px: f32,
        py: f32,
    ) -> bool {
        match phase {
            cambium::PointerPhase::Down => {
                let Some((x, y)) = self.board.position(id) else {
                    return false;
                };
                if !self.board.drag_start(id) {
                    return false;
                }
                self.dragging = Some((id.into(), px - x, py - y));
            },
            cambium::PointerPhase::Move => {
                let Some((_, dx, dy)) = &self.dragging else {
                    return false;
                };
                self.board.drag_move(
                    (px - dx).clamp(
                        8.0,
                        (self.extent.0 as f32 - self.card_size.0 - 8.0).max(8.0),
                    ),
                    (py - dy).clamp(
                        216.0,
                        (self.extent.1 as f32 - 180.0 - self.card_size.1).max(216.0),
                    ),
                );
            },
            cambium::PointerPhase::Up | cambium::PointerPhase::Cancel => {
                if self.dragging.take().is_none() {
                    return false;
                }
                self.board.drag_end();
            },
        }
        self.dirty = true;
        true
    }
    /// Endpoints from the arrangement owner, shared by both renderers.
    pub(crate) fn relation_lines(&self) -> Vec<([f32; 4], bool)> {
        let mut lines = Vec::new();
        if self.workspace.location().view != PracticeView::Relations {
            return lines;
        }
        let record = self.workspace.comparison();
        if let Some((rx, ry)) = self.board.position(&record.relation().id) {
            for id in [&record.left.occurrence_id, &record.right.occurrence_id] {
                if let Some((x, y)) = self.board.position(id) {
                    lines.push((
                        [
                            x + self.card_size.0 * 0.5,
                            y + self.card_size.1,
                            rx + self.card_size.0 * 0.5,
                            ry,
                        ],
                        true,
                    ));
                }
            }
        }
        let center = |instance: &sceno::InstanceId| {
            self.compiled
                .occurrence_by_instance
                .get(instance)
                .and_then(|id| self.board.position(id))
                .map(|(x, y)| (x + self.card_size.0 * 0.5, y + self.card_size.1 * 0.5))
        };
        for relation in &self.compiled.scene.relations {
            if let (Some((fx, fy)), Some((tx, ty))) = (center(&relation.from), center(&relation.to))
            {
                lines.push(([fx, fy, tx, ty], false));
            }
        }
        lines
    }
    pub(crate) fn set_error(&mut self, error: String) {
        self.status = error;
        self.dirty = true;
    }
    pub(crate) fn fields(&self) -> Result<Vec<(&'static str, String)>, String> {
        Ok([
            (
                "data-practice-view",
                format!("{:?}", self.workspace.location().view).to_lowercase(),
            ),
            (
                "data-practice-selection",
                match &self.workspace.location().selection {
                    Selection::Comparison => "comparison".into(),
                    Selection::Occurrence(id) => id.clone(),
                },
            ),
            (
                "data-practice-history",
                self.workspace.history().len().to_string(),
            ),
            ("data-practice-saved", self.saved.to_string()),
            (
                "data-practice-layout",
                self.definition.arrangement.kind.clone(),
            ),
            (
                "data-practice-motion",
                self.workspace.runtime().physics_enabled.to_string(),
            ),
            (
                "data-practice-settling",
                self.board.is_settling().to_string(),
            ),
            (
                "data-practice-relations",
                self.compiled.scene.relations.len().to_string(),
            ),
            (
                "data-practice-encoded-drift",
                format!("{:.3}", self.board.encoded_drift()),
            ),
            (
                "data-practice-metrics",
                serde_json::to_string(&self.metrics).map_err(|e| e.to_string())?,
            ),
        ]
        .into_iter()
        .collect())
    }
}
