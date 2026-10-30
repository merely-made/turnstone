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

/// The current-session marker (`<root>/current_session`, the bare uuid): the
/// session a restart reopens. Best-effort, like every sidecar.
const CURRENT_SESSION_FILE: &str = "current_session";

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

const SESSION_FILES: [&str; 11] = [
    session_graph_store::GRAPH_FILE,
    frisket_store::FRAME_FILE,
    frisket_store::LEGACY_FRAME_FILE,
    WORKBENCH_FILE,
    pandect::browser_node_state::BROWSER_NODES_FILE,
    frisket_store::WINDOWS_FILE,
    frisket_store::LEGACY_WINDOWS_FILE,
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

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceLeftMarkerV1 {
    version: u16,
    graph_nodes: usize,
    chat_messages: usize,
    members: usize,
}

const PLACE_LEFT_MARKER_VERSION: u16 = 1;

/// A loaded graph carries only the placement declared at its input boundary.
pub(crate) struct LoadedSessionGraph {
    pub graph: Graph,
    pub placement: Option<pandect::graph_placement::PlacementProfile>,
}

/// The workbench tiling sidecar, beside `graph.json` (the meerkat convention:
/// the tiling is the graph's, so it persists with the session).
const WORKBENCH_FILE: &str = "workbench.json";

#[cfg(test)]
#[path = "session/tests.rs"]
mod tests;

#[path = "session/recent_visited_urls.rs"]
mod recent_visited_urls;
pub(crate) use recent_visited_urls::recent_visited_urls;
#[path = "session/copy_session_component.rs"]
mod copy_session_component;
pub(crate) use copy_session_component::copy_session_component;
#[path = "session/default_turnstone_root.rs"]
mod default_turnstone_root;
pub use default_turnstone_root::default_turnstone_root;
#[path = "session/sessions_root.rs"]
mod sessions_root;
pub use sessions_root::sessions_root;
#[path = "session/session_dir.rs"]
mod session_dir;
pub use session_dir::session_dir;
#[path = "session/record_current_session.rs"]
mod record_current_session;
pub use record_current_session::record_current_session;
#[path = "session/pick_session.rs"]
mod pick_session;
pub use pick_session::pick_session;
#[path = "session/load_manifests.rs"]
mod load_manifests;
pub use load_manifests::load_manifests;
#[path = "session/required_collection_selection.rs"]
mod required_collection_selection;
use required_collection_selection::required_collection_selection;
#[path = "session/load_place_collection.rs"]
mod load_place_collection;
pub(crate) use load_place_collection::load_place_collection;
#[path = "session/save_place_collection.rs"]
mod save_place_collection;
pub(crate) use save_place_collection::save_place_collection;
#[path = "session/impl_from_place_binding_error_for.rs"]
mod impl_from_place_binding_error_for;
#[path = "session/impl_from_serde_json_error_for.rs"]
mod impl_from_serde_json_error_for;
#[path = "session/impl_from_std_io_error_for.rs"]
mod impl_from_std_io_error_for;
#[path = "session/impl_std_error_error_for_place.rs"]
mod impl_std_error_error_for_place;
#[path = "session/impl_std_fmt_display_for_place.rs"]
mod impl_std_fmt_display_for_place;
#[path = "session/place_binding_path.rs"]
mod place_binding_path;
pub use place_binding_path::place_binding_path;
#[path = "session/save_place_binding.rs"]
mod save_place_binding;
pub use save_place_binding::save_place_binding;
#[path = "session/update_place_binding.rs"]
mod update_place_binding;
pub use update_place_binding::update_place_binding;
#[path = "session/remove_place_binding.rs"]
mod remove_place_binding;
pub use remove_place_binding::remove_place_binding;
#[path = "session/load_place_binding.rs"]
mod load_place_binding;
pub use load_place_binding::load_place_binding;
#[path = "session/mark_place_left.rs"]
mod mark_place_left;
pub use mark_place_left::mark_place_left;
#[path = "session/load_place_left.rs"]
mod load_place_left;
pub use load_place_left::load_place_left;
#[path = "session/clear_place_left.rs"]
mod clear_place_left;
pub use clear_place_left::clear_place_left;
#[path = "session/projection_score_path.rs"]
mod projection_score_path;
pub use projection_score_path::projection_score_path;
#[path = "session/save_projection_score.rs"]
mod save_projection_score;
pub use save_projection_score::save_projection_score;
#[path = "session/load_projection_score.rs"]
mod load_projection_score;
pub use load_projection_score::load_projection_score;
#[path = "session/view_intent_path.rs"]
mod view_intent_path;
pub use view_intent_path::view_intent_path;
#[path = "session/migrate_flat_layout.rs"]
mod migrate_flat_layout;
pub use migrate_flat_layout::migrate_flat_layout;
#[path = "session/try_load_session_graph.rs"]
mod try_load_session_graph;
pub(crate) use try_load_session_graph::try_load_session_graph;
#[path = "session/try_load_session_graph_locked.rs"]
mod try_load_session_graph_locked;
pub(crate) use try_load_session_graph_locked::try_load_session_graph_locked;
#[path = "session/load_session_graph.rs"]
mod load_session_graph;
pub use load_session_graph::load_session_graph;
#[path = "session/externalize_legacy_images.rs"]
mod externalize_legacy_images;
use externalize_legacy_images::externalize_legacy_images;
#[path = "session/encode_rgba_png.rs"]
mod encode_rgba_png;
pub(crate) use encode_rgba_png::encode_rgba_png;
#[path = "session/save_session_graph.rs"]
#[cfg(test)]
mod save_session_graph;
#[cfg(test)]
pub(crate) use save_session_graph::save_session_graph;
#[path = "session/images_dir.rs"]
mod images_dir;
use images_dir::images_dir;
#[path = "session/save_image_blob.rs"]
mod save_image_blob;
pub use save_image_blob::save_image_blob;
#[path = "session/try_save_image_blob.rs"]
mod try_save_image_blob;
use try_save_image_blob::try_save_image_blob;
#[path = "session/load_image_blob.rs"]
mod load_image_blob;
pub use load_image_blob::load_image_blob;
#[path = "session/gc_orphan_image_blobs.rs"]
mod gc_orphan_image_blobs;
pub use gc_orphan_image_blobs::gc_orphan_image_blobs;
#[path = "session/load_node_facets.rs"]
mod load_node_facets;
pub use load_node_facets::load_node_facets;
#[path = "session/save_node_facets.rs"]
mod save_node_facets;
pub use save_node_facets::save_node_facets;
#[path = "session/load_frisket_layout.rs"]
mod load_frisket_layout;
pub use load_frisket_layout::load_frisket_layout;
#[path = "session/save_frisket_layout.rs"]
mod save_frisket_layout;
pub use save_frisket_layout::save_frisket_layout;
#[path = "session/save_lens_spaces.rs"]
mod save_lens_spaces;
pub use save_lens_spaces::save_lens_spaces;
#[path = "session/load_lens_spaces.rs"]
mod load_lens_spaces;
pub use load_lens_spaces::load_lens_spaces;
#[path = "session/save_workbench.rs"]
mod save_workbench;
pub use save_workbench::save_workbench;
#[path = "session/heal_nil_graph_ids.rs"]
mod heal_nil_graph_ids;
pub use heal_nil_graph_ids::heal_nil_graph_ids;
#[path = "session/load_legacy_browser_nodes.rs"]
mod load_legacy_browser_nodes;
pub use load_legacy_browser_nodes::load_legacy_browser_nodes;
#[path = "session/load_workbench.rs"]
mod load_workbench;
pub use load_workbench::load_workbench;
