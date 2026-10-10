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
        selected: canvas.selected_members(),
        physics_paused: canvas.physics_paused(),
        dynamics: SavedDynamics::from_spec(&spec),
        camera_offset: camera.offset,
        camera_zoom: camera.zoom,
        default_handler: handler.to_owned(),
        cartography: canvas.cartography_geometry(),
    })
}

/// Validate before replacing a working canvas. Missing members in a saved
/// sidecar are skipped by the existing member-keyed Canvas restoration APIs.
pub fn restore_scene(
    canvas: &mut Canvas,
    owner: &Graph,
    scene: &SavedSceneV3,
    viewport: (u32, u32),
) -> Result<(), String> {
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
    canvas.set_graph(owner.clone());
    canvas.set_layout_strategy(
        (arrangement != crate::canvas_physics::FREE_ARRANGEMENT).then(|| arrangement.clone()),
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
