// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The persistence port: where turnstone's data lives and how sessions
//! save/load. Multi-session since rung 6's second half: each session owns
//! `sessions/<id>/` (graph.json, frame.json, workbench.json,
//! browser_nodes.json, windows.json, manifest.json); the manifest set is
//! pandect's `ManifestStore`, and the flat single-session layout
//! this port started on migrates in on first boot.

use std::path::{Path, PathBuf};

use crate::panes::{FrisketLayout, SessionId};
use crate::place::{PlaceBindingError, PlaceBindingV1};
use image::ImageEncoder;
// The frame-sidecar store is frisket's own since meerkat's deletion (it moved
// out of pandect with the pane model).
use crate::panes::store as frisket_store;
use mere::kernel::graph::Graph;
use pandect::{GraphSessionManifest, ManifestStore, session_graph_store};
use sceno::Score;

/// Copy the displayed component across Surface and Resource relations. The
/// supplier's older cross-graph helper walks only raw Surface edges. Fresh
/// Surface identities retain copy provenance; Resource identities, records and
/// held assertions travel verbatim within this component. The result is quiet.
pub(crate) fn copy_session_component(
    source: &Graph,
    seed: uuid::Uuid,
    source_graph: Option<String>,
) -> Result<(Graph, mere::kernel::graph::ComponentCopy), String> {
    use std::collections::{HashMap, HashSet};
    let members = source.component_members(seed, &[]);
    let mut graph = Graph::new();
    let mut copy = mere::kernel::graph::ComponentCopy::default();
    let mut remap = HashMap::new();
    let mut resources = HashSet::new();
    for member in members {
        let (key, node) = source.get_node_by_id(member).ok_or("component member disappeared")?;
        if let Some(resource) = source.shown_resource_id(key) {
            resources.insert(resource);
        }
        let copied = graph.copy_node_from_xy(node, source_graph.clone(), 0.0, 0.0);
        let id = graph.get_node(copied).ok_or("copied member disappeared")?.id;
        copy.new_keys.push(copied);
        copy.id_remap.push((member, id));
        remap.insert(member.to_string(), id.to_string());
    }
    // Preserve unshown Resource intermediates and tag concepts too. This is
    // graph closure, with no fetches or changes to Keep/subscription policy.
    let mut neighbors: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for (from, to, _) in source.resource_edges() {
        neighbors.entry(from.id()).or_default().push(to.id());
        neighbors.entry(to.id()).or_default().push(from.id());
    }
    let mut pending: Vec<_> = resources.iter().copied().collect();
    while let Some(resource) = pending.pop() {
        for neighbor in neighbors.get(&resource).into_iter().flatten() {
            if resources.insert(*neighbor) {
                pending.push(*neighbor);
            }
        }
    }
    let original = source.to_snapshot();
    let mut snapshot = graph.to_snapshot();
    snapshot.edges = original.edges.into_iter().filter_map(|mut edge| {
        edge.from_node_id = remap.get(&edge.from_node_id)?.clone();
        edge.to_node_id = remap.get(&edge.to_node_id)?.clone();
        Some(edge)
    }).collect();
    snapshot.resources = original.resources.into_iter().filter(|record| {
        resources.contains(&chartulary::resource_id_from_canonical_iri(&record.canonical_iri))
    }).collect();
    snapshot.resource_edges = original.resource_edges.into_iter().filter(|edge| {
        edge.from_node_id.parse().is_ok_and(|id| resources.contains(&id))
            && edge.to_node_id.parse().is_ok_and(|id| resources.contains(&id))
    }).collect();
    snapshot.shown_resources = original.shown_resources.into_iter().filter_map(|mut shown| {
        shown.surface_id = remap.get(&shown.surface_id)?.clone();
        Some(shown)
    }).collect();
    let facets = graph.facets().clone();
    let mut graph = Graph::try_from_recorded_snapshot(&snapshot).map_err(|error| error.to_string())?;
    graph.overlay_facets(facets);
    Ok((graph, copy))
}

/// The per-user data root (`<data_dir>/turnstone`). A `TURNSTONE_ROOT` override
/// points the whole root at a scratch profile, so a headed-verification run
/// (or any throwaway session) isolates from the real per-user data dir (the
/// meerkat `MERE_ROOT` convention).
pub fn default_turnstone_root() -> PathBuf {
    if let Some(root) = std::env::var_os("TURNSTONE_ROOT") {
        return PathBuf::from(root);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("turnstone")
}

/// The sessions directory under the data root: one subdirectory per session,
/// named by its uuid (ManifestStore's own layout).
pub fn sessions_root(data_root: &Path) -> PathBuf {
    data_root.join("sessions")
}

/// One session's directory: where ALL its sidecars live.
pub fn session_dir(data_root: &Path, id: SessionId) -> PathBuf {
    sessions_root(data_root).join(id.as_uuid().to_string())
}

/// The current-session marker (`<root>/current_session`, the bare uuid): the
/// session a restart reopens. Best-effort, like every sidecar.
const CURRENT_SESSION_FILE: &str = "current_session";

pub fn record_current_session(data_root: &Path, id: SessionId) {
    let path = data_root.join(CURRENT_SESSION_FILE);
    if let Err(err) = std::fs::write(&path, id.as_uuid().to_string()) {
        tracing::warn!(%err, "failed to record the current session");
    }
}

/// The session a boot should open: the recorded current session when it
/// still exists, else the most recently updated manifest, else `None` (a
/// fresh install — the caller mints one).
pub fn pick_session(data_root: &Path, store: &ManifestStore) -> Option<SessionId> {
    let recorded = std::fs::read_to_string(data_root.join(CURRENT_SESSION_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<uuid::Uuid>().ok())
        .map(SessionId::from_uuid)
        .filter(|id| store.get(*id).is_some());
    recorded.or_else(|| {
        store
            .iter()
            .max_by_key(|(_, m)| m.updated_at)
            .map(|(id, _)| id)
    })
}

/// Load the manifest set from `sessions/`. Failures are logged per directory
/// (the store's own report), never fatal.
pub fn load_manifests(data_root: &Path) -> ManifestStore {
    let mut store = ManifestStore::new();
    match store.load_from_disk(sessions_root(data_root)) {
        Ok(report) => {
            for failure in &report.failed {
                tracing::warn!(
                    dir = %failure.dir_name,
                    reason = %failure.reason,
                    "a session manifest failed to load"
                );
            }
        }
        Err(err) => tracing::warn!(%err, "failed to read the sessions directory"),
    }
    store
}

/// The sidecar files a session owns (the flat layout's file set, and each
/// session directory's).
const PROJECTION_SCORE_FILE: &str = "projection-score.json";
const VIEW_INTENT_FILE: &str = "view-intent.json";
pub const PLACE_FILE: &str = "place.json";
pub const PLACE_COLLECTION_FILE: &str = "place-collection.json";
/// A local "this session left" mark, beside `place.json` rather than a field
/// on it: `PlaceBindingV1` is the wire shape invitations and cards also
/// serialize, and "left" is a fact local to this sidecar, never something to
/// carry onto the wire. Its presence, not the binding's absence, is what
/// `Left` reads on.
pub const PLACE_LEFT_FILE: &str = "place-left.json";

const SESSION_FILES: [&str; 9] = [
    session_graph_store::GRAPH_FILE,
    frisket_store::FRAME_FILE,
    WORKBENCH_FILE,
    pandect::browser_node_state::BROWSER_NODES_FILE,
    frisket_store::WINDOWS_FILE,
    PROJECTION_SCORE_FILE,
    VIEW_INTENT_FILE,
    PLACE_FILE,
    PLACE_COLLECTION_FILE,
];

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceCollectionSidecar {
    version: u32,
    #[serde(deserialize_with = "required_collection_selection")]
    selection: Option<crate::place::PlaceCollectionVersion>,
}

// An explicit null clears the choice; an omitted field is corrupt, not a
// request to broaden the scope. Serde otherwise defaults missing Options.
fn required_collection_selection<'de, D>(
    deserializer: D,
) -> Result<Option<crate::place::PlaceCollectionVersion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer)
}

/// Missing means the original all-effective default. An unreadable saved
/// choice must not silently broaden the search on restart.
pub(crate) fn load_place_collection(
    directory: &Path,
) -> Result<Option<crate::place::PlaceCollectionVersion>, String> {
    let bytes = match std::fs::read(directory.join(PLACE_COLLECTION_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read saved collection choice: {error}")),
    };
    let saved: PlaceCollectionSidecar = serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode saved collection choice: {error}"))?;
    if saved.version != 1 {
        return Err(format!(
            "unsupported collection choice version {}",
            saved.version
        ));
    }
    Ok(saved.selection)
}

/// The place worker writes its local choice before acknowledging it. Atomic
/// replacement keeps a failed write from losing the previously saved scope.
pub(crate) fn save_place_collection(
    directory: &Path,
    selection: Option<&crate::place::PlaceCollectionVersion>,
) -> Result<(), String> {
    use std::io::Write;
    let saved = PlaceCollectionSidecar {
        version: 1,
        selection: selection.cloned(),
    };
    let bytes = serde_json::to_vec_pretty(&saved)
        .map_err(|error| format!("encode collection choice: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| format!("stage collection choice: {error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("write collection choice: {error}"))?;
    temporary
        .persist(directory.join(PLACE_COLLECTION_FILE))
        .map_err(|error| format!("replace collection choice: {}", error.error))?;
    Ok(())
}

/// A strict `place.json` read or write failure. Unlike optional view sidecars,
/// an invalid binding must remain visible because silently treating it as a
/// personal session would change the session's authority model.
#[derive(Debug)]
pub enum PlaceSidecarError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Binding(PlaceBindingError),
    UnsupportedVersion(u16),
}

impl std::fmt::Display for PlaceSidecarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "place sidecar I/O: {error}"),
            Self::Json(error) => write!(formatter, "place sidecar JSON: {error}"),
            Self::Binding(error) => write!(formatter, "place sidecar binding: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported place-left marker version {version}")
            },
        }
    }
}

impl std::error::Error for PlaceSidecarError {}

impl From<std::io::Error> for PlaceSidecarError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for PlaceSidecarError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<PlaceBindingError> for PlaceSidecarError {
    fn from(error: PlaceBindingError) -> Self {
        Self::Binding(error)
    }
}

pub fn place_binding_path(session_dir: &Path) -> PathBuf {
    session_dir.join(PLACE_FILE)
}

/// Persist the public binding through an adjacent temporary file. Group
/// secrets, welcome material, and live rendezvous state have no representation
/// in this sidecar.
pub fn save_place_binding(
    session_dir: &Path,
    binding: &PlaceBindingV1,
) -> Result<(), PlaceSidecarError> {
    binding.validate()?;
    std::fs::create_dir_all(session_dir)?;
    let target = place_binding_path(session_dir);
    let temporary = target.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(binding)?;
    if let Err(error) = (|| -> std::io::Result<()> {
        std::fs::write(&temporary, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&temporary, &target)
    })() {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

/// Update a binding that admission already established.
///
/// Returns `Ok(false)` when none is present. Routine session saves use this
/// rather than [`save_place_binding`] so that saving can never *mint* a
/// `place.json` for a session that was never admitted. Creating one is
/// admission's job alone, and keeping that structural means the ordering
/// survives someone adding a new save path later.
pub fn update_place_binding(
    session_dir: &Path,
    binding: &PlaceBindingV1,
) -> Result<bool, PlaceSidecarError> {
    if !place_binding_path(session_dir).exists() {
        return Ok(false);
    }
    save_place_binding(session_dir, binding)?;
    Ok(true)
}

/// Remove the local place binding after the worker has released its handles.
/// Retained place stores and shared membership records are left untouched.
pub fn remove_place_binding(session_dir: &Path) -> Result<bool, PlaceSidecarError> {
    // Remove contact metadata first. A failure retains the binding and remains
    // visible; a subsequent binding-removal failure still permits offline use.
    match std::fs::remove_file(session_dir.join(crate::place::rendezvous::RENDEZVOUS_FILE)) {
        Ok(()) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(error.into()),
    }
    match std::fs::remove_file(place_binding_path(session_dir)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Load and validate this session's public shared-place binding. Absence means
/// a personal session; malformed or unsupported content is an explicit error.
pub fn load_place_binding(session_dir: &Path) -> Result<Option<PlaceBindingV1>, PlaceSidecarError> {
    let path = place_binding_path(session_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let binding: PlaceBindingV1 = serde_json::from_slice(&bytes)?;
    binding.validate()?;
    Ok(Some(binding))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceLeftMarkerV1 {
    version: u16,
    graph_nodes: usize,
    chat_messages: usize,
    members: usize,
}

const PLACE_LEFT_MARKER_VERSION: u16 = 1;

/// Mark this session as having locally left its place: write a small marker
/// beside `place.json` recording the retained counts at the moment of
/// leaving. The binding, the rendezvous hints, and every retained store are
/// left exactly as they were -- `Rejoin` needs all three, and the marker's
/// presence alone is what `Left` reads on, not the binding's absence.
pub fn mark_place_left(
    session_dir: &Path,
    retained: &crate::place::PlaceLeftSummary,
) -> Result<(), PlaceSidecarError> {
    std::fs::create_dir_all(session_dir)?;
    let target = session_dir.join(PLACE_LEFT_FILE);
    let temporary = target.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&PlaceLeftMarkerV1 {
        version: PLACE_LEFT_MARKER_VERSION,
        graph_nodes: retained.graph_nodes,
        chat_messages: retained.chat_messages,
        members: retained.members,
    })?;
    if let Err(error) = (|| -> std::io::Result<()> {
        std::fs::write(&temporary, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&temporary, &target)
    })() {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

/// The retained counts a local leave recorded, or `None` when this session
/// was not left (the ordinary case, and what a fresh reconnect leaves it as
/// once `Rejoin` clears the mark).
pub fn load_place_left(
    session_dir: &Path,
) -> Result<Option<crate::place::PlaceLeftSummary>, PlaceSidecarError> {
    let bytes = match std::fs::read(session_dir.join(PLACE_LEFT_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let marker: PlaceLeftMarkerV1 = serde_json::from_slice(&bytes)?;
    if marker.version != PLACE_LEFT_MARKER_VERSION {
        return Err(PlaceSidecarError::UnsupportedVersion(marker.version));
    }
    Ok(Some(crate::place::PlaceLeftSummary {
        graph_nodes: marker.graph_nodes,
        chat_messages: marker.chat_messages,
        members: marker.members,
    }))
}

/// Clear the left mark, so this session reads as ordinarily joined again.
/// Returns `false` when there was none to clear.
pub fn clear_place_left(session_dir: &Path) -> Result<bool, PlaceSidecarError> {
    match std::fs::remove_file(session_dir.join(PLACE_LEFT_FILE)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// The persisted product-free score for this session's active analytic view.
pub fn projection_score_path(session_dir: &Path) -> PathBuf {
    session_dir.join(PROJECTION_SCORE_FILE)
}

/// Persist a score atomically. It is view state: a missing or malformed score
/// must never prevent the graph session itself from opening.
pub fn save_projection_score(session_dir: &Path, score: &Score) {
    let target = projection_score_path(session_dir);
    let tmp = target.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(session_dir)?;
        let bytes = serde_json::to_vec_pretty(score).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&tmp, &target)
    })();
    if let Err(err) = result {
        tracing::warn!(%err, path = ?target, "failed to persist projection score");
        let _ = std::fs::remove_file(tmp);
    }
}

/// Restore the last valid score. A corrupt sidecar is diagnosed and ignored;
/// the canvas will recompute a fresh score from current graph truth.
pub fn load_projection_score(session_dir: &Path) -> Option<Score> {
    let path = projection_score_path(session_dir);
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(score) => Some(score),
            Err(err) => {
                tracing::warn!(%err, path = ?path, "failed to parse projection score");
                None
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            tracing::warn!(%err, path = ?path, "failed to read projection score");
            None
        }
    }
}

/// What the viewer *asked for*, as against the score, which is what the
/// solver produced.
///
/// The distinction is the reason this is its own sidecar: a score can be
/// recomputed from graph truth at any time, but "I chose Board" cannot be
/// recovered from the positions it produced, so reopening a session without it
/// silently reverts the arrangement to the surface default. The strategy id is
/// the persistence key, never the display name, so an arrangement rename
/// leaves stored sessions untouched.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewIntentV1 {
    /// The active analytic arrangement, or `None` for the surface's native
    /// force-directed one.
    #[serde(default)]
    pub layout_strategy: Option<String>,
    /// The physics law id (`mere::canvas::PhysicsLaw::id`); `None` reads as
    /// Springs, the canvas default, so a session saved before the catalog
    /// reopens as it always did. (Physics catalog — P2.)
    #[serde(default)]
    pub physics_law: Option<String>,
    /// The overlay ids composed onto the law, in run order.
    #[serde(default)]
    pub physics_overlays: Vec<String>,
    /// The kind / mass / depth source ids; `None` is each catalog's default.
    #[serde(default)]
    pub physics_kind_source: Option<String>,
    #[serde(default)]
    pub physics_mass_source: Option<String>,
    #[serde(default)]
    pub physics_depth_source: Option<String>,
    /// The person's command menu (Scenograph editor plan SE28 to SE31):
    /// commands kept beyond the defaults, defaults dropped, and the recent
    /// ones, stored as Cambium's own choices (SE45), so the JSON is the one
    /// every host writes. View state like the rest of this sidecar, never
    /// graph truth.
    #[serde(default)]
    pub command_menu: cambium::CommandChoices,
}

pub fn view_intent_path(session_dir: &Path) -> PathBuf {
    session_dir.join(VIEW_INTENT_FILE)
}

/// Persist view intent atomically. Like the score, it is view state: a missing
/// or malformed sidecar must never prevent the session from opening.
pub fn save_view_intent(session_dir: &Path, intent: &ViewIntentV1) {
    let target = view_intent_path(session_dir);
    let tmp = target.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(session_dir)?;
        let bytes = serde_json::to_vec_pretty(intent).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&tmp, &target)
    })();
    if let Err(err) = result {
        tracing::warn!(%err, path = ?target, "failed to persist view intent");
        let _ = std::fs::remove_file(tmp);
    }
}

/// Restore the last valid view intent. A corrupt sidecar is diagnosed and
/// ignored, leaving the session on its default arrangement.
pub fn load_view_intent(session_dir: &Path) -> Option<ViewIntentV1> {
    let path = view_intent_path(session_dir);
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(intent) => Some(intent),
            Err(err) => {
                tracing::warn!(%err, path = ?path, "failed to parse view intent");
                None
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            tracing::warn!(%err, path = ?path, "failed to read view intent");
            None
        }
    }
}

/// One-time migration from the flat single-session layout: when a flat
/// `graph.json` sits at the root and no session holds anything yet, mint a
/// session, MOVE the flat sidecars into its directory, and write its
/// manifest. Returns the minted id when a migration ran. Best-effort per
/// file (a copy that fails logs and stays put — the graph file moving is
/// what the migration is judged by).
pub fn migrate_flat_layout(data_root: &Path, store: &mut ManifestStore) -> Option<SessionId> {
    if !store.is_empty() {
        return None;
    }
    let flat_graph = data_root.join(session_graph_store::GRAPH_FILE);
    if !flat_graph.exists() {
        return None;
    }
    let id = SessionId::new();
    let dir = session_dir(data_root, id);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::warn!(%err, "flat-layout migration could not create the session dir");
        return None;
    }
    for file in SESSION_FILES {
        let from = data_root.join(file);
        if !from.exists() {
            continue;
        }
        if let Err(err) = std::fs::rename(&from, dir.join(file)) {
            tracing::warn!(%err, %file, "flat-layout migration failed to move a sidecar");
            if file == session_graph_store::GRAPH_FILE {
                return None;
            }
        }
    }
    let mut manifest = GraphSessionManifest::new(id, crate::panes::GraphId::nil());
    manifest.storage_path = Some(dir);
    store.insert(manifest);
    if let Err(err) = store.flush_dirty() {
        tracing::warn!(%err, "flat-layout migration failed to write the manifest");
    }
    tracing::info!(session = %id.as_uuid(), "flat single-session layout migrated to sessions/");
    Some(id)
}

/// A loaded graph carries only the placement declared at its input boundary.
pub(crate) struct LoadedSessionGraph {
    pub graph: Graph,
    pub placement: Option<pandect::graph_placement::PlacementProfile>,
}

/// Missing inputs are fresh; corrupt or unreadable inputs refuse persistence.
/// Read canonical facets before any migration write or image deposition.
pub(crate) fn try_load_session_graph(data_root: &Path) -> Result<Option<LoadedSessionGraph>, String> {
    // Pandect's compatibility reader uses exists(); distinguish inspection
    // failure here before it can silently treat an unreadable path as absent.
    crate::session_persistence::require_complete_file(&pandect::node_facets_path(data_root))
        .map_err(|error| format!("canonical facets could not be inspected: {error}"))?;
    let canonical = pandect::load_node_facets(data_root)
        .map_err(|error| format!("canonical facets could not be read: {error}"))?
        .unwrap_or_default();
    let graph_file = data_root.join(session_graph_store::GRAPH_FILE);
    crate::session_persistence::require_complete_file(&graph_file)
        .map_err(|error| format!("session graph replacement is incomplete: {error}"))?;
    let Some(mut input) = session_graph_store::load_profiled_snapshot(&graph_file)
        .map_err(|error| format!("session graph could not be read: {error}"))? else {
        return Ok(None);
    };
    let placement = input.placement;
    let snapshot = &mut input.snapshot;
    // Validate the untouched source before stripping old controls or depositing blobs.
    pandect::graph_placement::materialize_snapshot(snapshot, placement)?;
    let legacy_node_facets = snapshot.legacy_node_facet_count();
    let legacy_controls = crate::surface_controls::extract_legacy(snapshot);
    let controls_migrated = !legacy_controls.is_empty();
    let migrated = externalize_legacy_images(snapshot, data_root)
        .map_err(|error| format!("session imagery could not be migrated: {error}"))?;
    let mut graph = pandect::graph_placement::materialize_snapshot(snapshot, placement)?;
    graph.overlay_facets(legacy_controls);
    graph.overlay_facets(canonical);
    if legacy_node_facets > 0 || controls_migrated || migrated > 0 {
        // Evidence first, and propagate failure so the display runtime cannot
        // overwrite the source later through an automatic ordinary save.
        pandect::save_node_facets(data_root, graph.facets())
            .map_err(|error| format!("migration facets could not be persisted: {error}"))?;
        session_graph_store::save_profiled(&graph_file, &graph, placement)
            .map_err(|error| format!("migrated graph could not be persisted: {error}"))?;
    }
    Ok(Some(LoadedSessionGraph { graph, placement }))
}

/// Read-only compatibility for consumers that need a graph without adopting
/// session persistence authority. The application uses the fallible loader.
pub fn load_session_graph(data_root: &Path) -> Option<Graph> {
    match try_load_session_graph(data_root) {
        Ok(input) => input.map(|input| input.graph),
        Err(error) => { tracing::warn!(%error, "session graph load refused"); None }
    }
}

/// Move a legacy snapshot's inline image bytes into the session blob
/// directory, leaving references on the nodes. Returns how many blobs were
/// written.
///
/// The file-sidecar counterpart of `pandect::image_store::
/// migrate_legacy_images`, which needs an eidetic `Store` this host does not
/// have. Same digest and same `<hex>` key, so the two agree.
///
/// The legacy favicon is raw RGBA with no container; encode it once here so
/// every durable image blob has the same PNG format.
fn externalize_legacy_images(
    snapshot: &mut mere::kernel::persistence::GraphSnapshot,
    data_root: &Path,
) -> std::io::Result<usize> {
    use mere::kernel::types::{ImageRef, ImageRole};

    let mut written = 0usize;
    for node in &mut snapshot.nodes {
        if let Some(png) = node.legacy_thumbnail_png.take() {
            let digest = *eidetic::Hash::of(&png).as_bytes();
            let image = ImageRef::new(
                digest,
                node.legacy_thumbnail_width,
                node.legacy_thumbnail_height,
            );
            try_save_image_blob(data_root, &image.hex(), &png)?;
            node.images.insert(ImageRole::Preview, image);
            node.legacy_thumbnail_width = 0;
            node.legacy_thumbnail_height = 0;
            written += 1;
        }
        if let Some(rgba) = node.legacy_favicon_rgba.take() {
            let Some(png) =
                encode_rgba_png(&rgba, node.legacy_favicon_width, node.legacy_favicon_height)
            else {
                node.legacy_favicon_rgba = Some(rgba);
                continue;
            };
            let digest = *eidetic::Hash::of(&png).as_bytes();
            let image = ImageRef::new(
                digest,
                node.legacy_favicon_width,
                node.legacy_favicon_height,
            );
            try_save_image_blob(data_root, &image.hex(), &png)?;
            node.images.insert(ImageRole::Favicon, image);
            node.legacy_favicon_width = 0;
            node.legacy_favicon_height = 0;
            written += 1;
        }
    }
    debug_assert_eq!(
        snapshot.legacy_image_count(),
        0,
        "every legacy image must be externalized before the snapshot materializes"
    );
    Ok(written)
}

/// Encode decoded straight-alpha RGBA8 pixels into the one durable image
/// format used by the sidecar store.
pub(crate) fn encode_rgba_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if width == 0 || height == 0 || rgba.len() != expected {
        return None;
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(png)
}

/// Persist the session graph at the flat `graph.json`. Best-effort: a write
/// failure is logged, not fatal. Run after each enrichment (so a crash loses
/// nothing) and on close.
pub fn save_session_graph(data_root: &Path, graph: &Graph) {
    let graph_file = data_root.join(session_graph_store::GRAPH_FILE);
    if let Err(err) = session_graph_store::save(&graph_file, graph) {
        tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
    }
}

/// The session's image-blob directory: `<session>/images/<hex>`.
///
/// After the node-image externalization the graph carries ~40-byte references
/// and the pixels live out of line. Turnstone's session persistence is
/// file-sidecar shaped (`graph.json`, `facets.json`), so its blob store is a
/// directory of the same shape rather than the eidetic-backed
/// `pandect::image_store` — same content-addressed `<hex>` key, so the
/// two converge cleanly if this session ever gains a real store.
fn images_dir(data_root: &Path) -> std::path::PathBuf {
    data_root.join("images")
}

/// Persist one image blob under its digest hex. Best-effort, like the graph:
/// a lost favicon re-fetches, so a write failure is logged, not fatal.
pub fn save_image_blob(data_root: &Path, hex: &str, bytes: &[u8]) {
    if let Err(error) = try_save_image_blob(data_root, hex, bytes) {
        tracing::warn!(%error, "failed to persist an image blob");
    }
}

fn try_save_image_blob(data_root: &Path, hex: &str, bytes: &[u8]) -> std::io::Result<()> {
    let dir = images_dir(data_root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(hex);
    if path.try_exists()? {
        if std::fs::read(&path)? == bytes { return Ok(()); }
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
            "existing content-addressed image blob has different bytes"));
    }
    std::fs::write(path, bytes)
}

/// Read one image blob back, or `None` when it is absent (swept, not yet
/// fetched, or a reference that outlived its blob).
pub fn load_image_blob(data_root: &Path, hex: &str) -> Option<Vec<u8>> {
    std::fs::read(images_dir(data_root).join(hex)).ok()
}

/// Mark/sweep the session's file-sidecar image store against every live graph
/// reference. Only 64-character hex filenames are store members; unrelated
/// files in the directory are left alone. Returns blobs actually removed.
pub fn gc_orphan_image_blobs(data_root: &Path, graph: &Graph) -> usize {
    let referenced: std::collections::HashSet<String> = graph
        .nodes()
        .flat_map(|(_, node)| node.images.values())
        .map(|image| image.hex())
        .collect();
    let Ok(entries) = std::fs::read_dir(images_dir(data_root)) else {
        return 0;
    };
    let mut dropped = 0;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let is_digest = name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit());
        if is_digest && !referenced.contains(&name) && std::fs::remove_file(entry.path()).is_ok() {
            dropped += 1;
        }
    }
    dropped
}

/// Restore the persisted per-node facet store (`facets.json`), if one exists.
/// Carries the `arrangement.position` facets (the durable canvas layout — the
/// graph itself is position-free) plus any other namespace. Missing or corrupt
/// starts empty: the canvas keeps its origin park and settles fresh.
pub fn load_node_facets(data_root: &Path) -> Option<pandect::NodeFacetStore> {
    match pandect::load_node_facets(data_root) {
        Ok(facets) => facets,
        Err(err) => {
            tracing::warn!(%err, "failed to load the facet store; starting empty");
            None
        }
    }
}

/// Persist the per-node facet store at `facets.json`. Best-effort, like the graph.
pub fn save_node_facets(data_root: &Path, facets: &pandect::NodeFacetStore) {
    if let Err(err) = pandect::save_node_facets(data_root, facets) {
        tracing::warn!(%err, "failed to persist the facet store");
    }
}

/// Restore the persisted pane layout (rung 5 slice C), if one exists. The sidecar
/// is `frame.json` beside `graph.json` (the on-disk tag stays `frame`, a parked
/// format decision). `None` starts on the default single-pane layout.
pub fn load_frisket_layout(data_root: &Path) -> Option<FrisketLayout> {
    match frisket_store::load_frisket_layout(data_root) {
        Ok(layout) => layout,
        Err(err) => {
            tracing::warn!(%err, "failed to load the pane layout; starting on the default");
            None
        }
    }
}

/// Persist the pane layout at `frame.json`. Best-effort, like the graph.
pub fn save_frisket_layout(data_root: &Path, layout: &FrisketLayout) {
    if let Err(err) = frisket_store::save_frisket_layout(data_root, layout) {
        tracing::warn!(%err, "failed to persist the pane layout");
    }
}

/// Persist the lens-window spaces at `windows.json` (rung 7 depth: windows
/// are pane hosts, so torn-out panes survive a restart AS windows). Closed
/// slots persist as `null`, keeping ordinals stable. Best-effort, like the
/// rest.
pub fn save_lens_spaces(data_root: &Path, lenses: &[Option<FrisketLayout>]) {
    if let Err(err) = frisket_store::save_lens_spaces(data_root, lenses) {
        tracing::warn!(%err, "failed to persist the lens windows");
    }
}

/// Restore the lens-window spaces. Missing or corrupt starts with none (the
/// primary window alone — the panes those windows held are gone with them,
/// honestly, not silently folded into the primary).
pub fn load_lens_spaces(data_root: &Path) -> Vec<Option<FrisketLayout>> {
    match frisket_store::load_lens_spaces(data_root) {
        Ok(Some(lenses)) => lenses,
        Ok(None) => Vec::new(),
        Err(err) => {
            tracing::warn!(%err, "failed to load the lens-window sidecar; starting with none");
            Vec::new()
        }
    }
}

/// The workbench tiling sidecar, beside `graph.json` (the meerkat convention:
/// the tiling is the graph's, so it persists with the session).
const WORKBENCH_FILE: &str = "workbench.json";

/// Persist the workbench tiling as platen's canonical `(Arrangement, geometry)`
/// pair (the live `Pane` tree is a derived cache, never serde;
/// `to_persisted_json` debug-asserts `canonical_roundtrips` — platen's
/// persistence discipline, preserved verbatim). Best-effort, like the rest.
pub fn save_workbench(data_root: &Path, workbench: &mere::platen::Workbench) {
    match workbench.to_persisted_json() {
        Ok(json) => {
            let path = data_root.join(WORKBENCH_FILE);
            if let Err(err) = std::fs::write(&path, json) {
                tracing::warn!(%err, path = ?path, "failed to persist the workbench tiling");
            }
        }
        Err(err) => tracing::warn!(%err, "failed to serialize the workbench tiling"),
    }
}

/// Repair pre-overmap manifests whose `root_graph_id` is nil: mint each a real
/// `GraphId` and flush. The root graph is the session's container node (the
/// one-node model) — its id keys the `scene.*` facets and is the session's
/// identity in the overmap, so nil ids would collide every pre-overmap session
/// onto one overmap node. Returns how many were healed; the caller migrates
/// each healed session's nil-keyed scene facets on adopt
/// (`App::adopt_session`). Idempotent: a healed store has nothing nil.
pub fn heal_nil_graph_ids(sessions: &mut ManifestStore) -> usize {
    let nil: Vec<SessionId> = sessions
        .iter()
        .filter(|(_, m)| m.root_graph_id == crate::panes::GraphId::nil())
        .map(|(id, _)| id)
        .collect();
    for id in &nil {
        sessions.update(*id, |m| m.root_graph_id = crate::panes::GraphId::new());
    }
    if !nil.is_empty() {
        if let Err(err) = sessions.flush_dirty() {
            tracing::warn!(%err, "failed to persist healed session graph ids");
        }
        tracing::info!(
            count = nil.len(),
            "healed nil root_graph_ids (overmap identity)"
        );
    }
    nil.len()
}

// `save_browser_nodes` / `load_browser_nodes` left with the web.* facet
// convergence (2026-07-20): browser state persists as web.* facets in
// facets.json (write_web_states in the save path, read_web_states on adopt).
// Only the legacy read below remains, for pre-convergence profiles.

/// Read a pre-convergence `browser_nodes.json`, if one exists — the one-time
/// legacy absorb on adopt (facet values win; this only seeds unseen nodes).
/// Missing or corrupt reads empty.
pub fn load_legacy_browser_nodes(
    data_root: &Path,
) -> pandect::browser_node_state::BrowserNodeStates {
    match pandect::browser_node_state::load_browser_node_states(data_root) {
        Ok(Some(states)) => states,
        Ok(None) => pandect::browser_node_state::BrowserNodeStates::new(),
        Err(err) => {
            tracing::warn!(%err, "failed to read the legacy browser-state sidecar; ignoring it");
            pandect::browser_node_state::BrowserNodeStates::new()
        }
    }
}

/// Restore the workbench tiling, pruned to `present` (the live graph's
/// members, so a tile whose node vanished between sessions collapses away).
/// A missing or corrupt sidecar starts on an empty workbench.
pub fn load_workbench(
    data_root: &Path,
    present: &std::collections::HashSet<uuid::Uuid>,
) -> mere::platen::Workbench {
    let path = data_root.join(WORKBENCH_FILE);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return mere::platen::Workbench::new();
    };
    match mere::platen::Workbench::from_persisted_json(&json, present) {
        Some(wb) => {
            tracing::info!(path = ?path, tiles = wb.tile_count(), "workbench tiling restored");
            wb
        }
        None => {
            tracing::warn!(path = ?path, "failed to parse the workbench sidecar; starting empty");
            mere::platen::Workbench::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{Arrangement, Spiral};

    // Future-supplier controls: these require the coordinated immutable
    // resource supplier. They cover only load and migration re-save placement.
    fn recorded_session_fixture() -> Graph {
        use mere::kernel::graph::apply::add_node;
        let mut graph = Graph::new();
        add_node(&mut graph, None, "https://placement.test/dir/page".into(), Default::default());
        add_node(&mut graph, None, "https://placement.test/dir/".into(), Default::default());
        let mut snapshot = graph.to_snapshot();
        snapshot.edges.clear();
        snapshot.resource_edges.clear();
        let recorded = Graph::try_from_recorded_snapshot(&snapshot).unwrap();
        let unqualified = Graph::try_from_snapshot(&snapshot).unwrap().to_snapshot();
        assert!(unqualified.edges.len() + unqualified.resource_edges.len() > 0,
            "the deliberately broken unprofiled load re-derives URL containment");
        recorded
    }

    // Legacy pixels are deserialize-only fields. Build their historical wire
    // form explicitly; serializing PersistedNode intentionally omits them.
    fn with_inline_thumbnail(mut wire: serde_json::Value) -> Vec<u8> {
        wire["nodes"][0]["thumbnail_png"] = serde_json::json!(encode_rgba_png(&[1, 2, 3, 255], 1, 1).unwrap());
        wire["nodes"][0]["thumbnail_width"] = serde_json::json!(1);
        wire["nodes"][0]["thumbnail_height"] = serde_json::json!(1);
        serde_json::to_vec_pretty(&wire).unwrap()
    }

    #[test]
    fn session_profile_corrupt_facets_preserve_graph_labels_and_deposit_no_images() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(session_graph_store::GRAPH_FILE);
        let mut snapshot = recorded_session_fixture().to_snapshot();
        snapshot.nodes[0].tags = vec!["keep".into(), "Paper".into()];
        let bytes = with_inline_thumbnail(serde_json::to_value(&snapshot).unwrap());
        std::fs::write(&path, &bytes).unwrap();
        let facets = pandect::node_facets_path(root.path());
        std::fs::write(&facets, b"{bad facets").unwrap();
        assert!(try_load_session_graph(root.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read(facets).unwrap(), b"{bad facets");
        assert!(!root.path().join("images").exists());
    }

    #[test]
    fn session_profile_migration_write_failure_preserves_raw_control_evidence() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(session_graph_store::GRAPH_FILE);
        let mut snapshot = recorded_session_fixture().to_snapshot();
        snapshot.nodes[0].tags = vec!["keep".into(), "Paper".into()];
        let bytes = serde_json::to_vec_pretty(&snapshot).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let blocked_tmp = pandect::node_facets_path(root.path()).with_extension("json.tmp");
        std::fs::create_dir(&blocked_tmp).unwrap();
        assert!(try_load_session_graph(root.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_dir(blocked_tmp).unwrap();
        let input = try_load_session_graph(root.path()).unwrap().unwrap();
        let first = uuid::Uuid::parse_str(&snapshot.nodes[0].node_id).unwrap();
        assert!(crate::surface_controls::read(&input.graph, first, crate::surface_controls::Control::Keep));
        let saved = session_graph_store::load_profiled_snapshot(&path).unwrap().unwrap();
        assert_eq!(saved.placement, None);
        assert!(saved.snapshot.nodes.iter().all(|node| !node.tags.iter().any(|tag| tag == "keep")));
    }

    #[test]
    fn session_profile_recorded_load_and_migration_preserve_held_relations() {
        use pandect::graph_placement::PlacementProfile;
        use mere::kernel::types::ImageRole;
        for image_migration in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join(session_graph_store::GRAPH_FILE);
            let graph = recorded_session_fixture();
            session_graph_store::save_profiled(&path, &graph,
                Some(PlacementProfile::RecordedStrataV1)).unwrap();
            let mut input = session_graph_store::load_profiled_snapshot(&path).unwrap().unwrap();
            let migrated_member = input.snapshot.nodes[0].node_id.clone();
            let bytes = if image_migration {
                with_inline_thumbnail(serde_json::to_value(&input).unwrap())
            } else {
                input.snapshot.nodes[0].is_pinned = true;
                serde_json::to_vec_pretty(&input).unwrap()
            };
            std::fs::write(&path, bytes).unwrap();
            let restored = load_session_graph(root.path()).expect("qualified recorded input");
            let restored_snapshot = restored.to_snapshot();
            assert!(restored_snapshot.edges.is_empty());
            assert!(restored_snapshot.resource_edges.is_empty());
            let saved = session_graph_store::load_profiled_snapshot(&path).unwrap().unwrap();
            assert_eq!(saved.placement, Some(PlacementProfile::RecordedStrataV1));
            assert_eq!(saved.snapshot.legacy_image_count(), 0);
            assert_eq!(saved.snapshot.legacy_node_facet_count(), 0);
            if image_migration {
                let migrated = saved.snapshot.nodes.iter().find(|node| node.node_id == migrated_member).unwrap();
                let image = migrated.images.get(&ImageRole::Preview)
                    .expect("the migrated member retains its preview reference");
                assert!(load_image_blob(root.path(), &image.hex()).is_some());
            }
            let reopened = load_session_graph(root.path()).unwrap().to_snapshot();
            assert_eq!(reopened.edges, saved.snapshot.edges);
            assert_eq!(reopened.resource_edges, saved.snapshot.resource_edges);
        }
    }

    #[test]
    fn session_profile_invalid_resource_and_declared_legacy_refuse_without_rewrite() {
        use mere::kernel::persistence::PersistedShownResource;
        use pandect::graph_placement::PlacementProfile;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(session_graph_store::GRAPH_FILE);
        let mut invalid = recorded_session_fixture().to_snapshot();
        invalid.shown_resources.push(PersistedShownResource {
            surface_id: "not-a-uuid".into(), resource_id: uuid::Uuid::new_v4().to_string(),
        });
        std::fs::write(&path, with_inline_thumbnail(serde_json::to_value(&invalid).unwrap())).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(load_session_graph(root.path()).is_none(), "invalid explicit resource data is fallible");
        assert!(!root.path().join("images").exists(), "refused input deposits no migration blobs");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        session_graph_store::save_profiled(&path, &Graph::new(),
            Some(PlacementProfile::LegacySurfaceV1)).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(load_session_graph(root.path()).is_none(), "declared legacy needs qualified replay");
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn session_profile_absent_migration_does_not_mint_qualification() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(session_graph_store::GRAPH_FILE);
        let mut snapshot = recorded_session_fixture().to_snapshot();
        snapshot.nodes[0].is_pinned = true;
        std::fs::write(&path, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
        assert!(load_session_graph(root.path()).is_some());
        let saved = session_graph_store::load_profiled_snapshot(&path).unwrap().unwrap();
        assert_eq!(saved.placement, None);
        assert_eq!(saved.snapshot.legacy_node_facet_count(), 0);
    }

    #[test]
    fn place_collection_sidecar_is_exact_and_refuses_corruption() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(load_place_collection(root.path()).unwrap(), None);
        let selected = crate::place::PlaceCollectionVersion {
            moot: crate::place::PlaceId([1; 32]),
            collection: crate::place::PlaceCollectionId([2; 32]),
            frontier: vec![[3; 32], [4; 32]],
            membership_commitment: [5; 32],
        };
        save_place_collection(root.path(), Some(&selected)).unwrap();
        assert_eq!(load_place_collection(root.path()).unwrap(), Some(selected));
        save_place_collection(root.path(), None).unwrap();
        assert_eq!(load_place_collection(root.path()).unwrap(), None);
        std::fs::write(
            root.path().join(PLACE_COLLECTION_FILE),
            b"{\"version\":2,\"selection\":null}",
        )
        .unwrap();
        assert!(
            load_place_collection(root.path())
                .unwrap_err()
                .contains("unsupported")
        );
        std::fs::write(root.path().join(PLACE_COLLECTION_FILE), br#"{"version":1}"#).unwrap();
        assert!(
            load_place_collection(root.path())
                .unwrap_err()
                .contains("decode")
        );
        std::fs::write(root.path().join(PLACE_COLLECTION_FILE), b"{").unwrap();
        assert!(
            load_place_collection(root.path())
                .unwrap_err()
                .contains("decode")
        );
    }

    fn temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "turnstone-session-test-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The flat single-session layout migrates into `sessions/<id>/` exactly
    /// once: the sidecars MOVE (the flat graph is gone), the manifest is
    /// written, and `pick_session` finds the minted session. A second boot
    /// with a populated store migrates nothing.
    #[test]
    fn flat_layout_migrates_into_sessions_once() {
        let root = temp_root("migrate");
        std::fs::write(root.join(session_graph_store::GRAPH_FILE), b"{}").unwrap();
        std::fs::write(root.join(frisket_store::FRAME_FILE), b"{}").unwrap();

        let mut store = load_manifests(&root);
        let id = migrate_flat_layout(&root, &mut store).expect("the flat layout migrates");
        let dir = session_dir(&root, id);
        assert!(dir.join(session_graph_store::GRAPH_FILE).exists());
        assert!(dir.join(frisket_store::FRAME_FILE).exists());
        assert!(dir.join("manifest.json").exists());
        assert!(
            !root.join(session_graph_store::GRAPH_FILE).exists(),
            "the flat graph MOVED, not copied"
        );
        assert_eq!(store.len(), 1);
        assert_eq!(pick_session(&root, &store), Some(id));

        // Reload from disk (a second boot): nothing migrates again.
        let mut store2 = load_manifests(&root);
        assert_eq!(store2.len(), 1);
        assert_eq!(migrate_flat_layout(&root, &mut store2), None);
    }

    /// The current-session marker round-trips, and a stale marker (a session
    /// that no longer exists) falls back to the most recent manifest.
    #[test]
    fn current_session_marker_round_trips_and_falls_back() {
        let root = temp_root("current");
        let mut store = ManifestStore::with_root(sessions_root(&root));
        let a = SessionId::new();
        let b = SessionId::new();
        let mut ma = GraphSessionManifest::new(a, crate::panes::GraphId::nil());
        // `b` is newer (created after), so the fallback picks it.
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mb = GraphSessionManifest::new(b, crate::panes::GraphId::nil());
        ma.touch();
        store.insert(ma);
        store.insert(mb);

        record_current_session(&root, a);
        assert_eq!(pick_session(&root, &store), Some(a));
        record_current_session(&root, SessionId::new());
        // The recorded id is unknown; the newest manifest wins. `a` was
        // touched later than `b` was created, so updated_at prefers `a`.
        assert_eq!(pick_session(&root, &store), Some(a));
    }

    #[test]
    fn projection_score_sidecar_round_trips_without_becoming_graph_truth() {
        let root = temp_root("projection-score");
        let score = Score::new(Arrangement::Spiral(Spiral::default()));
        save_projection_score(&root, &score);
        assert_eq!(load_projection_score(&root), Some(score));
        assert!(projection_score_path(&root).exists());
        assert!(
            !root.join(session_graph_store::GRAPH_FILE).exists(),
            "the score remains a view sidecar"
        );
    }

    #[test]
    fn command_menu_rides_the_view_sidecar_and_old_sidecars_default_it() {
        let root = temp_root("command-menu");
        let intent = ViewIntentV1 {
            command_menu: cambium::CommandChoices {
                added: vec!["Reseed layout".into()],
                removed: vec!["view:fit".into()],
                recent: vec!["nav:back".into()],
            },
            ..ViewIntentV1::default()
        };
        save_view_intent(&root, &intent);
        assert_eq!(load_view_intent(&root), Some(intent));

        // A sidecar written before the command menu existed still opens.
        std::fs::write(view_intent_path(&root), br#"{"layout_strategy":"spiral"}"#).unwrap();
        let older = load_view_intent(&root).expect("older sidecar loads");
        assert_eq!(older.command_menu, cambium::CommandChoices::default());
        assert_eq!(older.layout_strategy.as_deref(), Some("spiral"));

        // A sidecar from the label-keyed `CommandMenuV1` reads back with its
        // fields intact: the JSON names did not change (SE45).
        std::fs::write(
            view_intent_path(&root),
            br#"{"command_menu":{"added":["Reseed layout"],"removed":["Fit view"],"recent":[]}}"#,
        )
        .unwrap();
        let labelled = load_view_intent(&root).expect("a label-keyed sidecar loads");
        assert_eq!(labelled.command_menu.added, ["Reseed layout"]);
        assert_eq!(labelled.command_menu.removed, ["Fit view"]);
    }

    #[test]
    fn view_intent_round_trips_and_survives_a_corrupt_sidecar() {
        let root = temp_root("view-intent");
        assert_eq!(
            load_view_intent(&root),
            None,
            "absent sidecar is not an error"
        );

        let intent = ViewIntentV1 {
            layout_strategy: Some("kanban.community".to_string()),
            ..Default::default()
        };
        save_view_intent(&root, &intent);
        assert_eq!(load_view_intent(&root), Some(intent));

        // Force-directed is a real choice, not the absence of one.
        let native = ViewIntentV1 {
            layout_strategy: None,
            ..Default::default()
        };
        save_view_intent(&root, &native);
        assert_eq!(load_view_intent(&root), Some(native));

        // A corrupt sidecar is diagnosed and ignored rather than failing the
        // session open, matching the score's posture.
        std::fs::write(view_intent_path(&root), b"{ not json").unwrap();
        assert_eq!(load_view_intent(&root), None);
    }

    /// The physics choice rides the intent by id, and a sidecar written
    /// before the catalog (no physics fields at all) still opens, as the
    /// canvas defaults. (Physics catalog — P2.)
    #[test]
    fn the_physics_choice_rides_the_view_intent_by_id() {
        let root = temp_root("view-intent-physics");
        let intent = ViewIntentV1 {
            layout_strategy: None,
            physics_law: Some("orbit.gravity".to_string()),
            physics_overlays: vec!["skeleton".to_string(), "tide".to_string()],
            physics_kind_source: Some("coloring".to_string()),
            physics_mass_source: Some("pagerank".to_string()),
            physics_depth_source: Some("layers".to_string()),
            command_menu: cambium::CommandChoices::default(),
        };
        save_view_intent(&root, &intent);
        assert_eq!(load_view_intent(&root), Some(intent));

        std::fs::write(
            view_intent_path(&root),
            br#"{"layout_strategy":"grid.default"}"#,
        )
        .unwrap();
        let loaded = load_view_intent(&root).unwrap();
        assert_eq!(loaded.layout_strategy.as_deref(), Some("grid.default"));
        assert_eq!(loaded.physics_law, None);
        assert!(loaded.physics_overlays.is_empty());
        assert_eq!(loaded.physics_kind_source, None);
    }

    #[test]
    fn the_persisted_strategy_is_an_id_never_a_display_name() {
        // An arrangement rename (Kanban -> Board) must not touch stored
        // sessions, which is only true while the sidecar holds the id.
        let root = temp_root("view-intent-key");
        let stored = mere::canvas::CANVAS_LAYOUT_STRATEGIES
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in stored {
            save_view_intent(
                &root,
                &ViewIntentV1 {
                    layout_strategy: Some(id.to_string()),
                    ..Default::default()
                },
            );
            let loaded = load_view_intent(&root).unwrap().layout_strategy.unwrap();
            assert_eq!(loaded, id);
            assert!(
                !loaded.chars().next().unwrap().is_uppercase(),
                "{loaded} reads like a display name, not a persistence key"
            );
        }
    }

    #[test]
    fn place_binding_round_trips_as_a_distinct_public_sidecar() {
        let root = temp_root("place-binding");
        let binding = PlaceBindingV1::new(
            crate::place::PlaceId([0x11; 32]),
            crate::place::SharedContainerId([0x22; 32]),
            crate::place::ChatSpaceId([0x33; 32]),
            "commons",
        )
        .unwrap();

        assert_eq!(load_place_binding(&root).unwrap(), None);
        save_place_binding(&root, &binding).unwrap();
        assert_eq!(load_place_binding(&root).unwrap(), Some(binding));
        assert!(
            !root.join(session_graph_store::GRAPH_FILE).exists(),
            "the place binding remains beside graph truth"
        );
    }

    #[test]
    fn a_routine_save_updates_a_binding_but_never_mints_one() {
        let root = temp_root("place-binding-update");
        let binding = PlaceBindingV1::new(
            crate::place::PlaceId([0x11; 32]),
            crate::place::SharedContainerId([0x22; 32]),
            crate::place::ChatSpaceId([0x33; 32]),
            "commons",
        )
        .unwrap();

        // No admitted place yet: a save reports that it wrote nothing rather
        // than creating a binding admission never granted.
        assert_eq!(update_place_binding(&root, &binding).unwrap(), false);
        assert!(!place_binding_path(&root).exists());
        assert_eq!(load_place_binding(&root).unwrap(), None);

        // Once admission has established one, ordinary saves may update it.
        save_place_binding(&root, &binding).unwrap();
        let mut renamed = binding.clone();
        renamed.default_channel = "hall".into();
        assert_eq!(update_place_binding(&root, &renamed).unwrap(), true);
        assert_eq!(load_place_binding(&root).unwrap(), Some(renamed));
    }

    #[test]
    fn removing_place_binding_preserves_retained_session_files() {
        let root = temp_root("place-binding-remove");
        let binding = PlaceBindingV1::new(
            crate::place::PlaceId([0x91; 32]),
            crate::place::SharedContainerId([0x92; 32]),
            crate::place::ChatSpaceId([0x93; 32]),
            "hall",
        )
        .unwrap();
        save_place_binding(&root, &binding).unwrap();
        std::fs::write(root.join("place-history.redb"), b"retained").unwrap();

        assert_eq!(remove_place_binding(&root).unwrap(), true);
        assert_eq!(load_place_binding(&root).unwrap(), None);
        assert!(root.join("place-history.redb").exists());
        assert_eq!(remove_place_binding(&root).unwrap(), false);

        // A malformed path must report failure and preserve what it found.
        let binding_path = place_binding_path(&root);
        std::fs::create_dir(&binding_path).unwrap();
        std::fs::write(binding_path.join("keep"), b"unexpected retained data").unwrap();
        assert!(remove_place_binding(&root).is_err());
        assert!(binding_path.join("keep").exists());
    }

    #[test]
    fn marking_a_place_left_preserves_the_binding_and_retained_session_files() {
        let root = temp_root("place-left-mark");
        let binding = PlaceBindingV1::new(
            crate::place::PlaceId([0x94; 32]),
            crate::place::SharedContainerId([0x95; 32]),
            crate::place::ChatSpaceId([0x96; 32]),
            "hall",
        )
        .unwrap();
        save_place_binding(&root, &binding).unwrap();
        std::fs::write(root.join("place-history.redb"), b"retained").unwrap();
        let retained =
            crate::place::PlaceLeftSummary { graph_nodes: 3, chat_messages: 5, members: 2 };

        assert_eq!(load_place_left(&root).unwrap(), None);
        mark_place_left(&root, &retained).unwrap();
        assert_eq!(load_place_left(&root).unwrap(), Some(retained));
        // Unlike `remove_place_binding`, the binding and every retained
        // store stay exactly as they were: Rejoin needs both.
        assert_eq!(load_place_binding(&root).unwrap(), Some(binding));
        assert!(root.join("place-history.redb").exists());

        assert_eq!(clear_place_left(&root).unwrap(), true);
        assert_eq!(load_place_left(&root).unwrap(), None);
        assert_eq!(clear_place_left(&root).unwrap(), false);

        // A malformed marker must report failure rather than silently
        // reading as joined or as left.
        let marker_path = root.join(PLACE_LEFT_FILE);
        std::fs::write(&marker_path, b"not json").unwrap();
        assert!(load_place_left(&root).is_err());
        std::fs::write(&marker_path, br#"{"version":2,"graph_nodes":0,"chat_messages":0,"members":0}"#).unwrap();
        assert!(matches!(
            load_place_left(&root),
            Err(PlaceSidecarError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn unsupported_place_binding_stays_a_visible_error() {
        let root = temp_root("place-version");
        let binding = PlaceBindingV1::new(
            crate::place::PlaceId([0x11; 32]),
            crate::place::SharedContainerId([0x22; 32]),
            crate::place::ChatSpaceId([0x33; 32]),
            "commons",
        )
        .unwrap();
        let mut value = serde_json::to_value(binding).unwrap();
        value["version"] = serde_json::json!(99);
        std::fs::write(
            place_binding_path(&root),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();

        let error = load_place_binding(&root).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported place binding version 99")
        );
        assert!(
            place_binding_path(&root).exists(),
            "a failed load does not erase the evidence"
        );
    }

    #[test]
    fn legacy_node_columns_migrate_on_load_and_existing_facets_win() {
        let root = temp_root("node-facet-migration");
        let mut canvas = mere::canvas::Canvas::new();
        let key = canvas.visit("https://legacy.example/");
        let node_id = canvas.graph().get_node(key).unwrap().id;
        save_session_graph(&root, canvas.graph());

        let graph_file = root.join(session_graph_store::GRAPH_FILE);
        let mut snapshot = session_graph_store::load_snapshot(&graph_file)
            .unwrap()
            .unwrap();
        snapshot.nodes[0].is_pinned = true;
        std::fs::write(
            &graph_file,
            serde_json::to_string_pretty(&snapshot).unwrap(),
        )
        .unwrap();

        let mut facets = pandect::NodeFacetStore::new();
        facets
            .set(
                node_id,
                chartulary::FacetId::new(mere::kernel::graph::node_facets::ARRANGEMENT_PIN),
                serde_json::json!(false),
                &chartulary::AcceptAll,
            )
            .unwrap();
        save_node_facets(&root, &facets);

        let restored = load_session_graph(&root).expect("legacy graph");
        let restored_key = restored.get_node_key_by_id(node_id).unwrap();
        assert_eq!(
            restored.node_is_pinned(restored_key),
            Some(false),
            "the canonical sidecar overlays the imported legacy value"
        );

        let canonical = session_graph_store::load_snapshot(&graph_file)
            .unwrap()
            .unwrap();
        assert!(!canonical.nodes[0].is_pinned);
        assert_eq!(canonical.legacy_node_facet_count(), 0);
        let persisted = load_node_facets(&root).unwrap();
        assert_eq!(
            persisted.get(
                &node_id,
                &chartulary::FacetId::new(mere::kernel::graph::node_facets::ARRANGEMENT_PIN,),
            ),
            Some(&serde_json::json!(false))
        );
    }

    #[test]
    fn image_gc_keeps_live_blobs_and_drops_only_hash_named_orphans() {
        let root = temp_root("image-gc");
        let live = mere::kernel::types::ImageRef::new([1; 32], 1, 1);
        let orphan = mere::kernel::types::ImageRef::new([2; 32], 1, 1);
        save_image_blob(&root, &live.hex(), b"live");
        save_image_blob(&root, &orphan.hex(), b"orphan");
        std::fs::write(images_dir(&root).join("README"), b"not a blob").unwrap();

        let mut canvas = mere::canvas::Canvas::new();
        let key = canvas.visit("https://live.example/");
        let node = canvas.graph().get_node(key).unwrap().id;
        assert!(canvas.set_node_favicon_for(node, live));

        assert_eq!(gc_orphan_image_blobs(&root, canvas.graph()), 1);
        assert!(load_image_blob(&root, &live.hex()).is_some());
        assert!(load_image_blob(&root, &orphan.hex()).is_none());
        assert!(images_dir(&root).join("README").exists());
        assert_eq!(
            gc_orphan_image_blobs(&root, canvas.graph()),
            0,
            "re-running the sweep is a no-op"
        );
    }
}
