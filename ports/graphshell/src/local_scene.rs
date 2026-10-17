// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Saved-scene capture and restoration shared by Graphshell presentations.
use mere::canvas::{CameraView, Canvas};
use mere::kernel::{geometry::PortablePoint, graph::Graph};

use crate::product::{SavedDynamics, SavedSceneV3};

pub fn capture_scene(
    canvas: &Canvas,
    arrangement: &str,
    handler: &str,
) -> Result<SavedSceneV3, String> {
    let mut spec = canvas.dynamics_spec()?;
    if let Some(target) = spec.target.as_mut() {
        target.arrangement = arrangement.to_owned();
    }
    let camera = canvas.camera();
    Ok(SavedSceneV3 {
        name: "Graphshell working scene".into(),
        selected: canvas
            .selected_members()
            .into_iter()
            .filter(|member| !canvas.is_derived_member(*member))
            .collect(),
        physics_paused: canvas.physics_paused(),
        dynamics: SavedDynamics::from_spec(&spec),
        camera_offset: camera.offset,
        camera_zoom: camera.zoom,
        default_handler: handler.to_owned(),
        cartography: canvas.cartography_geometry(),
    })
}

struct PreparedScene {
    spec: mere::canvas::dynamics_spec::DynamicsSpec,
    arrangement: String,
}
fn prepare_scene(scene: &SavedSceneV3) -> Result<PreparedScene, String> {
    let spec = scene.dynamics_spec()?;
    let arrangement = scene.arrangement()?;
    if !scene.camera_offset.0.is_finite()
        || !scene.camera_offset.1.is_finite()
        || !scene.camera_zoom.is_finite()
        || scene.camera_zoom <= 0.0
    {
        return Err("saved scene has an invalid camera".into());
    }
    if scene
        .cartography
        .iter()
        .any(|(_, (x, y))| !x.is_finite() || !y.is_finite())
    {
        return Err("saved scene has an invalid position".into());
    }
    Ok(PreparedScene { spec, arrangement })
}
/// Graph-free rejection before an imported codicil changes App authority.
pub(crate) fn validate_scene(scene: &SavedSceneV3) -> Result<(), String> {
    prepare_scene(scene).map(|_| ())
}

/// Validate before replacing a working canvas. Missing members in a saved
/// sidecar are skipped by the existing member-keyed Canvas restoration APIs.
pub fn restore_scene(
    canvas: &mut Canvas,
    owner: &Graph,
    scene: &SavedSceneV3,
    viewport: (u32, u32),
) -> Result<(), String> {
    let PreparedScene { spec, arrangement } = prepare_scene(scene)?;
    canvas.set_graph(owner.clone());
    canvas.set_layout_strategy(
        (arrangement != mere::canvas::physics_controls::FREE_ARRANGEMENT)
            .then(|| arrangement.clone()),
    );
    let positions: Vec<_> = scene
        .cartography
        .iter()
        .filter_map(|(id, (x, y))| {
            owner
                .get_node_key_by_id(id)
                .map(|key| (key, PortablePoint::new(x, y)))
        })
        .collect();
    canvas.apply_strategy_positions(&positions);
    canvas.note_strategy_computed(&arrangement, viewport.0, viewport.1, canvas.focused_key());
    canvas.apply_cartography_importance_metric(scene.cartography.importance_metric());
    canvas.apply_cartography_sizing(
        scene.cartography.size_iter(),
        scene.cartography.size_by_degree(),
        scene.cartography.size_by_importance(),
    );
    canvas.apply_cartography_sprites(scene.cartography.sprite_iter());
    canvas.apply_cartography_sprite_hulls(scene.cartography.sprite_hull_iter());
    canvas.apply_cartography_materials(scene.cartography.material_iter());
    canvas.apply_cartography_faces(scene.cartography.face_iter());
    canvas.apply_cartography_scene_state(&scene.cartography);
    // Binding was checked above, before the graph changed.
    canvas
        .set_dynamics_spec(&spec)
        .map_err(|error| format!("dynamics spec: {error}"))?;
    canvas.seed_cartography(scene.cartography.iter());
    canvas.set_physics_paused(scene.physics_paused);
    canvas.set_selected_members(&scene.selected);
    canvas.set_camera(CameraView {
        offset: scene.camera_offset,
        zoom: scene.camera_zoom,
    });
    Ok(())
}

/// A successful import is authority even if its optional view cannot restore.
pub struct OpenedCodicil {
    pub receipt: crate::product::ImportReceipt,
    pub owner: uuid::Uuid,
    pub scene: Option<SavedSceneV3>,
    pub view_error: Option<String>,
}
/// Restore a view of an already accepted graph. Every return leaves Canvas
/// attached to that graph; no old graph-local keys survive a restoration error.
pub(crate) fn install_accepted_codicil(
    canvas: &mut Canvas,
    owner: &Graph,
    scene: Option<&SavedSceneV3>,
    viewport: (u32, u32),
) -> Option<String> {
    if let Some(scene) = scene {
        if let Err(error) = restore_scene(canvas, owner, scene, viewport) {
            canvas.set_graph(owner.clone());
            return Some(error);
        }
    } else {
        canvas.set_graph(owner.clone());
    }
    None
}
/// The retained Scene Open operation, shared with native lifecycle tests.
pub fn open_codicil<B: muniment::Backend + Clone>(
    host: &mut crate::mere_host::MereHost<B>,
    canvas: &mut Canvas,
    bytes: &[u8],
    viewport: (u32, u32),
) -> Result<OpenedCodicil, String> {
    let (receipt, scene) = host
        .replace_with_product_codicil(bytes)
        .map_err(|e| e.to_string())?;
    let owner = host.graph_session().id().0;
    let view_error = install_accepted_codicil(canvas, host.graph(), scene.as_ref(), viewport);
    Ok(OpenedCodicil {
        receipt,
        owner,
        scene,
        view_error,
    })
}

/// Install the returned App graph before Forme reconciliation, including a
/// retained in-memory edit whose persistence failed.
/// Same-owner stable keys retain view and physics state. Reused keys or a new
/// actual owner use the graph replacement path, which releases old local holds.
pub fn reconcile_completed_graph(
    canvas: &mut Canvas,
    previous_owner: Option<uuid::Uuid>,
    actual_owner: uuid::Uuid,
    graph: &Graph,
) {
    let keys_match = previous_owner == Some(actual_owner)
        && canvas.graph().nodes().all(|(key, node)| {
            graph
                .get_node_by_id(node.id)
                .is_none_or(|(next, _)| next == key)
                && graph.get_node(key).is_none_or(|next| next.id == node.id)
        });
    if !keys_match {
        canvas.set_graph(graph.clone());
    } else if canvas.refresh_same_topology_graph(graph).is_err() {
        canvas.ingest_graph(|current| {
            *current = graph.clone();
            true
        });
    }
}
