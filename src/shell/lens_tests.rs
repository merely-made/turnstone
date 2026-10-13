// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Actual host plans and the adopted frame/action seams. No physical GPU claim.

use super::super::{
    NodeSurfaces,
    content_pool::{ContentKey, HostPlacement, NodeDocuments},
};
use super::*;
use accesskit::{
    Action as A11yAction, ActionRequest, Node, NodeId, Role, Tree, TreeId, TreeUpdate,
};
use inker::{SurfaceError, SurfaceFrame, SurfaceProducer};
use pelt_core::{PeltContent, PeltLayer, PeltRoute, PeltRouteSource, PeltRouteState};
use std::sync::Mutex;

type Events = Arc<Mutex<Vec<String>>>;
#[derive(Clone, Copy)]
enum Paint {
    Frame,
    Empty,
    Error,
}
#[derive(Debug)]
struct OwnedPaint;
impl inker::OwnedSurfaceFrame for OwnedPaint {
    fn payload_kind(&self) -> &'static str {
        "test.lens-owned-paint"
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}
struct NativePage {
    events: Events,
    paint: Paint,
}
impl SurfaceProducer for NativePage {
    fn resize(&mut self, w: u32, h: u32) -> Result<(), SurfaceError> {
        self.events.lock().unwrap().push(format!("resize {w}x{h}"));
        Ok(())
    }
    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.events.lock().unwrap().push(format!("offset {x},{y}"));
        Ok(())
    }
    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.events.lock().unwrap().push("acquire".into());
        match self.paint {
            Paint::Frame => Ok(Some(SurfaceFrame {
                texture: inker::NativeTextureHandle::OwnedPayload(Box::new(OwnedPaint)),
                sync: inker::SurfaceSyncHandle::None,
                width: 800,
                height: 600,
                format: inker::SurfaceTextureFormat::Bgra8Unorm,
                resource_epoch: 47,
            })),
            Paint::Empty => Ok(None),
            Paint::Error => Err(SurfaceError::FrameAcquisitionFailed(
                "supplier capture refused".into(),
            )),
        }
    }
    fn send_mouse_input(&mut self, _: inker::MouseEvent) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn send_pointer_input(&mut self, event: inker::PointerEvent) -> Result<(), SurfaceError> {
        self.events.lock().unwrap().push(format!(
            "pointer {:?} {},{}",
            event.phase, event.position.x, event.position.y
        ));
        Ok(())
    }
    fn send_keyboard_input(&mut self, _: inker::KeyboardEvent) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn move_focus(&mut self, _: inker::FocusReason) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
        None
    }
    fn apply_settings(&mut self, _: &inker::SurfaceSettings) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn set_accessibility_active(&mut self, active: bool) -> Result<Option<TreeId>, SurfaceError> {
        Ok(active.then_some(supplier_id()))
    }
    fn poll_accessibility_update(&mut self) -> Option<TreeUpdate> {
        Some(supplier_tree())
    }
    fn send_accessibility_action(&mut self, request: ActionRequest) -> Result<(), SurfaceError> {
        assert_eq!(request.target_tree, supplier_id());
        assert_eq!(request.target_node, NodeId(1));
        self.events
            .lock()
            .unwrap()
            .push(format!("a11y {:?}", request.action));
        Ok(())
    }
}
fn supplier_id() -> TreeId {
    TreeId(accesskit::Uuid::from_u64_pair(17, 47))
}
fn supplier_tree() -> TreeUpdate {
    let mut root = Node::new(Role::Document);
    root.set_children(vec![NodeId(1)]);
    let mut link = Node::new(Role::Link);
    link.set_label("Native lens continuation");
    link.add_action(A11yAction::Click);
    TreeUpdate {
        nodes: vec![(NodeId(0), root), (NodeId(1), link)],
        tree: Some(Tree::new(NodeId(0))),
        tree_id: supplier_id(),
        focus: NodeId(1),
    }
}
fn fixture(paint: Paint) -> (crate::app::App, NodeDocuments, uuid::Uuid, Events) {
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress("https://native-lens.fixture/".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    let events = Events::default();
    let mut contents = NodeDocuments::default();
    contents.insert(
        node,
        PeltContent::from_surface(
            Box::new(NativePage {
                events: events.clone(),
                paint,
            }),
            (800, 600),
            PeltRoute {
                decision: inker::routing::EngineRouteDecision {
                    engine_id: "native.lens.fixture".into(),
                    surface_contract: inker::routing::SurfaceContract {
                        target: inker::routing::SurfaceTargetId::new("native-lens-fixture"),
                        mode: inker::routing::SurfaceContractMode::CompositedTexture,
                    },
                },
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Surface,
            },
        ),
    );
    (app, contents, node, events)
}
fn sync(contents: &mut NodeDocuments, plan: &[crate::surface::Surface], window: u64) {
    contents.sync_placements(
        plan.iter()
            .map(|surface| HostPlacement::physical(*surface, window, true, 2.0))
            .collect(),
        Arc::new(super::super::standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        std::time::Instant::now(),
    );
}
fn content(plan: &[crate::surface::Surface], node: uuid::Uuid) -> crate::surface::Surface {
    *plan
        .iter()
        .find(|surface| surface.kind == SurfaceKind::Content(node))
        .expect("actual lens plan must admit the live native producer")
}

#[test]
fn native_surface_body_apertures_reserve_primary_and_lens_headers() {
    let (mut app, contents, node, _) = fixture(Paint::Frame);
    let primary = super::super::primary_content_plan(&app, &contents, 1200, 900);
    assert!(primary.iter().any(|surface| surface.kind == SurfaceKind::Frame));
    assert!(primary.iter().filter(|surface| matches!(surface.kind,
        SurfaceKind::Graph(_) | SurfaceKind::Pane(_))).all(|surface| surface.rect.y > 0.0));
    let identity = contents.get(&node).unwrap().session_identity();
    app.update(Action::TearOutActivePane);
    let lens = lens_content_plan(&app, &contents, 0, 800, 600);
    assert!(lens.iter().any(|surface| surface.kind == SurfaceKind::Frame));
    let body = content(&lens, node).rect;
    assert!(body.y > 0.0 && body.h > 0.0 && body.y + body.h <= 600.0);
    assert_eq!(contents.get(&node).unwrap().session_identity(), identity);
}
fn check_transfer(app: &mut crate::app::App, contents: &mut NodeDocuments, node: uuid::Uuid) {
    let before = super::super::primary_content_plan(app, contents, 1200, 900);
    let original = content(&before, node);
    sync(contents, &before, 0);
    let generation = contents.pool.placement_generation(&original.id).unwrap();
    let identity = contents.get(&node).unwrap().session_identity();
    app.update(Action::TearOutActivePane);
    let primary = super::super::primary_content_plan(app, contents, 1200, 900);
    assert!(
        !primary
            .iter()
            .any(|surface| surface.kind == SurfaceKind::Content(node)),
        "primary must withdraw the moved appearance"
    );
    let lens = lens_content_plan(app, contents, 0, 800, 600);
    let moved = content(&lens, node);
    assert_ne!(moved.id, original.id, "window-specific appearance identity");
    sync(contents, &lens, 1);
    assert!(!contents.pool.admits(&original.id, generation));
    let placed = contents.pool.placement(&moved.id).unwrap();
    assert_eq!(placed.window, 1);
    assert_eq!(
        placed.rect,
        pelt_core::WorkspaceRect::new(moved.rect.x, moved.rect.y, moved.rect.w, moved.rect.h)
    );
    assert_eq!(placed.scale_factor, 1.0);
    assert_eq!(contents.get(&node).unwrap().session_identity(), identity);
}
#[test]
fn native_surface_lens_workbench_moves_actual_placement_to_lens() {
    let (mut app, mut contents, node, _) = fixture(Paint::Frame);
    check_transfer(&mut app, &mut contents, node);
}
#[test]
fn native_surface_lens_pinned_tile_moves_actual_placement_to_lens() {
    let (mut app, mut contents, node, _) = fixture(Paint::Frame);
    app.update(Action::TearOutTile { member: node });
    let plan = lens_content_plan(&app, &contents, 0, 800, 600);
    let moved = content(&plan, node);
    assert!(
        !super::super::primary_content_plan(&app, &contents, 1200, 900)
            .iter()
            .any(|surface| surface.kind == SurfaceKind::Content(node))
    );
    sync(&mut contents, &plan, 1);
    assert_eq!(contents.pool.placement(&moved.id).unwrap().window, 1);
}
#[test]
fn native_surface_lens_floating_tile_keeps_actual_appearance() {
    let (mut app, mut contents, node, _) = fixture(Paint::Frame);
    app.update(Action::TearOutTile { member: node });
    app.update(Action::FloatActivePane);
    let plan = lens_content_plan(&app, &contents, 0, 800, 600);
    let moved = content(&plan, node);
    assert!(
        app.blueprint_space(crate::action::SpaceRef::Lens(0))
            .unwrap()
            .floating
            .iter()
            .any(|pane| Some(pane.pane) == app.active_pane)
    );
    sync(&mut contents, &plan, 1);
    assert_eq!(contents.pool.placement(&moved.id).unwrap().window, 1);
}
fn publish(
    forest: &mut crate::foreign_a11y::ForeignA11y,
    plan: &[crate::surface::Surface],
    focus: crate::surface::FocusTarget,
) -> Vec<TreeUpdate> {
    let mut frame = super::super::document_a11y::Projected {
        tree: uxtree::UxTree {
            root: NodeId(0),
            nodes: vec![(NodeId(0), Node::new(Role::Window))],
        },
        routes: Default::default(),
        focus: None,
        guests: Vec::new(),
        admissions: Default::default(),
    };
    let placements = super::super::foreign_a11y::foreign_placements(forest, focus, plan);
    let visible = append_foreign_lens(&mut frame, forest, placements, focus);
    forest
        .frame_with_guests(
            frame.tree.to_tree_update(frame.focus),
            &visible,
            &frame.guests,
        )
        .unwrap()
}
#[test]
fn native_surface_lens_actual_plan_controls_supplier_forest_action_and_retirement() {
    let (mut app, mut contents, node, events) = fixture(Paint::Frame);
    let mut primary_forest = crate::foreign_a11y::ForeignA11y::new();
    let producer = contents.surface_mut(&node).unwrap();
    primary_forest
        .admit(
            node,
            producer.set_accessibility_active(true).unwrap().unwrap(),
        )
        .unwrap();
    primary_forest
        .apply(node, producer.poll_accessibility_update().unwrap())
        .unwrap();
    let before = super::super::primary_content_plan(&app, &contents, 1200, 900);
    publish(&mut primary_forest, &before, app.focus);
    let request = || ActionRequest {
        action: A11yAction::Click,
        target_tree: supplier_id(),
        target_node: NodeId(1),
        data: None,
    };
    assert_eq!(primary_forest.action_target(&request()), Ok(node));
    app.update(Action::TearOutActivePane);
    let primary = super::super::primary_content_plan(&app, &contents, 1200, 900);
    let lens_plan = lens_content_plan(&app, &contents, 0, 800, 600);
    let mut lens_forest = crate::foreign_a11y::ForeignA11y::new();
    lens_forest.sync_retained_from(&primary_forest);
    publish(&mut primary_forest, &primary, app.focus);
    let updates = publish(&mut lens_forest, &lens_plan, app.focus);
    assert!(
        updates.iter().any(|update| update.tree_id == supplier_id()),
        "supplier tree must reach actual lens publication"
    );
    assert!(primary_forest.action_target(&request()).is_err());
    let primary_placements =
        super::super::foreign_a11y::foreign_placements(&primary_forest, app.focus, &primary);
    assert!(
        super::super::foreign_a11y::deliver_foreign_action(
            &mut contents,
            primary_forest.action_target(&request()),
            &primary_placements,
            request()
        )
        .is_err()
    );
    let lens_placements =
        super::super::foreign_a11y::foreign_placements(&lens_forest, app.focus, &lens_plan);
    assert_eq!(lens_forest.action_target(&request()), Ok(node));
    super::super::foreign_a11y::deliver_foreign_action(
        &mut contents,
        lens_forest.action_target(&request()),
        &lens_placements,
        request(),
    )
    .unwrap();
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| *event == "a11y Click")
            .count(),
        1
    );
    contents.remove(&node);
    primary_forest.retire(node);
    lens_forest.sync_retained_from(&primary_forest);
    let retired_plan = lens_content_plan(&app, &contents, 0, 800, 600);
    let updates = publish(&mut lens_forest, &retired_plan, app.focus);
    assert!(
        !updates
            .iter()
            .any(|update| update.tree_id == supplier_id() && !update.nodes.is_empty())
    );
    assert!(lens_forest.action_target(&request()).is_err());
    assert!(
        super::super::foreign_a11y::deliver_foreign_action(
            &mut contents,
            Ok(node),
            &lens_placements,
            request()
        )
        .is_err(),
        "retired producer refuses even an obsolete visible list"
    );
}
#[test]
fn native_surface_lens_render_seam_preserves_owned_empty_and_error_frames() {
    for paint in [Paint::Frame, Paint::Empty, Paint::Error] {
        let (app, mut contents, node, events) = fixture(paint);
        // The primary's actual plan supplies a placement before the lens plan repair.
        let plan = super::super::primary_content_plan(&app, &contents, 800, 600);
        let appearance = content(&plan, node);
        sync(&mut contents, &plan, 0);
        let layer = super::super::render::lens_content_frame(&mut contents.pool, &appearance.id);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| *event == "acquire")
                .count(),
            1
        );
        match (paint, layer) {
            (Paint::Frame, Some(PeltLayer::Surface(Ok(Some(frame))))) => {
                assert_eq!(
                    (frame.width, frame.height, frame.resource_epoch),
                    (800, 600, 47)
                );
                assert!(matches!(
                    frame.format,
                    inker::SurfaceTextureFormat::Bgra8Unorm
                ));
                assert!(matches!(frame.sync, inker::SurfaceSyncHandle::None));
                assert!(frame.texture.into_owned_payload::<OwnedPaint>().is_ok());
            },
            (Paint::Empty, Some(PeltLayer::Surface(Ok(None)))) => {},
            (Paint::Error, Some(PeltLayer::Surface(Err(reason)))) => assert_eq!(
                reason,
                "surface frame failed: frame acquisition: supplier capture refused"
            ),
            (_, _) => panic!("actual lens rendering seam discarded the surface layer"),
        }
    }
}
#[test]
fn native_surface_lens_document_plan_and_render_control_remains_live() {
    let (mut app, mut contents, node, _) = fixture(Paint::Frame);
    let engines = Arc::new(super::super::standard_content_engines());
    let surfaces = Arc::new(inker::SurfaceEngineRegistry::new());
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("lens-document-control"),
        view: None,
        node: None,
        address: "gemini://lens.fixture/".into(),
        content_type: None,
        pinned_engine: None,
    });
    let controller = pelt_core::PeltController::new_shared(
        engines,
        surfaces,
        pelt_core::PeltControllerConfig::from_request(
            decision.engine_id.clone(),
            inker::SessionSpawnRequest::new("gemini://lens.fixture/")
                .with_body("# Lens document\n=> /next Document continuation\n"),
        )
        .with_host_history(),
        super::super::ShellClock(std::time::Instant::now()),
    )
    .unwrap();
    contents.insert(
        node,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    check_transfer(&mut app, &mut contents, node);
    let plan = lens_content_plan(&app, &contents, 0, 800, 600);
    let placed = content(&plan, node);
    assert!(
        matches!(
            super::super::render::lens_content_frame(&mut contents.pool, &placed.id),
            Some(PeltLayer::Document(_))
        ),
        "existing document rendering stays admitted"
    );
}

#[test]
fn header_return_focus_preserves_primary_and_lens_actual_appearance() {
    let (mut app, contents, node, _) = fixture(Paint::Frame);
    let primary = super::super::primary_content_plan(&app, &contents, 1200, 900);
    let primary_body = content(&primary, node);
    let owner = super::super::frame_keys::HeaderFocus {
        slot: 0, session: *app.session_id.as_uuid(),
        return_pane: app.active_pane,
        return_focus: crate::surface::FocusTarget::Content { node, appearance: primary_body.id },
    };
    assert!(super::super::frame_keys::return_focus_is_valid(&app, &owner, app.active_pane, &primary));
    let identity = contents.get(&node).unwrap().session_identity();
    app.update(Action::TearOutActivePane);
    let lens = lens_content_plan(&app, &contents, 0, 800, 600);
    let lens_body = content(&lens, node);
    let mut owner = super::super::frame_keys::HeaderFocus {
        slot: 1, session: *app.session_id.as_uuid(), return_pane: app.active_pane,
        return_focus: crate::surface::FocusTarget::Content { node, appearance: lens_body.id },
    };
    assert!(super::super::frame_keys::return_focus_is_valid(&app, &owner, app.active_pane, &lens));
    assert!(!super::super::frame_keys::return_focus_is_valid(&app, &owner, app.active_pane, &[]));
    owner.session = uuid::Uuid::from_u128(app.session_id.as_uuid().as_u128().wrapping_add(1));
    assert!(!super::super::frame_keys::return_focus_is_valid(&app, &owner, app.active_pane, &lens));
    assert_eq!(contents.get(&node).unwrap().session_identity(), identity);
}
