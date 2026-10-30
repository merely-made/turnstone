// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native scene state, held as Graphshell holds it. The canvas's saved scene
//! (`Canvas::capture_scene`: dynamics spec, selection, camera, pause and the
//! cartography geometry) rides one `graphshell.saved-scene/v4` facet on the
//! graph's container node in `facets.json`; the rest of the view (folds, the
//! command menu) is a `pandect::ViewIntent` in `view-intent.json`. A session
//! saved before this (`arrangement.*` and `scene.*` facets, a flat
//! `view-intent.json`, `folds.json`) is read once, carried into the same shapes
//! and rewritten by the next save.

use std::io;
use std::path::Path;

use mere::canvas::physics_controls::FREE_ARRANGEMENT;
use mere::canvas::saved_scene::{self, SceneReadError};
use mere::canvas::{
    Canvas, PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay,
};
use pandect::{NodeFacetStore, SavedSceneV3, ViewIntent};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::app::GraphRuntimePool;
use crate::panes::GraphId;

/// The fold sidecar this replaced; read once, then removed by the save that
/// holds its records.
const LEGACY_FOLDS_FILE: &str = "folds.json";
const WORKING_SCENE: &str = "Turnstone working scene";

pub(crate) fn scope(graph: GraphId) -> String {
    format!("turnstone:graph:{}", graph.as_uuid())
}

/// What a runtime holds about the files its view reads and writes.
#[derive(Default)]
pub(crate) struct ViewState {
    /// An unreadable or interrupted view-intent file: saves leave its bytes
    /// intact.
    pub(crate) read_only: bool,
    /// The saved scene this build refused to read: the next save leaves the
    /// facet alone rather than replacing what a newer host wrote.
    scene_refused: bool,
    /// Fold records the canvas did not take (another scope, a newer version,
    /// a member since gone), kept as written.
    retained: Vec<Value>,
    /// `folds.json` held records now in the view intent.
    folds_file_migrated: bool,
}

impl ViewState {
    #[cfg(test)]
    pub(crate) fn retained_len(&self) -> usize {
        self.retained.len()
    }
}

/// The flat view intent this host wrote before the saved scene: read to be
/// carried into one.
#[derive(Default, Deserialize)]
#[serde(default)]
struct LegacyIntent {
    layout_strategy: Option<String>,
    physics_law: Option<String>,
    physics_paused: Option<bool>,
    physics_overlays: Vec<String>,
    physics_kind_source: Option<String>,
    physics_mass_source: Option<String>,
    physics_depth_source: Option<String>,
    command_menu: cambium::CommandChoices,
}

const LEGACY_KEYS: [&str; 8] = [
    "layout_strategy",
    "physics_law",
    "physics_paused",
    "physics_overlays",
    "physics_kind_source",
    "physics_mass_source",
    "physics_depth_source",
    "command_menu",
];

/// What opening a session's view found.
pub(crate) struct OpenedView {
    pub(crate) state: ViewState,
    pub(crate) commands: cambium::CommandChoices,
    legacy: Option<LegacyIntent>,
}

/// Read the view-intent file (and a leftover `folds.json`), restoring the
/// canvas's folds. A missing or malformed file never prevents the session
/// from opening.
pub(crate) fn open_view(dir: &Path, graph: GraphId, canvas: &mut Canvas) -> OpenedView {
    canvas.restore_folds(vec![]);
    let mut opened = OpenedView {
        state: ViewState::default(),
        commands: Default::default(),
        legacy: None,
    };
    let path = crate::session::view_intent_path(dir);
    let mut raw_folds: Vec<Value> = Vec::new();
    match read_object(&path) {
        Ok(None) => {},
        Ok(Some(mut object)) => {
            if LEGACY_KEYS.iter().any(|key| object.contains_key(*key)) {
                match serde_json::from_value::<LegacyIntent>(Value::Object(object)) {
                    Ok(legacy) => {
                        opened.commands = legacy.command_menu.clone();
                        opened.legacy = Some(legacy);
                    },
                    Err(error) => {
                        tracing::warn!(%error, path = ?path, "failed to parse view intent");
                        opened.state.read_only = true;
                    },
                }
            } else {
                if let Some(Value::Array(records)) = object.remove("folds") {
                    raw_folds.extend(records);
                }
                // Kept as written, never taken up again on a later open.
                if let Some(Value::Array(records)) = object.remove("retained_folds") {
                    opened.state.retained.extend(records);
                }
                match serde_json::from_value::<ViewIntent>(Value::Object(object)) {
                    Ok(read) => opened.commands = read.commands.unwrap_or_default(),
                    Err(error) => {
                        tracing::warn!(%error, path = ?path, "failed to parse view intent");
                        opened.state.read_only = true;
                    },
                }
            }
        },
        Err(error) => {
            tracing::warn!(%error, path = ?path, "view intent preserved unread");
            opened.state.read_only = true;
        },
    }
    if !opened.state.read_only {
        match legacy_folds(dir) {
            Ok(Some((records, refused))) => {
                raw_folds.extend(records);
                opened.state.retained.extend(refused);
                opened.state.folds_file_migrated = true;
            },
            Ok(None) => {},
            Err(error) => {
                tracing::warn!(%error, "legacy fold sidecar left in place unread");
            },
        }
    }
    let mut records = Vec::new();
    for raw in raw_folds {
        match serde_json::from_value::<curation::FoldRecord>(raw.clone()) {
            Ok(record) if record.source_scope == scope(graph) && record.validate().is_ok() => {
                records.push(record)
            },
            _ => opened.state.retained.push(raw),
        }
    }
    opened.state.retained.extend(
        canvas
            .restore_folds(records)
            .into_iter()
            .map(|record| json!(record)),
    );
    if !opened.state.retained.is_empty() {
        tracing::warn!(
            count = opened.state.retained.len(),
            "saved folds refused; records retained intact"
        );
    }
    opened
}

/// A JSON object read from `path`: `None` when absent, an error when it is
/// interrupted, unreadable or not an object.
fn read_object(path: &Path) -> io::Result<Option<serde_json::Map<String, Value>>> {
    if !crate::session_persistence::require_complete_file(path)? {
        return Ok(None);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    match serde_json::from_slice(&bytes) {
        Ok(Value::Object(object)) => Ok(Some(object)),
        Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "not an object")),
        Err(error) => Err(io::Error::new(io::ErrorKind::InvalidData, error)),
    }
}

/// The records `folds.json` holds, and the ones it had already refused.
fn legacy_folds(dir: &Path) -> io::Result<Option<(Vec<Value>, Vec<Value>)>> {
    let Some(object) = read_object(&dir.join(LEGACY_FOLDS_FILE))? else {
        return Ok(None);
    };
    let records = object.get("records").and_then(Value::as_array).cloned();
    let refused = object.get("refused_records");
    if object.get("version") != Some(&json!(1))
        || records.is_none()
        || refused.is_some_and(|refused| !refused.is_array())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unreadable or future fold envelope",
        ));
    }
    Ok(Some((
        records.unwrap_or_default(),
        refused
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    )))
}

/// Write the view intent: the canvas's folds, the records it could not take
/// and the command menu. Built whole from the live state, so every save is
/// the same save.
pub(crate) fn save_view(
    dir: &Path,
    canvas: &Canvas,
    commands: &cambium::CommandChoices,
    state: &ViewState,
) -> Result<(), String> {
    if state.read_only {
        return Err("unreadable view intent preserved".into());
    }
    let intent = ViewIntent {
        folds: canvas.fold_records().to_vec(),
        retained_folds: state.retained.clone(),
        commands: (commands != &cambium::CommandChoices::default()).then(|| commands.clone()),
        ..ViewIntent::default()
    };
    let bytes = serde_json::to_vec_pretty(&intent).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let target = crate::session::view_intent_path(dir);
    let tmp = target.with_extension("json.tmp");
    let result = (|| -> io::Result<()> {
        std::fs::write(&tmp, &bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&tmp, &target)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.to_string());
    }
    if state.folds_file_migrated {
        let _ = std::fs::remove_file(dir.join(LEGACY_FOLDS_FILE));
    }
    Ok(())
}

/// Capture `canvas` as its graph's saved scene, on `container`. A scene this
/// build refused to read is left as it was written.
pub(crate) fn stage_scene(canvas: &mut Canvas, container: Uuid, state: &ViewState) {
    if state.scene_refused {
        return;
    }
    let arrangement = canvas
        .layout_strategy()
        .unwrap_or(FREE_ARRANGEMENT)
        .to_owned();
    match canvas.capture_scene(WORKING_SCENE, &arrangement, "") {
        Ok(scene) => {
            if let Err(error) = crate::graph_facets::edit_canvas_facets(canvas, |facets| {
                pandect::write_saved_scene(facets, container, &scene)
            }) {
                tracing::warn!(%error, "saved scene facet edit was refused");
            }
        },
        Err(error) => tracing::warn!(%error, "scene capture refused; the saved scene is unchanged"),
    }
}

/// The saved scene on `container`, if there is one.
pub(crate) fn read_scene(
    facets: &NodeFacetStore,
    container: Uuid,
) -> Option<Result<SavedSceneV3, SceneReadError>> {
    pandect::stored_scene(facets, container).map(saved_scene::read_scene)
}

/// What restoring a scene did.
pub(crate) struct Restored {
    /// Positions came from a saved scene, for the first frame to adopt.
    pub(crate) positions: bool,
    /// Whether the scene saved physics paused; `None` with no scene.
    pub(crate) paused: Option<bool>,
    /// The refusals met applying a session's earlier physics choice, in
    /// order, for the host to record.
    pub(crate) refusals: Vec<Result<(), mere::canvas::OverlayRefusal>>,
}

impl Restored {
    fn none(refusals: Vec<Result<(), mere::canvas::OverlayRefusal>>) -> Self {
        Self {
            positions: false,
            paused: None,
            refusals,
        }
    }
}

/// Dress the active canvas (its graph already adopted) with the saved scene
/// on `container`, or with the scene a session saved before it held, which is
/// carried into one and its old facets cleared. The host records the
/// arrangement as computed on its first frame.
pub(crate) fn restore_scene(
    pool: &mut GraphRuntimePool,
    container: Option<Uuid>,
    opened: &mut OpenedView,
) -> Restored {
    let mut refusals = Vec::new();
    let scene = match container.and_then(|c| read_scene(pool.facets(), c)) {
        Some(Ok(scene)) => Some(scene),
        Some(Err(error)) => {
            tracing::warn!(
                ?error,
                "saved scene refused; the session opens on its defaults"
            );
            opened.state.scene_refused = true;
            None
        },
        None => carry_legacy(pool, container, opened.legacy.as_ref(), &mut refusals),
    };
    let Some(scene) = scene else {
        return Restored::none(refusals);
    };
    if let Err(error) = pool.apply_scene(&scene, None) {
        tracing::warn!(%error, "saved scene refused; the session opens on its defaults");
        opened.state.scene_refused = true;
        return Restored::none(refusals);
    }
    Restored {
        positions: !scene.cartography.is_empty(),
        paused: Some(scene.physics_paused),
        refusals,
    }
}

/// A session saved before the saved scene, as one: its `arrangement.*` and
/// `scene.*` facets and flat view intent, read once. The physics choice is
/// applied as it always was (an id this build does not know falls back to
/// the default, an overlay the law refuses is recorded) and the scene is
/// captured from the canvas that results.
fn carry_legacy(
    pool: &mut GraphRuntimePool,
    container: Option<Uuid>,
    legacy: Option<&LegacyIntent>,
    refusals: &mut Vec<Result<(), mere::canvas::OverlayRefusal>>,
) -> Option<SavedSceneV3> {
    let facets = pool.facets();
    let scene_facets = container
        .filter(|container| pandect::has_scene_facets(facets, *container))
        .map(|container| pandect::read_scene_facets(facets, container));
    let mut geometry = pandect::read_arrangement_geometry(facets, container);
    if legacy.is_none() && scene_facets.is_none() && geometry.is_empty() {
        return None;
    }
    if let Some(scene) = &scene_facets {
        geometry = geometry
            .with_size_by_degree(scene.size_by_degree)
            .with_size_by_importance(scene.size_by_importance)
            .with_importance_metric(scene.importance_metric.clone());
    }
    let default = LegacyIntent::default();
    let legacy = legacy.unwrap_or(&default);
    if let Some(source) = legacy
        .physics_kind_source
        .as_deref()
        .and_then(PhysicsKindSource::parse)
    {
        pool.set_physics_kind_source(source);
    }
    if let Some(source) = legacy
        .physics_mass_source
        .as_deref()
        .and_then(PhysicsMassSource::parse)
    {
        pool.set_physics_mass_source(source);
    }
    if let Some(source) = legacy
        .physics_depth_source
        .as_deref()
        .and_then(PhysicsDepthSource::parse)
    {
        pool.set_physics_depth_source(source);
    }
    if let Some(law) = legacy.physics_law.as_deref().and_then(PhysicsLaw::parse) {
        refusals.push(pool.set_physics_law(law));
    }
    refusals.push(
        pool.set_physics_overlays(
            legacy
                .physics_overlays
                .iter()
                .filter_map(|id| PhysicsOverlay::parse(id))
                .collect(),
        ),
    );
    pool.set_physics_damping(
        scene_facets
            .as_ref()
            .map_or(pandect::DEFAULT_PHYSICS_DAMPING, |scene| {
                scene.physics_damping
            }),
    );
    let arrangement = legacy
        .layout_strategy
        .clone()
        .unwrap_or_else(|| FREE_ARRANGEMENT.into());
    let captured = pool.capture_scene(WORKING_SCENE, &arrangement, "");
    let mut scene = match captured {
        Ok(scene) => scene,
        Err(error) => {
            tracing::warn!(%error, "the earlier scene state could not be carried; it is left in place");
            return None;
        },
    };
    scene.cartography = geometry;
    if let Some(paused) = legacy.physics_paused {
        scene.physics_paused = paused;
    }
    if let Err(error) = pool.edit_facets(|facets| {
        pandect::clear_arrangement(facets, container);
        if let Some(container) = container {
            pandect::clear_scene_facets(facets, container);
        }
    }) {
        tracing::warn!(%error, "saved scene facet carry was refused");
        return None;
    }
    Some(scene)
}
