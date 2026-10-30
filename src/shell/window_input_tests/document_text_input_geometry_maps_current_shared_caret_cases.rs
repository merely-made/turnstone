// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn document_text_input_geometry_maps_current_shared_caret_in_primary_and_lens() {
    fn editor() -> PeltContent<netrender::Scene> {
        let controller = pelt_core::PeltController::new(
            pelt_core::native::sessions(), inker::SurfaceEngineRegistry::new(),
            pelt_core::PeltControllerConfig::from_request(
                inker::routing::ENGINE_GENET_LIVERY,
                SessionSpawnRequest::new("https://fixture.test/caret")
                    .with_body("<!doctype html><body style='margin:0'><input aria-label='editor' value='a😀b' style='margin:20px;width:180px'>")
                    .with_content_type("text/html").with_viewport(300,200),
            ), super::super::super::ShellClock(std::time::Instant::now()),
        ).unwrap();
        PeltContent::from_controller(controller, route())
    }
    let focus = FocusTarget::Content {
        node: uuid::Uuid::from_u128(7),
        appearance: SurfaceId(10),
    };
    for window in [0, 1] {
        let mut pool = Pool::new();
        pool.insert(key(7), editor());
        let mut placement = placed(key(7), window, 40.0, true);
        placement.rect = pelt_core::WorkspaceRect::new(40.0, 70.0, 300.0, 200.0);
        placement.scale_factor = 2.0;
        pool.set_placements([(SurfaceId(10), placement.clone())]);
        assert!(document_text_input_area(&pool, focus, window).is_none());
        pool.try_frame(&SurfaceId(10)).unwrap();
        let controller = pool.content_mut(&key(7)).unwrap().document_mut().unwrap();
        let projection = controller.session().accessibility_projection().unwrap();
        let field = projection
            .nodes()
            .iter()
            .find(|n| n.name.as_deref() == Some("editor"))
            .unwrap();
        assert!(controller.session_mut().dispatch_accessibility_action(
            &inker::DocumentA11yActionRequest {
                revision: projection.revision(),
                target: field.id,
                action: inker::DocumentA11yAction::Focus,
                data: None,
            }
        ));
        pool.try_frame(&SurfaceId(10)).unwrap();
        let caret = pool.text_input_state(&SurfaceId(10)).unwrap().caret;
        let area = document_text_input_area(&pool, focus, window).unwrap();
        assert!((f64::from(area.x) - (40.0 + caret.x) * 2.0).abs() < 0.001);
        assert!((f64::from(area.y) - (70.0 + caret.y) * 2.0).abs() < 0.001);
        assert!((f64::from(area.h) - caret.height * 2.0).abs() < 0.001);
        assert!(document_text_input_area(&pool, focus, 1 - window).is_none());
        assert!(
            document_text_input_area(&pool, FocusTarget::Graph(crate::panes::PaneId(99)), window)
                .is_none()
        );
        let wrong_owner = FocusTarget::Content {
            node: uuid::Uuid::from_u128(8),
            appearance: SurfaceId(10),
        };
        assert!(document_text_input_area(&pool, wrong_owner, window).is_none());
        // Geometry from another appearance's viewport cannot be borrowed,
        // even when both appearances retain the same document receiver.
        let mut other = placement.clone();
        other.window = 1 - window;
        other.focused = false;
        other.rect.width = 240.0;
        pool.set_placements([(SurfaceId(10), placement.clone()), (SurfaceId(11), other)]);
        pool.try_frame(&SurfaceId(11)).unwrap();
        assert!(document_text_input_area(&pool, focus, window).is_none());
        pool.try_frame(&SurfaceId(10)).unwrap();
        assert!(document_text_input_area(&pool, focus, window).is_some());
        // A fresh edit immediately revokes old layout, until the producer frames.
        pool.input_at(
            window,
            PeltInput::Session(inker::SessionInput::Text("x".into())),
        );
        assert!(document_text_input_area(&pool, focus, window).is_none());
        pool.try_frame(&SurfaceId(10)).unwrap();
        assert!(document_text_input_area(&pool, focus, window).is_some());
        placement.visible = false;
        pool.set_placements([(SurfaceId(10), placement)]);
        assert!(document_text_input_area(&pool, focus, window).is_none());
        pool.insert(key(7), editor());
        assert!(document_text_input_area(&pool, focus, window).is_none());
        pool.remove_content(&key(7));
        assert!(document_text_input_area(&pool, focus, window).is_none());
    }
}
