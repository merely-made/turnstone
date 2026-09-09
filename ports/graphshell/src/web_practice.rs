// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A working session over disclosed source objects. Product evidence enters at
//! the edge; the grammar places occurrences, Seiche moves a transient view,
//! and Cambium/Genet build retained Netrender fragments. Motion never lays out
//! text or recompiles a recipe. The browser's native controls are a keyed
//! realization of the very same Control records, including their rectangles.

use cambium::{GenetAppRunner, el, text};
use genet_render::TextSystem;
use genet_scripted_dom::ScriptedDom;
use graphshell::{
    practice_workspace::{
        ComparisonRecord, PracticeAction, PracticeRuntimeConfig, PracticeSource, PracticeView,
        PracticeWorkspace, PracticeWorkspaceSnapshot, Selection,
    },
    projection_compile::{
        CompiledProjection, ProjectionDataset, ProjectionFieldType, ProjectionOccurrence,
        ProjectionValue, compile, default_definition, refresh,
    },
    projection_editor::{Channel, ProjectionDefinition, SourceBinding},
};
use mere::canvas::{BoardItem, PhysicsBoard};
use netrender::{Scene, ScenePath, Transform};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::Closure};
use web_sys::{Element, PointerEvent};

use super::{BrowserHost, document, element, root, web_gpu::GpuPresenter, window};

const FIXTURE: &str = include_str!("../web/fixtures/woodshed-comparison.json");
const STORAGE: &str = "graphshellPracticeWorkspaceV1";
const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq)]
struct Face {
    label: String,
    detail: String,
    kind: &'static str,
    selected: bool,
    width: u32,
    height: u32,
}

struct Control {
    id: String,
    face: Face,
    rect: [f32; 4],
    command: Option<String>,
    body: Option<String>,
}

struct RetainedControl {
    face: Face,
    fragment: u64,
    element: Element,
    rect: [f32; 4],
    visible: bool,
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
    dataset: ProjectionDataset,
    definition: ProjectionDefinition,
    compiled: CompiledProjection,
    board: PhysicsBoard,
    dirty: bool,
    extent: (u32, u32),
    controls: HashMap<String, RetainedControl>,
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

fn tones(value: &serde_json::Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("label").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn dataset_for(record: &ComparisonRecord) -> ProjectionDataset {
    let mut occurrences = Vec::new();
    let relation = record.relation();
    for (id, source, label, x, y) in [
        (
            record.left.occurrence_id.clone(),
            sceno::SourceRef::new(&record.left.source.adapter, &record.left.source.id),
            record.left.label.clone(),
            0.0,
            0.0,
        ),
        (
            record.right.occurrence_id.clone(),
            sceno::SourceRef::new(&record.right.source.adapter, &record.right.source.id),
            record.right.label.clone(),
            24.0,
            0.0,
        ),
        (
            relation.id.clone(),
            sceno::SourceRef::new("graphshell.comparison", &relation.id),
            format!("Common tones: {}", tones(&record.result["shared"])),
            12.0,
            14.0,
        ),
    ] {
        occurrences.push(ProjectionOccurrence {
            occurrence_id: id.clone(),
            source,
            values: BTreeMap::from([
                ("occurrence_id".into(), ProjectionValue::Text(id)),
                ("label".into(), ProjectionValue::Text(label)),
                ("x".into(), ProjectionValue::Number(x)),
                ("y".into(), ProjectionValue::Number(y)),
            ]),
        });
    }
    ProjectionDataset {
        source: SourceBinding {
            authority: record.source.authority.clone(),
            domain: record.source.domain.clone(),
            resource: record.source.resource.clone(),
        },
        revision: record.revision.clone().into(),
        fields: BTreeMap::from([
            ("occurrence_id".into(), ProjectionFieldType::Text),
            ("label".into(), ProjectionFieldType::Text),
            ("x".into(), ProjectionFieldType::Number),
            ("y".into(), ProjectionFieldType::Number),
        ]),
        occurrences,
    }
}

impl PracticeHost {
    pub(super) fn new(json: Option<&str>) -> Result<Self, String> {
        let record =
            graphshell::practice_disclosure::parse_woodshed_comparison(json.unwrap_or(FIXTURE))?;
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
        let dataset = dataset_for(&record);
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
        let mut definition = default_definition(&dataset);
        definition.label = "Thursday practice".into();
        definition.appearance.title = "Thursday practice".into();
        definition.encoding.x = Channel::Field("x".into());
        definition.encoding.y = Channel::Field("y".into());
        let compiled = compile(&definition, &dataset).map_err(|e| format!("{e:?}"))?;
        let mut board = PhysicsBoard::new();
        board.set_pull(120.0);
        Ok(Self {
            workspace,
            dataset,
            definition,
            compiled,
            board,
            dirty: true,
            extent: (0, 0),
            controls: HashMap::new(),
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
            card_size: (164.0, 68.0),
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
        self.card_size = (164.0 * scale, 68.0 * scale);
        let stacked = width < 480;
        if stacked {
            self.card_size = (width.saturating_sub(48) as f32, 68.0);
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
            self.board.set_pull(120.0);
        }
        self.board.sync(items);
        if !self.workspace.runtime().physics_enabled {
            self.board.halt();
        }
        self.clock_ms = None;
        self.accumulator_ms = 0.0;
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
                    let compiled = refresh(&self.compiled, &definition, &self.dataset)
                        .map_err(|e| format!("{e:?}"))?;
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
                    let compiled = refresh(&self.compiled, &saved.definition, &self.dataset)
                        .map_err(|e| format!("{e:?}"))?;
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

    fn controls(&self, width: u32, height: u32) -> Vec<Control> {
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
                for occurrence in &self.dataset.occurrences {
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
                        "Drag a card; release to settle toward its slot".into(),
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

    pub(super) fn frame(
        &mut self,
        width: u32,
        height: u32,
        host_ms: f64,
        text_system: &mut TextSystem,
        gpu: &GpuPresenter,
    ) -> Result<Option<Scene>, String> {
        let started = now();
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
            return Ok(None);
        }
        let controls = self.controls(width, height);
        let mut scene = Scene::new(width, height);
        scene.push_rect(
            0.0,
            0.0,
            width as f32,
            height as f32,
            [0.10, 0.13, 0.105, 1.0],
        );
        if self.workspace.location().view == PracticeView::Relations {
            let record = self.workspace.comparison();
            let relation = record.relation();
            if let Some((rx, ry)) = self.board.position(&relation.id) {
                for id in [&record.left.occurrence_id, &record.right.occurrence_id] {
                    if let Some((x, y)) = self.board.position(id) {
                        let (x, y) = (x + self.card_size.0 * 0.5, y + self.card_size.1);
                        let (tx, ty) = (rx + self.card_size.0 * 0.5, ry);
                        let mut path = ScenePath::new();
                        path.move_to(x, y)
                            .cubic_to(x, (y + ty) * 0.5, tx, (y + ty) * 0.5, tx, ty);
                        scene.push_shape_stroked(path, [0.60, 0.70, 0.57, 1.0], 1.5);
                    }
                }
            }
        }
        let container = element("practice-controls")?;
        // Keep alternate views warm, including the actual focused controls.
        // The named disclosure admits <=12 pitches and history <=64 entries;
        // cap the cache as well so even future views cannot grow it unbounded.
        let active: std::collections::HashSet<_> = controls.iter().map(|c| c.id.as_str()).collect();
        for (id, old) in &mut self.controls {
            if old.visible && !active.contains(id.as_str()) {
                old.element
                    .set_attribute("hidden", "")
                    .map_err(|_| "Hide inactive control")?;
                old.visible = false;
                self.metrics.semantic_updates += 1;
            }
        }
        if self.controls.len() > 160 {
            let dead: Vec<_> = self
                .controls
                .iter()
                .filter(|(_, c)| !c.visible)
                .map(|(id, _)| id.clone())
                .collect();
            for id in dead {
                if let Some(old) = self.controls.remove(&id) {
                    old.element.remove();
                    gpu.release_fragment(old.fragment);
                }
            }
        }
        for control in controls {
            let old = self.controls.remove(&control.id);
            let face_changed = old.as_ref().is_none_or(|old| old.face != control.face);
            let fragment = if face_changed {
                self.metrics.layouts += 1;
                gpu.retain(
                    paint_face(&control.face, text_system)?,
                    old.as_ref().map(|old| old.fragment),
                )?
            } else {
                old.as_ref().unwrap().fragment
            };
            let element = if let Some(old) = &old {
                if !old.visible {
                    old.element
                        .remove_attribute("hidden")
                        .map_err(|_| "Show cached control")?;
                    self.metrics.semantic_updates += 1;
                }
                old.element.clone()
            } else {
                let el = document()?
                    .create_element(if control.command.is_some() {
                        "button"
                    } else {
                        "div"
                    })
                    .map_err(|_| "Workspace target creation failed")?;
                el.set_attribute("data-practice-id", &control.id)
                    .map_err(|_| "Workspace target identity")?;
                if let Some(command) = &control.command {
                    el.set_attribute("type", "button")
                        .map_err(|_| "Button type")?;
                    el.set_attribute("data-practice-command", command)
                        .map_err(|_| "Button command")?;
                }
                if let Some(body) = &control.body {
                    el.set_attribute("data-practice-body", body)
                        .map_err(|_| "Physics body identity")?;
                }
                if control.id == "selection" || control.id == "status" {
                    el.set_attribute("aria-live", "polite")
                        .map_err(|_| "Live status")?;
                }
                if control.face.kind == "heading" {
                    el.set_attribute("role", "heading")
                        .map_err(|_| "Heading role")?;
                    el.set_attribute("aria-level", "1")
                        .map_err(|_| "Heading level")?;
                }
                container
                    .append_child(&el)
                    .map_err(|_| "Workspace target attach")?;
                self.metrics.semantic_creates += 1;
                el
            };
            if face_changed {
                element.set_text_content(Some(&format!(
                    "{} {}",
                    control.face.label, control.face.detail
                )));
                element
                    .set_attribute(
                        "aria-label",
                        &format!("{} {}", control.face.label, control.face.detail),
                    )
                    .map_err(|_| "Workspace label")?;
                if control.command.is_some() {
                    element
                        .set_attribute(
                            "aria-pressed",
                            if control.face.selected {
                                "true"
                            } else {
                                "false"
                            },
                        )
                        .map_err(|_| "Workspace selection")?;
                }
                self.metrics.semantic_updates += 1;
            }
            if old.as_ref().is_none_or(|old| old.rect != control.rect) {
                let [x, y, w, h] = control.rect;
                element
                    .set_attribute(
                        "style",
                        &format!("left:{x}px;top:{y}px;width:{w}px;height:{h}px"),
                    )
                    .map_err(|_| "Workspace bounds")?;
                self.metrics.semantic_updates += 1;
            }
            scene.place_fragment(
                fragment,
                Transform::translate_2d(control.rect[0], control.rect[1]),
            );
            self.controls.insert(
                control.id,
                RetainedControl {
                    face: control.face,
                    fragment,
                    element,
                    rect: control.rect,
                    visible: true,
                },
            );
        }
        if self.focus_view {
            let id = format!("{:?}", self.workspace.location().view).to_lowercase();
            if let Some(control) = self.controls.get(&id) {
                if let Some(element) = control.element.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = element.focus();
                }
            }
            self.focus_view = false;
        }
        self.dirty = false;
        self.metrics.presents += 1;
        let elapsed = now() - started;
        self.metrics.frame_cpu_ms += elapsed;
        self.metrics.max_frame_cpu_ms = self.metrics.max_frame_cpu_ms.max(elapsed);
        self.publish()?;
        Ok(Some(scene))
    }

    pub(super) fn publish(&self) -> Result<(), String> {
        let root = root()?;
        for (key, value) in [
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
                "data-practice-metrics",
                serde_json::to_string(&self.metrics).map_err(|e| e.to_string())?,
            ),
        ] {
            if root.get_attribute(key).as_deref() != Some(&value) {
                root.set_attribute(key, &value)
                    .map_err(|_| "Workspace state")?;
            }
        }
        Ok(())
    }
}

fn now() -> f64 {
    window()
        .ok()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or_else(js_sys::Date::now)
}

fn paint_face(face: &Face, text_system: &mut TextSystem) -> Result<Scene, String> {
    let model = face.clone();
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let runner = GenetAppRunner::new(
        dom,
        move |_: &()| {
            el::<_, (), ()>(
                "div",
                (
                    el("div", text(model.label.clone())).attr("class", "label"),
                    el("div", text(model.detail.clone())).attr("class", "detail"),
                ),
            )
            .attr(
                "class",
                format!(
                    "face {} {}",
                    model.kind,
                    if model.selected { "selected" } else { "" }
                ),
            )
        },
        (),
    );
    let sheet = format!(
        r#"
      .face {{ width:{}px;height:{}px;box-sizing:border-box;color:#e4eadc;font-family:Roboto;font-size:14px;overflow:hidden; }}
      .button {{background-color:#29352a;border:1px solid #53624a;border-radius:5px;padding:8px;}}
      .card {{background-color:#263426;border:1px solid #627255;border-radius:6px;padding:9px;}}
      .selected {{background-color:#3a4c30;border:2px solid #bdd59e;}}
      .card .label {{font-size:18px;}}
      .detail {{font-size:12px;color:#b5c2ac;margin-top:5px;}}
      .heading .label {{font-size:27px;color:#e8ecd9;}}
      .heading .detail {{font-size:14px;}}
      .text .label {{font-size:17px;}}
      .quiet {{font-size:12px;color:#bdc9b5;padding-top:5px;}}
    "#,
        face.width, face.height
    );
    let dom = runner.dom();
    let dom = dom.borrow();
    genet_render::scene_from_scripted_dom_with_text_system(
        &*dom,
        &[sheet.as_str()],
        face.width,
        face.height,
        None,
        &Default::default(),
        text_system,
    )
    .map_err(|e| e.to_string())
}

impl BrowserHost {
    pub(super) fn practice_command(&mut self, command: &str) {
        if let Some(practice) = &mut self.practice {
            if let Err(error) = practice.command(command) {
                practice.status = format!("Could not complete action: {error}");
                practice.dirty = true;
            }
        }
    }
}

pub(super) fn install(state: &Rc<RefCell<BrowserHost>>) -> Result<(), String> {
    if state.borrow().practice.is_none() {
        return Ok(());
    }
    element("graphshell-canvas")?
        .set_attribute("aria-hidden", "true")
        .map_err(|_| "Canvas semantics")?;
    element("graphshell-canvas")?
        .remove_attribute("tabindex")
        .map_err(|_| "Canvas focus")?;
    element("semantic-host")?
        .set_attribute("hidden", "")
        .map_err(|_| "Hide fixture chrome")?;
    let container = document()?
        .create_element("section")
        .map_err(|_| "Practice controls")?;
    container.set_id("gs-practice-controls");
    container
        .set_attribute("aria-label", "Thursday practice workspace")
        .map_err(|_| "Workspace name")?;
    root()?
        .append_child(&container)
        .map_err(|_| "Mount workspace")?;
    for phase in [
        "pointerdown",
        "pointermove",
        "pointerup",
        "pointercancel",
        "lostpointercapture",
    ] {
        let state = state.clone();
        let listener = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let mut host = state.borrow_mut();
            let (px, py) = host.pointer_position(event.client_x(), event.client_y());
            let Some(practice) = &mut host.practice else {
                return;
            };
            if phase == "pointerdown" {
                if event.button() != 0 || !practice.workspace.runtime().physics_enabled {
                    return;
                }
                let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };
                let Some(id) = target.get_attribute("data-practice-body") else {
                    return;
                };
                let Some((x, y)) = practice.board.position(&id) else {
                    return;
                };
                if practice.board.drag_start(&id) {
                    practice.dragging = Some((id, px - x, py - y));
                    let _ = target.set_pointer_capture(event.pointer_id());
                    practice.dirty = true;
                }
            } else if phase == "pointermove" {
                if let Some((_, dx, dy)) = &practice.dragging {
                    // The visible workspace is the drag boundary; source slots
                    // and schema coordinates are unchanged by this clamp.
                    practice.board.drag_move(
                        (px - dx).clamp(
                            8.0,
                            (practice.extent.0 as f32 - practice.card_size.0 - 8.0).max(8.0),
                        ),
                        (py - dy).clamp(
                            216.0,
                            (practice.extent.1 as f32 - 180.0 - practice.card_size.1).max(216.0),
                        ),
                    );
                    practice.dirty = true;
                }
            } else if practice.dragging.take().is_some() {
                practice.board.drag_end();
                practice.dirty = true;
            }
        });
        root()?
            .add_event_listener_with_callback(phase, listener.as_ref().unchecked_ref())
            .map_err(|_| "Practice pointer listener")?;
        listener.forget();
    }
    // Browser reduced-motion is a host preference, rather than a new recipe
    // meaning. Explicit saved/workspace motion controls remain available.
    if root()?.get_attribute("data-reduced-motion").as_deref() == Some("true") {
        let mut host = state.borrow_mut();
        if let Some(practice) = &mut host.practice {
            practice.command("motion")?;
        }
    }
    Ok(())
}
