// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
#![cfg(feature = "web")]

use graphshell::app::GraphshellApp;
use graphshell::local_scene::{capture_scene, restore_scene};
use graphshell::mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef};
use mere::canvas::{CameraView, Canvas, Face, Role};
use muniment::MemoryBackend;

#[test]
fn retained_scene_restores_camera_roles_representation_and_selection() {
    let app = GraphshellApp::fixture(
        MemoryBackend::new(),
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.into(),
            profile: "profile:scene-test".into(),
        },
    )
    .unwrap();
    let owner = app.host.graph();
    let member = owner.get_node_by_url(FIXTURE_WEB_ADDRESS).unwrap().1.id;
    let mut canvas = Canvas::with_graph(owner.clone());
    canvas.seed_cartography(
        owner
            .nodes()
            .enumerate()
            .map(|(i, (_, node))| (node.id, (i as f32 * 40.0, i as f32 * 20.0))),
    );
    canvas.set_layout_strategy(Some("grid.default".into()));
    canvas.set_arrangement_role(Role::Anchored);
    canvas.set_member_role(member, Some(Role::Pinned));
    canvas.set_node_face(member, Face::Bare);
    canvas.set_selected_members(&[member]);
    canvas.set_camera(CameraView {
        offset: (42.0, 19.0),
        zoom: 1.75,
    });
    canvas.set_physics_paused(true);
    let saved = capture_scene(&canvas, "grid.default", "graphshell.inspect").unwrap();
    let mut restored = Canvas::with_graph(owner.clone());
    restore_scene(&mut restored, owner, &saved, (900, 600)).unwrap();
    assert_eq!(restored.camera(), canvas.camera());
    assert_eq!(restored.selected_members(), [member]);
    assert_eq!(restored.member_role(member), Some(Role::Pinned));
    assert_eq!(restored.arrangement_roles().default, Role::Anchored);
    assert!(restored.physics_paused());
    assert_eq!(
        restored.node_face(owner.get_node_key_by_id(member).unwrap()),
        Face::Bare
    );
    assert_eq!(
        restored
            .cartography_geometry()
            .iter()
            .collect::<std::collections::BTreeMap<_, _>>(),
        canvas
            .cartography_geometry()
            .iter()
            .collect::<std::collections::BTreeMap<_, _>>()
    );
}

#[test]
fn malformed_scene_refuses_before_replacing_the_canvas() {
    let app = GraphshellApp::fixture(
        MemoryBackend::new(),
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.into(),
            profile: "profile:scene-test".into(),
        },
    )
    .unwrap();
    let owner = app.host.graph();
    let mut canvas = Canvas::with_graph(owner.clone());
    let camera = canvas.camera();
    let mut saved = capture_scene(&canvas, "grid.default", "graphshell.inspect").unwrap();
    saved.camera_zoom = f32::NAN;
    assert!(restore_scene(&mut canvas, owner, &saved, (900, 600)).is_err());
    assert_eq!(canvas.camera(), camera);
    assert_eq!(canvas.graph().revision(), owner.revision());
}
